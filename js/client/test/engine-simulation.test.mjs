// Deterministic simulation of an unreliable disk and an unreliable network.
//
// A port of rust/tests/engine_simulation_tests.rs. FoundationDB and
// TigerBeetle test their storage engines by running the real code against
// simulated disks and networks, driven by one seeded random source, so that
// every fault schedule replays exactly. This test does the same for the two
// places where the client depends on hardware it does not control.
//
// Disk. `SimDevice` implements the page device (and the repair device) over
// three simulated files (pages and two metadata slots). Each file keeps its
// durable bytes and the writes issued since its last sync. A simulated power
// loss keeps, for every 512-byte sector independently, the durable bytes or
// any prefix of the pending writes to that sector, sometimes half of the next
// one (a torn sector), and a file length from any prefix of the pending
// operations. That is every reordering and loss a volatile disk cache may
// perform within a flush interval. Writes and syncs also fail at random (the
// failed call may or may not have reached the cache). The real
// `PagedCommitLog` and the compiled engine run several clients on top; after
// every crash the test reopens the database and requires the exact committed
// state of one revision between the last acknowledged and the last attempted
// commit.
//
// A lying disk acknowledges syncs without persisting anything. Over such a
// disk no engine can keep its promise, so the test requires the next best
// thing: recovery either refuses the damaged database or returns the exact
// state of an earlier revision, never a mixture. A mirror that includes a
// lying disk must recover every acknowledged commit; with an honest primary
// it must also always open, because power loss alone never tears both copies
// of a metadata decision.
//
// Network. Several `ClusterEngine` members reach one atomic journal store
// through simulated links that drop requests, lose replies, deliver requests
// late (after other transactions ran), and are cut or made one-way by
// partitions. Clients on every member run random transactions over two
// shards. Every client observation goes into one history, which the
// Elle-style checker shared with the history conformance scenario searches for
// anomalies, including real-time edges between members: the cluster must be
// strictly serializable. Commits with unknown outcomes are resolved from the
// store's own log, as a client would read fresh journals after reconnecting.
//
// The simulated disk and the crash workload live in engine-simulation-disk.mjs,
// shared with engine-power-loss-backends.test.mjs. The engine is the Rust core
// compiled to WebAssembly, so this needs a `wasm-pack build --target web`
// package of rust/wasm: CLINK_WASM, or js/pkg. Without one the tests are
// skipped.
import test from 'node:test';
import assert from 'node:assert/strict';
import { ClusterEngine } from '../src/cluster.js';
import { PagedCommitLog } from '../src/engine-paged-log.js';
import { MirroredPageDevice } from '../src/engine-mirrored-pages.js';
import { check } from './conformance/history.mjs';
import {
  FIRST_WRITE,
  Disk,
  SimDevice,
  binding,
  below,
  crashRun,
  injected,
  rng,
  seeds,
  skip,
} from './engine-simulation-disk.mjs';

/** Different journal encodings, state structures and concurrency controls
 * write different pages; each must survive the same faults. */
const DISK_CONFIGURATIONS = [
  {},
  {
    concurrency: 'pessimistic',
    isolation: 'snapshot',
    stateStructure: 'shared',
    revisionEncoding: 'changes',
  },
  {
    serializableValidation: 'read-set',
    stateStructure: 'sorted-runs',
    revisionEncoding: 'changes',
    history: 4,
  },
  { concurrency: 'exclusive', requireDurableFlush: true },
];

const DISK_SEEDS = 24n;
const CYCLES = 8;

function single(seed, options, lies, errors) {
  const disk = new Disk(seed * 31n, lies, errors);
  return crashRun(seed, options, CYCLES, () => new PagedCommitLog(new SimDevice(disk)), [disk]);
}

test(
  'power loss over a reordering disk recovers exactly an acknowledged or attempted commit',
  { skip },
  async (t) => {
    for (const options of DISK_CONFIGURATIONS) {
      const sum = await seeds(DISK_SEEDS, (seed) => single(seed, options, 0, 15));
      const context = `${JSON.stringify(options)}: ${JSON.stringify(sum)}`;
      t.diagnostic(context);
      // An honest disk never loses an acknowledged commit or refuses to open.
      assert.deepEqual([sum.refused, sum.lost], [0, 0], context);
      // The faults happened: unsynced sectors were lost and torn, and failed
      // flushes recovered both ways.
      assert.ok(sum.commits > 250, context);
      assert.ok(sum.dropped > 20 && sum.torn > 4, context);
      assert.ok(sum.unknownCommitted > 0 && sum.unknownLost > 0, context);
    }
  },
);

test(
  'a lying disk is detected or rolls back to an exact earlier revision, never a mixture',
  { skip },
  async (t) => {
    const sum = await seeds(DISK_SEEDS, (seed) => single(seed, {}, 500, 0));
    t.diagnostic(JSON.stringify(sum));
    // Acknowledged commits are lost, and some recoveries refuse the damage;
    // `crashRun` already checked that every opened state is exact.
    assert.ok(sum.lied > 100, JSON.stringify(sum));
    assert.ok(sum.lost > 0 && sum.refused > 0, JSON.stringify(sum));
  },
);

test('a mirror of honest and lying disks keeps every acknowledged commit', { skip }, async (t) => {
  // [primary lies, secondary lies]
  for (const lies of [
    [false, false],
    [false, true],
    [true, false],
  ]) {
    const sum = await seeds(DISK_SEEDS, (seed) => {
      const first = new Disk(seed * 31n, lies[0] ? 500 : 0, 10);
      const second = new Disk(seed * 37n, lies[1] ? 500 : 0, 10);
      return crashRun(
        seed,
        {},
        CYCLES,
        () =>
          new PagedCommitLog(new MirroredPageDevice(new SimDevice(first), new SimDevice(second))),
        [first, second],
      );
    });
    const context = `${lies}: ${JSON.stringify(sum)}`;
    t.diagnostic(context);
    assert.equal(sum.lost, 0, context);
    assert.ok(sum.commits > 300, context);
    assert.equal(sum.lied > 100, lies[0] || lies[1], context);
    // The secondary copy of a decision is written only after the primary
    // copy is durable, so power loss alone never tears both copies. Only a
    // primary that lies about its sync can leave both torn; that second
    // fault is refused, never a silent rollback.
    if (!lies[0]) assert.equal(sum.refused, 0, context);
  }
});

test('the same seed replays the same crashes', { skip }, async () => {
  assert.deepEqual(await single(7n, {}, 300, 15), await single(7n, {}, 300, 15));
  assert.notDeepEqual(await single(7n, {}, 300, 15), await single(8n, {}, 300, 15));
});

// ---------------------------------------------------------------------------
// Simulated network
// ---------------------------------------------------------------------------

/** Up; Cut: nothing gets through; OneWay: requests arrive, replies are lost. */
const UP = 'up';
const CUT = 'cut';
const ONE_WAY = 'one-way';

/** The atomic store and every member's link to it. */
class Network {
  faults = false;
  links = [];
  revision = 0;
  /** key -> { generation, entries }, as `AtomicJournalStore.read` returns them. */
  journals = new Map();
  /** The transaction id of every applied compare-exchange, by revision. */
  applied = new Map();
  /** Requests the network holds back and delivers later. */
  delayed = [];
  /** The transaction a member is committing; the store logs it. */
  caller = 0;
  deliveredLate = 0;
  constructor(seed, members) {
    this.random = rng(seed * 7919n);
    this.links = Array(members).fill(UP);
  }
  apply({ keys, expected, updates, caller }) {
    if (
      keys.some(
        (key, at) => (this.journals.get(key)?.generation ?? '0') !== expected[at].generation,
      )
    )
      return false;
    const generation = ++this.revision;
    keys.forEach((key, at) => {
      if (updates[at] !== null)
        this.journals.set(key, { generation: String(generation), entries: [...updates[at]] });
    });
    this.applied.set(generation, caller);
    return true;
  }
  deliverOneLate() {
    const request = this.delayed.shift();
    if (request && this.apply(request)) this.deliveredLate++;
  }
  /** Whether the request reaches the store. */
  send(member) {
    return !this.faults || (this.links[member] !== CUT && !this.random.perMille(40));
  }
  /** Whether the reply reaches the member. */
  reply(member) {
    return !this.faults || (this.links[member] === UP && !this.random.perMille(40));
  }
}

/** One member's atomic journal store: the shared store behind its own link. */
class Member {
  supportsDurableFlush = false;
  constructor(network, id) {
    this.network = network;
    this.id = id;
  }
  async read(keys) {
    const network = this.network;
    if (!network.send(this.id)) throw injected('dropped request');
    const values = keys.map((key) =>
      structuredClone(network.journals.get(key) ?? { generation: '0', entries: [] }),
    );
    if (!network.reply(this.id)) throw injected('lost reply');
    return values;
  }
  async compareExchange(keys, expected, updates) {
    const network = this.network;
    const request = {
      keys: [...keys],
      expected: structuredClone(expected),
      updates: structuredClone(updates),
      caller: network.caller,
    };
    if (!network.send(this.id)) throw injected('dropped request');
    if (network.faults && network.random.perMille(60)) {
      // A slow link: the client times out; the request arrives later.
      network.delayed.push(request);
      throw injected('timeout');
    }
    const applied = network.apply(request);
    if (!network.reply(this.id)) throw injected('lost reply');
    return applied;
  }
}

const SHARDS = ['a', 'b'];
const SHARD_LINKS = 2n;
const MEMBERS = 3;
const NET_CLIENTS = 5;
const NET_TRANSACTIONS = 80;
const NET_SEEDS = 30n;

/** One key per link of every shard, as the checker sees it. */
const key = (shard, link) => 10n * BigInt(shard + 1) + BigInt(link);

const transaction = (began = 0) => ({
  reads: [],
  writes: [],
  ownReads: [],
  revision: undefined,
  began,
  ended: 0,
});

function observe(record, link, target) {
  const own = record.writes.findLast(([written]) => written === link);
  if (own) record.ownReads.push([link, target, own[1]]);
  else record.reads.push([link, target]);
}

const ACKNOWLEDGED = 0;
const REFUSED = 1;
const UNKNOWN = 2;

/** A history of `NET_TRANSACTIONS` transactions by clients spread over
 * `MEMBERS` cluster members. */
async function networkHistory(seed) {
  const network = new Network(seed, MEMBERS);
  // One member rewrites journals as checkpoints; the others read them.
  const members = Array.from(
    { length: MEMBERS },
    (_, id) =>
      new ClusterEngine(
        binding,
        new Member(network, id),
        id === 1 ? { journalCheckpointInterval: 3 } : {},
      ),
  );
  // Transaction 0 installs the initial values.
  const initial = transaction();
  const setup = await members[0].begin(SHARDS);
  for (const [shard, name] of SHARDS.entries())
    for (let link = 1n; link <= SHARD_LINKS; link++) {
      const session = setup.shard(name);
      assert.equal((await session.create(0n, 0n)).index, link);
      await session.update(link, link, key(shard, link));
      initial.writes.push([key(shard, link), key(shard, link)]);
    }
  await setup.commit();
  const records = [initial];
  const outcomes = [ACKNOWLEDGED];
  network.faults = true;
  const source = rng(seed);
  const stats = {
    committed: 0,
    conflicts: 0,
    unknownCommitted: 0,
    unknownAborted: 0,
    deliveredLate: 0,
    unavailable: 0,
  };
  const clients = Array(NET_CLIENTS).fill(undefined);
  let started = 0;
  let finished = 0;
  let now = 0;
  let value = BigInt(FIRST_WRITE);
  while (finished < NET_TRANSACTIONS) {
    now++;
    if (source.below(25n) === 0n) {
      const member = below(source, MEMBERS);
      network.links[member] = [CUT, ONE_WAY, UP, UP][below(source, 4)];
    }
    if (source.below(6n) === 0n) network.deliverOneLate();
    const index = below(source, NET_CLIENTS);
    if (clients[index] === undefined) {
      if (started === NET_TRANSACTIONS) continue;
      started++;
      const shards = [[0], [1], [0, 1]][below(source, 3)];
      const ops = [];
      for (let count = 1 + below(source, 4); count > 0; count--) {
        const shard = shards[below(source, shards.length)];
        const link = 1n + source.below(SHARD_LINKS);
        const kind = below(source, 10);
        ops.push(
          kind <= 3
            ? { read: [shard, link] }
            : kind <= 7
              ? { write: [shard, link] }
              : { scan: shard },
        );
      }
      ops.push(source.below(20n) === 0n ? { rollback: true } : { commit: true });
      records.push(transaction(now));
      outcomes.push(REFUSED);
      clients[index] = {
        member: index % MEMBERS,
        id: records.length - 1,
        shards,
        session: undefined,
        ops,
      };
    }
    const client = clients[index];
    const record = records[client.id];
    if (client.session === undefined) {
      try {
        client.session = await members[client.member].begin(
          client.shards.map((shard) => SHARDS[shard]),
        );
      } catch {
        // The member could not reach the store: nothing happened.
        stats.unavailable++;
        record.ended = now;
        clients[index] = undefined;
        finished++;
        continue;
      }
    }
    const session = client.session;
    const op = client.ops.shift();
    let done = Boolean(op.commit || op.rollback);
    if (op.read) {
      const [shard, link] = op.read;
      const [found] = await session.shard(SHARDS[shard]).each([link]);
      observe(record, key(shard, link), found.target);
    } else if (op.scan !== undefined) {
      for (const found of await session.shard(SHARDS[op.scan]).each())
        observe(record, key(op.scan, found.index), found.target);
    } else if (op.write) {
      const [shard, link] = op.write;
      value++;
      await session.shard(SHARDS[shard]).update(link, link, value);
      record.writes.push([key(shard, link), value]);
    } else if (op.commit) {
      network.caller = client.id;
      try {
        await session.commit();
        outcomes[client.id] = ACKNOWLEDGED;
      } catch (error) {
        if (String(error).includes('conflict')) stats.conflicts++;
        else {
          assert.match(String(error), /indeterminate/);
          outcomes[client.id] = UNKNOWN;
        }
      }
    } else await session.rollback();
    if (client.ops.length === 0) done = true;
    if (done) {
      record.ended = now;
      clients[index] = undefined;
      finished++;
    }
  }
  // Heal the network; requests still in flight arrive.
  while (network.delayed.length > 0) network.deliverOneLate();
  network.faults = false;
  stats.deliveredLate = network.deliveredLate;
  for (const [revision, id] of [...network.applied].sort(([a], [b]) => a - b)) {
    const record = records[id];
    record.revision = BigInt(revision);
    if (outcomes[id] === ACKNOWLEDGED) stats.committed++;
    else if (outcomes[id] === UNKNOWN) {
      // Unknown to the client: it could have ended any time later.
      record.ended = Infinity;
      stats.unknownCommitted++;
    } else assert.fail(`seed ${seed}: refused transaction ${id} was applied`);
  }
  outcomes.forEach((outcome, id) => {
    if (id === 0 || records[id].revision !== undefined) return;
    if (outcome === ACKNOWLEDGED)
      assert.fail(`seed ${seed}: acknowledged transaction ${id} is not applied`);
    if (outcome === UNKNOWN) stats.unknownAborted++;
  });
  // Read the final state through a healed member.
  const last = await members[2].begin(SHARDS);
  const finalState = new Map();
  for (const [shard, name] of SHARDS.entries())
    for (const link of await last.shard(name).each())
      finalState.set(key(shard, link.index), link.target);
  await last.close();
  return { history: { transactions: records, finalState }, stats };
}

test('cluster members over a faulty network stay strictly serializable', { skip }, async (t) => {
  const sum = {
    committed: 0,
    conflicts: 0,
    unknownCommitted: 0,
    unknownAborted: 0,
    deliveredLate: 0,
    unavailable: 0,
  };
  const shardsPerCommit = new Set();
  for (let seed = 1n; seed <= NET_SEEDS; seed++) {
    const { history, stats } = await networkHistory(seed);
    const anomalies = check(history);
    assert.equal(anomalies.size, 0, `seed ${seed}: ${JSON.stringify([...anomalies])}`);
    for (const { writes, revision } of history.transactions)
      if (revision !== undefined)
        shardsPerCommit.add(new Set(writes.map(([written]) => written / 10n)).size);
    for (const name of Object.keys(sum)) sum[name] += stats[name];
  }
  // Every kind of outcome happened, including cross-shard commits, late
  // deliveries that still committed, and unknown outcomes both ways.
  const context = JSON.stringify(sum);
  t.diagnostic(`${context} shards per commit ${JSON.stringify([...shardsPerCommit])}`);
  assert.ok(shardsPerCommit.has(2), JSON.stringify([...shardsPerCommit]));
  assert.ok(sum.committed > 500 && sum.conflicts > 50, context);
  assert.ok(sum.unknownCommitted > 0 && sum.unknownAborted > 0, context);
  assert.ok(sum.deliveredLate > 0 && sum.unavailable > 0, context);
});

test('the same seed replays the same network history', { skip }, async () => {
  const text = (value) =>
    JSON.stringify(value, (_, item) =>
      typeof item === 'bigint' ? `${item}` : item instanceof Map ? [...item] : item,
    );
  const first = await networkHistory(3n);
  const again = await networkHistory(3n);
  assert.equal(text(first.history.transactions), text(again.history.transactions));
  assert.deepEqual(first.stats, again.stats);
});
