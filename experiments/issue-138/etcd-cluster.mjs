// Bounded real consensus integration. All children are killed and reaped.
// ETCD_BIN=/path/to/etcd node experiments/issue-138/etcd-cluster.mjs
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createWriteStream, mkdtempSync, readFileSync } from 'node:fs';
import net from 'node:net';
import { resolve, join } from 'node:path';
import * as binding from '../../js/pkg/clink_wasm.js';
import { ClusterEngine, EtcdJournalStore } from '../../js/client/src/cluster.js';
import { waitForMembers } from './etcd-harness.mjs';

binding.initSync({ module: readFileSync(new URL('../../js/pkg/clink_wasm_bg.wasm', import.meta.url)) });
const binary = process.env.ETCD_BIN ?? resolve('experiments/issue-138/etcd/etcd-v3.7.3-linux-amd64/etcd');
const directory = mkdtempSync(resolve('experiments/issue-138/etcd/run-'));
const ports = [];
for (let i = 0; i < 6; i++) {
  const server = net.createServer(); await new Promise((done) => server.listen(0, '127.0.0.1', done));
  ports.push(server.address().port); await new Promise((done) => server.close(done));
}
const endpoints = ports.slice(0, 3).map((port) => `http://127.0.0.1:${port}`);
const peers = ports.slice(3).map((port) => `http://127.0.0.1:${port}`);
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
  const engine = new ClusterEngine(binding, store, { requireDurableFlush: true });
  const seed = await engine.begin(['a', 'b']);
  await seed.shard('a').create(0, 0); await seed.shard('b').create(0, 0); await seed.commit();
  const stale = await engine.begin(['a', 'b']);
  const other = new ClusterEngine(binding, new EtcdJournalStore(endpoints, options));
  const fresh = await other.begin(['b']); await fresh.shard('b').setName(1, 'leader-safe 雪'); await fresh.commit();
  await stale.shard('a').delete(1); await assert.rejects(stale.commit(), { outcome: 'failed' });
  console.log('PASS real etcd: atomic multi-shard CAS and read-only dependency conflict');

  let loseReply = true;
  const lossy = new EtcdJournalStore(endpoints, { ...options, fetch: async (url, request) => {
    const result = await fetch(url, request);
    if (loseReply && JSON.parse(request.body).compare) { loseReply = false; await result.body.cancel(); throw Error('injected lost commit response'); }
    return result;
  } });
  const uncertain = await new ClusterEngine(binding, lossy).begin(['a', 'b']);
  await uncertain.shard('a').setName(1, 'unknown committed'); await uncertain.shard('b').setName(1, 'unknown committed');
  await assert.rejects(uncertain.commit(), { outcome: 'unknown' });
  const recovered = await engine.begin(['a', 'b']);
  assert.equal(await recovered.shard('a').getName(1), 'unknown committed');
  assert.equal(await recovered.shard('b').getName(1), 'unknown committed'); await recovered.close();
  console.log('PASS real etcd: dropped response discovers both committed shards without replay');

  if (process.env.CLUSTER_RUST_EXAMPLE) {
    const child = spawn(process.env.CLUSTER_RUST_EXAMPLE, [endpoints.join(','), '/issue138/'], { stdio: 'inherit' });
    assert.equal((await once(child, 'exit'))[0], 0);
  }
  if (process.env.CLUSTER_CSHARP_EXAMPLE) {
    const child = spawn('dotnet', [process.env.CLUSTER_CSHARP_EXAMPLE, endpoints.join(','), '/issue138/'], { env: { ...process.env, DOTNET_GCHeapHardLimit: '0x18000000' }, stdio: 'inherit' });
    assert.equal((await once(child, 'exit'))[0], 0);
  }
  const native = await engine.begin(['a', 'b']);
  for (const key of ['a', 'b']) assert.equal(await native.shard(key).getName(1), process.env.CLUSTER_CSHARP_EXAMPLE ? 'C# coordinated 雪' : process.env.CLUSTER_RUST_EXAMPLE ? 'Rust coordinated 雪' : 'unknown committed');
  await native.close();
  const status = await Promise.all(endpoints.map(async (endpoint) => (await fetch(`${endpoint}/v3/maintenance/status`, { method: 'POST', body: '{}', signal: AbortSignal.timeout(2000) })).json()));
  const leader = status.findIndex((item) => item.header.member_id === item.leader);
  assert.ok(leader >= 0); await stop(nodes[leader]);
  const after = await ready(() => engine.begin(['a', 'b']));
  assert.equal(await after.shard('a').count(), 1n); assert.equal(await after.shard('b').count(), 1n);
  await after.shard('a').setName(1, 'after failover'); await after.commit();
  console.log(`PASS real etcd: killed leader n${leader}; surviving majority elected and committed`);
  const second = [0, 1, 2].find((i) => i !== leader); await stop(nodes[second]);
  await assert.rejects(engine.begin(['a', 'b']));
  console.log('PASS real etcd: majority loss refuses a fresh linearizable snapshot');
} finally {
  await Promise.all(nodes.map(stop));
  for (const node of nodes) node.log.end();
  console.log(`etcd evidence: ${directory}`);
}
