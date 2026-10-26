// Bounded real consensus integration. All children are killed and reaped.
// ETCD_BIN=/path/to/etcd node experiments/issue-138/etcd-cluster.mjs
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createWriteStream, mkdtempSync, readFileSync } from 'node:fs';
import net from 'node:net';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { ClusterEngine, EtcdJournalStore } from '../../js/client/src/cluster.js';
import { waitForMembers } from './etcd-harness.mjs';
import { PartitionedJournalStore } from '../../js/client/src/partitioned.js';

const wasmDirectory = resolve(process.env.CLINK_WASM ?? 'js/pkg');
const binding = await import(pathToFileURL(join(wasmDirectory, 'clink_wasm.js')).href);
binding.initSync({ module: readFileSync(join(wasmDirectory, 'clink_wasm_bg.wasm')) });
const binary = process.env.ETCD_BIN ?? resolve('experiments/issue-138/etcd/etcd-v3.7.3-linux-amd64/etcd');
const directory = mkdtempSync(resolve('experiments/issue-138/etcd/run-'));
const checkpoint = process.env.CLUSTER_JOURNAL_CHECKPOINT_INTERVAL === '1';
const checkpointFlags = checkpoint ? ['--checkpoint'] : [];
const checkpointOptions = checkpoint ? { journalCheckpointInterval: 1 } : {};
const ports = [];
const reservations = [];
try {
  for (let i = 0; i < 10; i++) {
    const server = net.createServer(); reservations.push(server);
    await new Promise((done, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', done); });
    ports.push(server.address().port);
  }
} finally { await Promise.all(reservations.map((server) => new Promise((done) => server.close(done)))); }
const endpoints = ports.slice(0, 3).map((port) => `http://127.0.0.1:${port}`);
const peers = ports.slice(3, 6).map((port) => `http://127.0.0.1:${port}`);
const cluster = peers.map((peer, i) => `n${i}=${peer}`).join(',');
const nodes = [];
const delay = (ms) => new Promise((done) => setTimeout(done, ms));
async function stop(node) {
  if (node.child.exitCode === null && node.child.signalCode === null) node.child.kill('SIGKILL');
  await node.exited;
}
async function ready(action) {
  let failure;
  for (let attempt = 0; attempt < 30; attempt++) { try { return await action(); } catch (error) { failure = error; await delay(100); } }
  throw failure;
}
async function javascriptExample(args, engine) {
  const child = spawn(process.execPath, ['js/client/examples/cluster.mjs', ...args, ...checkpointFlags], {
    env: { ...process.env, CLINK_WASM: wasmDirectory }, stdio: 'inherit',
  });
  assert.equal((await once(child, 'exit'))[0], 0);
  const read = await engine.begin(['a', 'b']);
  try { for (const key of ['a', 'b']) assert.equal(await read.shard(key).getName(1), 'JS coordinated 雪'); }
  finally { await read.close(); }
  console.log('PASS public JS HTTP example: native journals reopen through actual WASM');
}
try {
  for (let i = 0; i < 3; i++) {
    const log = createWriteStream(join(directory, `n${i}.log`));
    const child = spawn(binary, ['--name', `n${i}`, '--data-dir', join(directory, `n${i}`),
      '--listen-client-urls', endpoints[i], '--advertise-client-urls', endpoints[i],
      '--listen-peer-urls', peers[i], '--initial-advertise-peer-urls', peers[i],
      '--initial-cluster', cluster, '--initial-cluster-token', `issue-138-${ports[0]}`,
      '--quota-backend-bytes', '16777216', '--snapshot-count', '1000', '--log-level', 'warn'],
      { env: { ...process.env, GOMEMLIMIT: '100MiB' }, stdio: ['ignore', 'pipe', 'pipe'] });
    child.stdout.pipe(log); child.stderr.pipe(log);
    nodes.push({ child, exited: once(child, 'exit'), log });
  }
  const options = { prefix: '/issue138/', timeoutMs: 700 };
  await waitForMembers(endpoints);
  const store = new EtcdJournalStore(endpoints, options);
  await ready(() => store.read(['a']));
  const engine = new ClusterEngine(binding, store, { requireDurableFlush: true, ...checkpointOptions });
  const seed = await engine.begin(['a', 'b']);
  await seed.shard('a').create(0, 0); await seed.shard('b').create(0, 0); await seed.commit();
  if (checkpoint) for (const value of await store.read(['a', 'b'])) {
    assert.equal(value.entries.length, 1); assert.ok(value.entries[0].startsWith('engine3|'));
  }
  const stale = await engine.begin(['a', 'b']);
  const other = new ClusterEngine(binding, new EtcdJournalStore(endpoints, options), checkpointOptions);
  const fresh = await other.begin(['b']); await fresh.shard('b').setName(1, 'leader-safe 雪'); await fresh.commit();
  await stale.shard('a').delete(1); await assert.rejects(stale.commit(), { outcome: 'failed' });
  console.log('PASS real etcd: atomic multi-shard CAS and read-only dependency conflict');

  let loseReply = true;
  const lossy = new EtcdJournalStore(endpoints, { ...options, fetch: async (url, request) => {
    const result = await fetch(url, request);
    if (loseReply && JSON.parse(request.body).compare) { loseReply = false; await result.body.cancel(); throw Error('injected lost commit response'); }
    return result;
  } });
  const uncertain = await new ClusterEngine(binding, lossy, checkpointOptions).begin(['a', 'b']);
  await uncertain.shard('a').setName(1, 'unknown committed'); await uncertain.shard('b').setName(1, 'unknown committed');
  await assert.rejects(uncertain.commit(), { outcome: 'unknown' });
  const recovered = await engine.begin(['a', 'b']);
  assert.equal(await recovered.shard('a').getName(1), 'unknown committed');
  assert.equal(await recovered.shard('b').getName(1), 'unknown committed'); await recovered.close();
  console.log('PASS real etcd: dropped response discovers both committed shards without replay');

  if (process.env.CLUSTER_RUST_EXAMPLE) {
    const child = spawn(process.env.CLUSTER_RUST_EXAMPLE, [endpoints.join(','), '/issue138/', ...checkpointFlags], { stdio: 'inherit' });
    assert.equal((await once(child, 'exit'))[0], 0);
  }
  if (process.env.CLUSTER_CSHARP_EXAMPLE) {
    const child = spawn('dotnet', [process.env.CLUSTER_CSHARP_EXAMPLE, endpoints.join(','), '/issue138/', ...checkpointFlags], { env: { ...process.env, DOTNET_GCHeapHardLimit: '0x18000000' }, stdio: 'inherit' });
    assert.equal((await once(child, 'exit'))[0], 0);
  }
  const native = await engine.begin(['a', 'b']);
  for (const key of ['a', 'b']) assert.equal(await native.shard(key).getName(1), process.env.CLUSTER_CSHARP_EXAMPLE ? 'C# coordinated 雪' : process.env.CLUSTER_RUST_EXAMPLE ? 'Rust coordinated 雪' : 'unknown committed');
  await native.close();
  await javascriptExample([endpoints.join(','), '/issue138/'], engine);

  // Two additional single-member consensus groups demonstrate physically
  // independent bulk storage. The three-member catalog carries pointers only.
  const dataEndpoints = ports.slice(6, 8).map((port) => `http://127.0.0.1:${port}`);
  for (let i = 0; i < 2; i++) {
    const name = `data${i}`; const peer = `http://127.0.0.1:${ports[8 + i]}`;
    const log = createWriteStream(join(directory, `${name}.log`));
    const child = spawn(binary, ['--name', name, '--data-dir', join(directory, name),
      '--listen-client-urls', dataEndpoints[i], '--advertise-client-urls', dataEndpoints[i],
      '--listen-peer-urls', peer, '--initial-advertise-peer-urls', peer, '--initial-cluster', `${name}=${peer}`,
      '--initial-cluster-token', `issue-138-${ports[0]}-${name}`, '--quota-backend-bytes', '16777216', '--log-level', 'warn'],
    { env: { ...process.env, GOMEMLIMIT: '100MiB' }, stdio: ['ignore', 'pipe', 'pipe'] });
    child.stdout.pipe(log); child.stderr.pipe(log); nodes.push({ child, exited: once(child, 'exit'), log });
  }
  await waitForMembers(dataEndpoints);
  const prefix = '/partitioned/';
  let catalogRequests = 0; let dataRequests = 0;
  const catalog = new EtcdJournalStore(endpoints, { ...options, prefix, fetch: (...args) => { catalogRequests++; return fetch(...args); } });
  const dataOptions = { ...options, prefix, fetch: (...args) => { dataRequests++; return fetch(...args); } };
  const east = new EtcdJournalStore([dataEndpoints[0]], dataOptions);
  const west = new EtcdJournalStore([dataEndpoints[1]], dataOptions);
  const cacheBytes = Number(process.env.CLUSTER_OBJECT_CACHE_BYTES ?? 0);
  const partitioned = new PartitionedJournalStore(catalog, { east, west }, (key) => key === 'a' ? 'east' : 'west', { cacheBytes });
  const partitionedEngine = new ClusterEngine(binding, partitioned, { requireDurableFlush: true, ...checkpointOptions });
  const dataSeed = await partitionedEngine.begin(['a', 'b']);
  await dataSeed.shard('a').create(0, 0); await dataSeed.shard('b').create(0, 0); await dataSeed.commit();
  if (cacheBytes > 0) {
    const first = await partitionedEngine.begin(['a', 'b']); await first.close();
    const beforeCatalog = catalogRequests; const beforeData = dataRequests;
    const second = await partitionedEngine.begin(['a', 'b']);
    assert.equal(await second.shard('a').count(), 1n); await second.close();
    assert.equal(catalogRequests, beforeCatalog + 1); assert.equal(dataRequests, beforeData);
    console.log('PASS immutable object cache: actual WASM begin rereads the catalog with zero repeat data requests');
  }
  const cacheFlags = cacheBytes > 0 ? ['--cache'] : [];
  if (process.env.CLUSTER_RUST_EXAMPLE) {
    const child = spawn(process.env.CLUSTER_RUST_EXAMPLE, [endpoints.join(','), prefix, ...dataEndpoints, ...cacheFlags, ...checkpointFlags], { stdio: 'inherit' });
    assert.equal((await once(child, 'exit'))[0], 0);
  }
  if (process.env.CLUSTER_CSHARP_EXAMPLE) {
    const child = spawn('dotnet', [process.env.CLUSTER_CSHARP_EXAMPLE, endpoints.join(','), prefix, ...dataEndpoints, ...cacheFlags, ...checkpointFlags], { env: { ...process.env, DOTNET_GCHeapHardLimit: '0x18000000' }, stdio: 'inherit' });
    assert.equal((await once(child, 'exit'))[0], 0);
  }
  const dataCheck = await partitionedEngine.begin(['a', 'b']);
  for (const key of ['a', 'b']) {
    assert.equal(await dataCheck.shard(key).count(), 1n);
    if (process.env.CLUSTER_CSHARP_EXAMPLE || process.env.CLUSTER_RUST_EXAMPLE) assert.equal(await dataCheck.shard(key).getName(1), process.env.CLUSTER_CSHARP_EXAMPLE ? 'C# coordinated 雪' : 'Rust coordinated 雪');
  }
  await dataCheck.close();
  await javascriptExample([endpoints.join(','), prefix, ...dataEndpoints, ...cacheFlags], partitionedEngine);
  const pointers = await catalog.read(['a', 'b']);
  if (checkpoint) {
    for (const value of await partitioned.read(['a', 'b'])) {
      assert.equal(value.entries.length, 1); assert.ok(value.entries[0].startsWith('engine3|'));
    }
    console.log('PASS checkpoints: native Rust/C#/actual-WASM round trips preserve compacted journals and monotonic CAS generations');
  }
  assert.deepEqual(pointers.map((value) => value.entries.slice(0, 2)), [['blob1', 'east'], ['blob1', 'west']]);
  assert.ok(pointers.every((value) => value.entries.length === 4));
  console.log('PASS independent consensus groups: native/WASM interoperability; catalog contains only atomic pointers');
  const unavailable = await partitionedEngine.begin(['a', 'b']);
  await unavailable.shard('a').setName(1, 'must remain private'); await unavailable.shard('b').setName(1, 'must remain private');
  await stop(nodes[4]);
  await assert.rejects(unavailable.commit());
  assert.deepEqual(await catalog.read(['a', 'b']), pointers);
  if (cacheBytes > 0) {
    const cachedWest = await partitionedEngine.begin(['b']);
    assert.equal(await cachedWest.shard('b').getName(1), 'JS coordinated 雪'); await cachedWest.close();
    console.log('PASS immutable object cache: previously verified west remains readable during its data-group outage');
  } else await assert.rejects(partitionedEngine.begin(['b']));
  const isolated = await partitionedEngine.begin(['a']); await isolated.shard('a').setName(1, 'independent group available'); await isolated.commit();
  const isolatedRead = await partitionedEngine.begin(['a']); assert.equal(await isolatedRead.shard('a').getName(1), 'independent group available'); await isolatedRead.close();
  console.log('PASS independent consensus groups: unavailable west publishes nothing; east still commits');

  const status = await Promise.all(endpoints.map(async (endpoint) => (await fetch(`${endpoint}/v3/maintenance/status`, { method: 'POST', body: '{}', signal: AbortSignal.timeout(2000) })).json()));
  const leader = status.findIndex((item) => item.header.member_id === item.leader);
  assert.ok(leader >= 0); await stop(nodes[leader]);
  const after = await ready(() => engine.begin(['a', 'b']));
  assert.equal(await after.shard('a').count(), 1n); assert.equal(await after.shard('b').count(), 1n);
  await after.shard('a').setName(1, 'after failover'); await after.commit();
  console.log(`PASS real etcd: killed leader n${leader}; surviving majority elected and committed`);
  const second = [0, 1, 2].find((i) => i !== leader); await stop(nodes[second]);
  await assert.rejects(engine.begin(['a', 'b']));
  await assert.rejects(partitionedEngine.begin(['a']));
  console.log('PASS real etcd: majority loss refuses a fresh linearizable snapshot');
} finally {
  await Promise.all(nodes.map(stop));
  for (const node of nodes) node.log.end();
  console.log(`etcd evidence: ${directory}`);
}
