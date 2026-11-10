// Real network partitions against a real three-member etcd cluster.
// ETCD_BIN=/path/to/etcd PARTITION_SEED=138 node experiments/issue-138/etcd-partition.mjs
//
// Peer traffic runs through `PeerNetwork` proxies (etcd-peer-network.mjs), so
// links can be cut both ways or only one way while clients keep talking to
// every member directly. Clients run `ClusterEngine` transactions through
// single members and through all of them. Each transaction creates one named
// link in each of two shards. After the network heals, the final state is
// checked against every outcome a client saw:
//
// - an acknowledged transaction is present in both shards (durability);
// - a transaction refused as failed, or that never started, is absent;
// - an unknown outcome is present in both shards or in neither (atomicity);
// - every member serves the same state;
// - a transaction that began after another was acknowledged saw at least
//   every transaction acknowledged before it began (no stale reads).
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createWriteStream, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import net from 'node:net';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { ClusterEngine, EtcdJournalStore } from '../../js/client/src/cluster.js';
import { waitForMembers } from './etcd-harness.mjs';
import { PeerNetwork } from './etcd-peer-network.mjs';

const wasmDirectory = resolve(process.env.CLINK_WASM ?? 'js/pkg');
const binding = await import(pathToFileURL(join(wasmDirectory, 'clink_wasm.js')).href);
binding.initSync({
  module: readFileSync(join(wasmDirectory, 'clink_wasm_bg.wasm')),
});
const binary =
  process.env.ETCD_BIN ?? resolve('experiments/issue-138/etcd/etcd-v3.7.3-linux-amd64/etcd');
const seed = BigInt(process.env.PARTITION_SEED ?? 138);
const rounds = Number(process.env.PARTITION_ROUNDS ?? 6);
const directory = mkdtempSync(resolve('experiments/issue-138/etcd/partition-'));
const size = 3;
// PARTITION_TRACE=1 logs every leader poll and network change to stderr.
const trace = process.env.PARTITION_TRACE
  ? (text) => console.error(`trace ${new Date().toISOString().slice(11)}: ${text}`)
  : () => {};
const delay = (ms) => new Promise((done) => setTimeout(done, ms));

// splitmix64, as in the simulation tests.
let state = seed;
function next() {
  state = (state + 0x9e3779b97f4a7c15n) & 0xffffffffffffffffn;
  let z = state;
  z = ((z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n) & 0xffffffffffffffffn;
  z = ((z ^ (z >> 27n)) * 0x94d049bb133111ebn) & 0xffffffffffffffffn;
  return z ^ (z >> 31n);
}
const below = (n) => Number(next() % BigInt(n));

async function freePorts(count) {
  const servers = [];
  try {
    for (let i = 0; i < count; i++) {
      const server = net.createServer();
      servers.push(server);
      await new Promise((done, reject) => {
        server.once('error', reject);
        server.listen(0, '127.0.0.1', done);
      });
    }
    return servers.map((server) => server.address().port);
  } finally {
    await Promise.all(servers.map((server) => new Promise((done) => server.close(done))));
  }
}

const ports = await freePorts(2 * size);
const endpoints = ports.slice(0, size).map((port) => `http://127.0.0.1:${port}`);
const network = await PeerNetwork.start(ports.slice(size), { trace });
const advertised = network.ports.map((port) => `http://127.0.0.1:${port}`);
const cluster = advertised.map((peer, i) => `n${i}=${peer}`).join(',');
const nodes = [];
let passed = false;

async function status(i) {
  const response = await fetch(`${endpoints[i]}/v3/maintenance/status`, {
    method: 'POST',
    body: '{}',
    signal: AbortSignal.timeout(1000),
  });
  return response.json();
}
async function until(what, check, attempts = 100) {
  let failure;
  for (let attempt = 0; attempt < attempts; attempt++) {
    try {
      const result = await check();
      if (result !== undefined && result !== false) return result;
      failure = new Error(`waiting for ${what}`);
    } catch (error) {
      failure = error;
    }
    await delay(100);
  }
  throw failure;
}
let ids = [];
// The member that member `i` follows, as an index, or null.
async function leaderSeenBy(i) {
  const { leader, header } = await status(i);
  const index = ids.indexOf(leader);
  trace(`n${i} follows ${index < 0 ? 'none' : `n${index}`} in term ${header?.raft_term}`);
  return index < 0 ? null : index;
}

const options = { prefix: '/partition/', timeoutMs: 700 };
const engines = new Map();
function engineThrough(members) {
  const key = members.join(',');
  if (!engines.has(key))
    engines.set(
      key,
      new ClusterEngine(
        binding,
        new EtcdJournalStore(
          members.map((i) => endpoints[i]),
          options,
        ),
        // Checkpoints keep each journal value small; without them every commit
        // stores the whole history and the backend quota runs out.
        { requireDurableFlush: true, journalCheckpointInterval: 8 },
      ),
    );
  return engines.get(key);
}

const attempts = [];
let sequence = 0;
// One transaction: a named link in each shard. Records what the client saw.
async function attempt(members) {
  const record = { name: `t${sequence++}`, members, start: performance.now() };
  attempts.push(record);
  let transaction;
  try {
    transaction = await engineThrough(members).begin(['a', 'b']);
    record.observed = Number(await transaction.shard('a').count());
  } catch {
    record.outcome = 'not-started';
    record.end = performance.now();
    return record;
  }
  try {
    for (const key of ['a', 'b']) {
      const link = await transaction.shard(key).create(0, 0);
      await transaction.shard(key).setName(link.index, record.name);
    }
    await transaction.commit();
    record.outcome = 'ok';
  } catch (error) {
    record.outcome = ['failed', 'not-sent'].includes(error.outcome) ? 'failed' : 'unknown';
    record.error = `${error.message}${error.cause ? `: ${error.cause.message ?? error.cause}` : ''}`;
  } finally {
    record.end = performance.now();
    await transaction.close().catch(() => {});
  }
  return record;
}
const outcomes = (records) =>
  Object.fromEntries(
    ['ok', 'failed', 'unknown', 'not-started'].map((outcome) => [
      outcome,
      records.filter((record) => record.outcome === outcome).length,
    ]),
  );
// Run attempts through `members` until `count` are acknowledged.
async function commitThrough(members, count) {
  const records = [];
  while (records.filter((record) => record.outcome === 'ok').length < count) {
    records.push(await attempt(members));
    trace(
      `${records.at(-1).name} through n${members.join(',n')}: ${records.at(-1).outcome} ${records.at(-1).error ?? ''}`,
    );
    assert.ok(records.length < 200, `no progress through n${members.join(',n')}`);
    if (records.at(-1).outcome !== 'ok') await delay(100);
  }
  return records;
}
async function heal() {
  network.heal();
  await until('every member to follow one leader', async () => {
    const leaders = await Promise.all(endpoints.map((_, i) => leaderSeenBy(i)));
    return leaders[0] !== null && leaders.every((leader) => leader === leaders[0]);
  });
  // A commit through every member, so each has caught up with the others.
  for (let i = 0; i < size; i++) await commitThrough([i], 1);
}

try {
  for (let i = 0; i < size; i++) {
    const log = createWriteStream(join(directory, `n${i}.log`));
    const child = spawn(
      binary,
      [
        '--name',
        `n${i}`,
        '--data-dir',
        join(directory, `n${i}`),
        '--listen-client-urls',
        endpoints[i],
        '--advertise-client-urls',
        endpoints[i],
        '--listen-peer-urls',
        `http://127.0.0.1:${ports[size + i]}`,
        '--initial-advertise-peer-urls',
        advertised[i],
        '--initial-cluster',
        cluster,
        '--initial-cluster-token',
        `issue-138-partition-${ports[0]}`,
        '--quota-backend-bytes',
        '67108864',
        '--auto-compaction-mode',
        'revision',
        '--auto-compaction-retention',
        '100',
        '--log-level',
        process.env.ETCD_LOG_LEVEL ?? 'warn',
      ],
      {
        env: { ...process.env, GOMEMLIMIT: '100MiB' },
        stdio: ['ignore', 'pipe', 'pipe'],
      },
    );
    child.stdout.pipe(log);
    child.stderr.pipe(log);
    nodes.push({ child, exited: once(child, 'exit'), log });
  }
  await waitForMembers(endpoints);
  const { members } = await (
    await fetch(`${endpoints[0]}/v3/cluster/member/list`, {
      method: 'POST',
      body: '{}',
    })
  ).json();
  ids = endpoints.map((_, i) => members.find((member) => member.name === `n${i}`).ID);
  network.learn(ids);
  // Peer connections opened before `learn` are classified now; reconnect them
  // through a heal so the first partition cuts them like any other.
  await heal();

  // 1. The leader cut off both ways. The two others elect a new leader and
  // commit; nothing commits through the old leader; a transaction staged
  // through it before the cut is refused after the heal.
  {
    const leader = await until('a leader', () => leaderSeenBy(0));
    const stale = await engineThrough([leader]).begin(['a', 'b']);
    await stale.shard('a').create(0, 0);
    network.isolate([leader], size);
    const others = [0, 1, 2].filter((i) => i !== leader);
    await commitThrough(others, 3);
    const elected = await until('a new leader among the majority', () =>
      leaderSeenBy(others[0]).then((l) => l !== null && l !== leader && l),
    );
    const cut = [await attempt([leader]), await attempt([leader])];
    assert.ok(
      cut.every((record) => record.outcome !== 'ok'),
      'nothing commits through a member cut off from the majority',
    );
    await heal();
    await assert.rejects(stale.commit(), (error) => ['failed', 'unknown'].includes(error.outcome));
    console.log(
      `PASS isolated leader n${leader}: n${elected} elected, majority committed, the cut member committed nothing, a stale transaction was refused`,
    );
  }

  // 2. One-way: the leader's messages are lost, but it still hears the
  // others. They elect a new leader, whose messages depose the old one.
  {
    const leader = await until('a leader', () => leaderSeenBy(0));
    const others = [0, 1, 2].filter((i) => i !== leader);
    network.partition(others.map((other) => [leader, other]));
    await commitThrough(others, 3);
    const deposed = await until('the old leader to follow the new one', async () => {
      const seen = await leaderSeenBy(leader);
      return seen !== null && seen !== leader && seen;
    });
    await heal();
    console.log(
      `PASS one-way cut from leader n${leader}: it hears n${deposed} lead, the majority committed`,
    );
  }

  // 3. One-way: the leader is heard, but hears nobody. Check-quorum makes it
  // step down; the others elect a leader whose messages it never receives.
  {
    const leader = await until('a leader', () => leaderSeenBy(0));
    const others = [0, 1, 2].filter((i) => i !== leader);
    network.partition(others.map((other) => [other, leader]));
    await commitThrough(others, 3);
    const elected = await until('a new leader among the majority', () =>
      leaderSeenBy(others[0]).then((l) => l !== null && l !== leader && l),
    );
    await heal();
    console.log(
      `PASS one-way cut to leader n${leader}: n${elected} elected, the majority committed`,
    );
  }

  // 4. A bridge: two members cannot reach each other, both reach the third.
  {
    const [a, b] = [below(size), below(size - 1)];
    const ends = [a, b >= a ? b + 1 : b];
    const bridge = [0, 1, 2].find((i) => !ends.includes(i));
    network.partition([
      [ends[0], ends[1]],
      [ends[1], ends[0]],
    ]);
    await commitThrough([bridge], 3);
    await commitThrough([0, 1, 2], 3);
    await heal();
    console.log(
      `PASS bridge through n${bridge}: n${ends[0]} and n${ends[1]} cut, commits continue`,
    );
  }

  // 5. Random partitions under concurrent clients (Jepsen's partition
  // nemesis), with one client per member and one using every member.
  const faults = [
    () => network.isolate([below(size)], size),
    () => {
      const from = below(size);
      network.partition([[from, (from + 1 + below(size - 1)) % size]]);
    },
    () => {
      const to = below(size);
      network.partition([0, 1, 2].filter((i) => i !== to).map((from) => [from, to]));
    },
    () => {
      const from = below(size);
      network.partition([0, 1, 2].filter((i) => i !== from).map((to) => [from, to]));
    },
    () => network.heal(),
  ];
  for (let round = 0; round < rounds; round++) {
    // A second fault lands mid-round, while commits are in flight, so some
    // clients lose the reply to a commit and must report an unknown outcome.
    const [first, second] = [faults[below(faults.length)], faults[below(faults.length)]];
    const at = 500 + below(1500);
    first();
    const cuts = [network.cuts.join(' ') || 'none'];
    const timer = setTimeout(() => {
      second();
      cuts.push(network.cuts.join(' ') || 'none');
    }, at);
    const deadline = performance.now() + 3000;
    const clients = [[0], [1], [2], [0, 1, 2]].map(async (members) => {
      const records = [];
      while (performance.now() < deadline) {
        records.push(await attempt(members));
        if (records.at(-1).outcome !== 'ok') await delay(50);
      }
      return records;
    });
    const records = (await Promise.all(clients)).flat();
    clearTimeout(timer);
    console.log(
      `round ${round}: cuts ${cuts.join(' | ')} at ${at} ms: ${JSON.stringify(outcomes(records))}`,
    );
    await heal();
  }

  // The final state, read through each member alone, against every outcome.
  const states = [];
  for (let i = 0; i < size; i++) {
    const transaction = await until(`a read through n${i}`, () =>
      engineThrough([i]).begin(['a', 'b']),
    );
    const present = new Map();
    for (const key of ['a', 'b']) {
      const names = new Set();
      for (const link of await transaction.shard(key).each()) {
        const name = await transaction.shard(key).getName(link.index);
        if (name !== null) names.add(name);
      }
      present.set(key, names);
    }
    await transaction.close();
    states.push(present);
  }
  const final = states[0];
  for (const state of states.slice(1))
    for (const key of ['a', 'b']) assert.deepEqual(state.get(key), final.get(key));
  assert.deepEqual(
    final.get('a'),
    final.get('b'),
    'every transaction is in both shards or neither',
  );
  const known = new Set(attempts.map((record) => record.name));
  for (const name of final.get('a')) assert.ok(known.has(name), `unexpected link ${name}`);
  for (const record of attempts) {
    const applied = final.get('a').has(record.name);
    if (record.outcome === 'ok') assert.ok(applied, `acknowledged ${record.name} was lost`);
    if (record.outcome === 'failed' || record.outcome === 'not-started')
      assert.ok(!applied, `${record.outcome} ${record.name} was applied`);
  }
  // Real time: a transaction sees everything acknowledged before it began.
  const acknowledged = attempts
    .filter((record) => record.outcome === 'ok')
    .map((record) => record.end)
    .sort((x, y) => x - y);
  for (const record of attempts) {
    if (record.observed === undefined) continue;
    let before = 0;
    while (before < acknowledged.length && acknowledged[before] < record.start) before++;
    assert.ok(
      record.observed >= before,
      `${record.name} through n${record.members.join(',n')} saw ${record.observed} links, but ${before} were acknowledged before it began`,
    );
  }
  assert.deepEqual(network.strangers, [], 'every peer connection named a known member');
  console.log(
    `PASS partitions (seed ${seed}): ${JSON.stringify(outcomes(attempts))}; ${final.get('a').size} applied; every member agrees, acknowledged kept, failed absent, unknown atomic, no stale reads`,
  );
  passed = true;
} finally {
  await network.close();
  await Promise.all(
    nodes.map(async (node) => {
      if (node.child.exitCode === null && node.child.signalCode === null)
        node.child.kill('SIGKILL');
      await node.exited;
    }),
  );
  for (const node of nodes) node.log.end();
  // Keep the logs; the data directories (mostly preallocated WAL) only on failure.
  if (passed) for (let i = 0; i < size; i++) rmSync(join(directory, `n${i}`), { recursive: true });
  console.log(`etcd evidence: ${directory}`);
}
