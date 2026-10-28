// Finite three-member real-etcd maintenance contracts; all children are reaped.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createWriteStream, mkdtempSync } from 'node:fs';
import net from 'node:net';
import { resolve, join } from 'node:path';
import { EtcdJournalStore } from '../../js/client/src/cluster.js';
import { waitForMembers } from './etcd-harness.mjs';
const directory = mkdtempSync(resolve('experiments/issue-138/etcd/maintenance-'));
const binary = process.env.ETCD_BIN ?? resolve('experiments/issue-138/etcd/etcd-v3.7.3-linux-amd64/etcd');
const nodes = []; const reservations = []; const ports = [];
for (let i = 0; i < 6; i++) {
  const server = net.createServer(); reservations.push(server);
  await new Promise((done, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', done); });
  ports.push(server.address().port);
}
await Promise.all(reservations.map((server) => new Promise((done) => server.close(done))));
const endpoints = ports.slice(0, 3).map((port) => `http://127.0.0.1:${port}`);
const peers = ports.slice(3).map((port) => `http://127.0.0.1:${port}`);
async function stop(node) { if (node.child.exitCode === null && node.child.signalCode === null) node.child.kill('SIGKILL'); await node.exited; }
async function child(command, args) {
  const process = spawn(command, args, { stdio: 'inherit', env: { ...globalThis.process.env, DOTNET_GCHeapHardLimit: '0x10000000' } });
  const deadline = setTimeout(() => process.kill('SIGKILL'), 30000);
  try { assert.equal((await once(process, 'exit'))[0], 0); } finally { clearTimeout(deadline); }
}
async function post(endpoint, path, body) {
  const response = await fetch(`${endpoint}${path}`, { method: 'POST', body: JSON.stringify(body), signal: AbortSignal.timeout(3000) });
  assert.ok(response.ok); return response.json();
}
const deadline = setTimeout(() => { for (const node of nodes) node.child.kill('SIGKILL'); }, 120000);
try {
  for (let i = 0; i < 3; i++) {
    const log = createWriteStream(join(directory, `n${i}.log`));
    const process = spawn(binary, ['--name', `n${i}`, '--data-dir', join(directory, `n${i}`),
      '--listen-client-urls', endpoints[i], '--advertise-client-urls', endpoints[i], '--listen-peer-urls', peers[i],
      '--initial-advertise-peer-urls', peers[i], '--initial-cluster', peers.map((peer, index) => `n${index}=${peer}`).join(','),
      '--initial-cluster-token', `maintenance-${ports[0]}`, '--quota-backend-bytes', '16777216', '--log-level', 'warn'],
    { env: { ...globalThis.process.env, GOMEMLIMIT: '80MiB' }, stdio: ['ignore', 'pipe', 'pipe'] });
    process.stdout.pipe(log); process.stderr.pipe(log); nodes.push({ child: process, exited: once(process, 'exit'), log });
  }
  const statuses = await waitForMembers(endpoints);
  assert.ok(process.env.MAINTENANCE_RUST_EXAMPLE && process.env.MAINTENANCE_CSHARP_EXAMPLE, 'build both native examples first');
  await child(process.env.MAINTENANCE_RUST_EXAMPLE, [endpoints.join(','), '/maintenance-rust/']);
  await child('dotnet', [process.env.MAINTENANCE_CSHARP_EXAMPLE, endpoints.join(','), '/maintenance-csharp/']);
  for (const [runtime, flags] of [['node', ['--max-old-space-size=96']], ['bun', []], ['deno', ['run', '--allow-net']]])
    await child(runtime, [...flags, 'js/client/examples/journal-maintenance.mjs', endpoints.join(','), `/maintenance-${runtime}/`]);
  const options = { prefix: '/maintenance-race/', timeoutMs: 700 }; const writer = new EtcdJournalStore(endpoints, options);
  assert.ok(await writer.compareExchange(['a', 'b', 'c'], ['a', 'b', 'c'].map(() => ({ generation: '0', entries: [] })), [['a-old'], ['b-old'], ['c-old']]));
  let changed = false;
  const reader = new EtcdJournalStore(endpoints, { ...options, fetch: async (url, request) => {
    const result = await fetch(url, request); const body = JSON.parse(request.body);
    if (!changed && body.success?.[0]?.request_range) {
      changed = true;
      const old = await writer.read(['b', 'c', 'd']);
      assert.ok(await writer.compareExchange(['b', 'd'], [old[0], old[2]], [['b-new'], ['d-new']]));
      assert.ok(await writer.compareDelete('c', old[1].generation));
    }
    return result;
  } });
  const snapshot = await reader.scanNamespace({ pageKeys: 1 });
  assert.deepEqual(snapshot.map((item) => item.key), ['a', 'b', 'c']);
  assert.deepEqual(snapshot.map((item) => item.version.entries[0]), ['a-old', 'b-old', 'c-old']);
  assert.deepEqual((await writer.scanNamespace({ pageKeys: 1 })).map((item) => item.key), ['a', 'b', 'd']);
  console.log('PASS real etcd: revision-pinned pages exclude concurrent insert/update/delete');
  let compacted = false; let firstPages = 0;
  const compactionReader = new EtcdJournalStore(endpoints, { ...options, fetch: async (url, request) => {
    const body = JSON.parse(request.body); const range = body.success?.[0]?.request_range;
    if (range && range.revision === undefined) firstPages++;
    const result = await fetch(url, request);
    if (!compacted && range) {
      compacted = true;
      const old = await writer.read(['b']); assert.ok(await writer.compareExchange(['b'], old, [['b-after-compaction']]));
      const fresh = await writer.read(['b']);
      await post(endpoints[0], '/v3/kv/compaction', { revision: fresh[0].generation });
    }
    return result;
  } });
  const fresh = await compactionReader.scanNamespace({ pageKeys: 1 });
  assert.equal(firstPages, 2); assert.equal(fresh[1].version.entries[0], 'b-after-compaction');
  console.log('PASS real etcd: compacted historical page restarts the complete snapshot');
  let sends = 0;
  const lossy = new EtcdJournalStore(endpoints, { ...options, fetch: async (url, request) => {
    sends++; const result = await fetch(url, request); await result.body.cancel(); throw Error('lost acknowledged delete response');
  } });
  await assert.rejects(lossy.compareDelete('a', fresh[0].version.generation), { outcome: 'unknown' });
  assert.equal(sends, 1); assert.equal((await writer.read(['a']))[0].generation, '0');
  console.log('PASS real etcd: lost deletion response is discovered without mutation replay');
  const leader = statuses.findIndex((status) => status.header.member_id === status.leader); assert.ok(leader >= 0);
  await stop(nodes[leader]);
  const survivors = nodes.map((node, index) => ({ node, index })).filter(({ index }) => index !== leader);
  // Status can still advertise the dead leader. Require a newly elected leader
  // and a successful linearizable read before evaluating post-election scans.
  let elected = false;
  for (let attempt = 0; attempt < 30; attempt++) {
    try {
      const current = await waitForMembers(survivors.map(({ index }) => endpoints[index]), { attempts: 1 });
      if (current.every((status) => status.leader !== statuses[leader].header.member_id)) {
        await new EtcdJournalStore(survivors.map(({ index }) => endpoints[index]), options).read(['b']);
        elected = true; break;
      }
    } catch { /* finite read-only readiness retry */ }
    await new Promise((done) => setTimeout(done, 100));
  }
  assert.ok(elected, 'surviving majority must elect and serve linearizable reads');
  assert.equal((await writer.scanNamespace({ pageKeys: 1 })).length, 2);
  console.log('PASS real etcd: complete scan after leader loss');
  await stop(survivors[0].node);
  await assert.rejects(writer.scanNamespace({ pageKeys: 1 }));
  await assert.rejects(writer.compareDelete('b', fresh[1].version.generation), { outcome: 'unknown' });
  console.log('PASS real etcd: majority loss returns no partial scan and no false deletion acknowledgment');
} finally {
  clearTimeout(deadline); await Promise.all(nodes.map(stop));
  await Promise.all(nodes.map(({ log }) => new Promise((done) => log.end(done))));
}
console.log(`PASS all maintenance integration contracts; retained server logs ${directory}`);
