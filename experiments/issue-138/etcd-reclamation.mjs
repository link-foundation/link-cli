// Finite real-etcd reclamation and actual native/WASM engine handoffs.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createWriteStream, mkdtempSync } from 'node:fs';
import net from 'node:net';
import { resolve, join } from 'node:path';
import { EtcdJournalStore, ReclaimingPartitionedJournalStore } from '../../js/client/src/index.js';
import { waitForMembers } from './etcd-harness.mjs';
const directory = mkdtempSync(resolve('experiments/issue-138/etcd/reclamation-'));
const binary = process.env.ETCD_BIN ?? resolve('experiments/issue-138/etcd/etcd-v3.7.3-linux-amd64/etcd');
const nodes = []; const reservations = []; const ports = [];
const chunked = process.env.RECLAMATION_CHUNKS === '1';
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
function providers(prefix, fetches = {}, selected = endpoints) {
  const provider = (name) => new EtcdJournalStore(selected, { prefix: `${prefix}${name}/`, timeoutMs: 700, ...(fetches[name] ? { fetch: fetches[name] } : {}) });
  return { catalog: provider('catalog'), east: provider('east'), west: provider('west') };
}
async function open(group, cacheBytes = 0) {
  return ReclaimingPartitionedJournalStore.open(group.catalog, { east: group.east, west: group.west }, (key) => key === 'a' ? 'east' : 'west', { pageKeys: 1, cacheBytes, chunks: chunked ? { maxRecords: 2 } : null });
}
const values = (name) => ['a', 'b'].map((shard) => chunked ? [`${name}-${shard}-prefix-1`, `${name}-${shard}-prefix-2`, `${name}-${shard}-tail`] : [`${name}-${shard}`]);
async function publish(store, values) { assert.equal(await store.compareExchange(['a', 'b'], await store.read(['a', 'b']), values), true); }
const key = (request) => Buffer.from(request.key, 'base64').toString();
async function lossCases() {
  for (const phase of ['initialization', 'staging', 'publication', 'fence', 'deletion']) for (const after of [false, true]) {
    const prefix = `/reclamation-loss-${phase}-${after}/`; const healthy = providers(prefix);
    let armed = phase === 'initialization'; let sends = 0;
    const lossy = async (url, request) => {
      const body = JSON.parse(request.body);
      const hit = armed && body.compare && body.success?.some((operation) => {
        if (phase === 'deletion') return operation.request_delete_range;
        const put = operation.request_put; if (!put) return false;
        const name = key(put);
        return phase === 'staging' ? name.includes('/east/blob2-') : phase === 'publication' ? name.endsWith('/catalog/a') : name.endsWith('/catalog/__link_gc_epoch');
      });
      if (!hit) return fetch(url, request);
      sends++;
      if (after) { const response = await fetch(url, request); await response.body.cancel(); }
      throw Error(`injected ${phase} ${after ? 'after' : 'before'} response loss`);
    };
    const affected = ['staging', 'deletion'].includes(phase) ? 'east' : 'catalog';
    const selected = providers(prefix, { [affected]: lossy });
    if (phase === 'initialization') await assert.rejects(open(selected), { outcome: 'unknown' });
    else {
      const writer = await open(healthy); await publish(writer, values('old'));
      if (phase === 'deletion') await publish(writer, values('new'));
      const attempt = await open(selected); armed = true;
      if (phase === 'staging' || phase === 'publication') await assert.rejects(publish(attempt, values('new')), { outcome: 'unknown' });
      else await assert.rejects(attempt.collectGarbage(), { outcome: 'unknown' });
    }
    assert.equal(sends, 1, `${phase} must send once`);
    const recovered = await open(healthy); const current = await recovered.read(['a', 'b']);
    if (phase === 'initialization') assert.deepEqual(current.map((item) => item.entries), [[], []]);
    else assert.deepEqual(current.map((item) => item.entries), phase === 'deletion' || (phase === 'publication' && after) ? values('new') : values('old'));
    if (phase === 'fence') assert.equal((await healthy.catalog.read(['__link_gc_epoch']))[0].entries[1], after ? '1' : '0');
    assert.ok(await recovered.collectGarbage());
    assert.deepEqual((await recovered.read(['a', 'b'])).map((item) => item.entries), current.map((item) => item.entries));
    console.log(`PASS real etcd: ${phase} ${after ? 'after' : 'before'} response loss, one mutation send, fresh recovery and collection`);
  }
}
async function readerCases() {
  const prefix = '/reclamation-reader/'; const healthy = providers(prefix); const writer = await open(healthy);
  await publish(writer, values('old')); let armed = false; let refreshed = false;
  const racing = providers(prefix, { catalog: async (url, request) => {
    const body = JSON.parse(request.body); const response = await fetch(url, request);
    if (armed && !refreshed && !body.compare && body.success?.length === 3) {
      refreshed = true; await publish(writer, values('new')); assert.equal((await writer.collectGarbage()).deleted, chunked ? 4 : 2);
    }
    return response;
  } });
  const reader = await open(racing); armed = true;
  assert.deepEqual((await reader.read(['a', 'b'])).map((item) => item.entries), values('new')); assert.ok(refreshed);
  console.log('PASS real etcd: retired two-shard snapshot refresh returns one complete new group');
  let dataDown = false; let catalogDown = false;
  const cached = await open(providers(prefix, Object.fromEntries(['catalog', 'east', 'west'].map((name) => [name, async (url, request) => {
    if (name === 'catalog' ? catalogDown : dataDown) throw Error('injected group outage');
    return fetch(url, request);
  }]))), 65536);
  const owned = await cached.read(['a', 'b']);
  await publish(writer, values('later')); assert.equal((await writer.collectGarbage()).deleted, chunked ? 4 : 2);
  assert.deepEqual(owned.map((item) => item.entries), values('new'));
  await cached.read(['a', 'b']); dataDown = true;
  assert.deepEqual((await cached.read(['a', 'b'])).map((item) => item.entries), values('later'));
  catalogDown = true; await assert.rejects(cached.read(['a', 'b']));
  console.log('PASS real etcd: owned retired journals, warm-cache data outage, catalog remains required');
}
const deadline = setTimeout(() => { for (const node of nodes) node.child.kill('SIGKILL'); }, 120000);
try {
  for (let i = 0; i < 3; i++) {
    const log = createWriteStream(join(directory, `n${i}.log`));
    const process = spawn(binary, ['--name', `n${i}`, '--data-dir', join(directory, `n${i}`),
      '--listen-client-urls', endpoints[i], '--advertise-client-urls', endpoints[i], '--listen-peer-urls', peers[i],
      '--initial-advertise-peer-urls', peers[i], '--initial-cluster', peers.map((peer, index) => `n${index}=${peer}`).join(','),
      '--initial-cluster-token', `reclamation-${ports[0]}`, '--quota-backend-bytes', '16777216', '--log-level', 'warn'],
    { env: { ...globalThis.process.env, GOMEMLIMIT: '80MiB' }, stdio: ['ignore', 'pipe', 'pipe'] });
    process.stdout.pipe(log); process.stderr.pipe(log); nodes.push({ child: process, exited: once(process, 'exit'), log });
  }
  await waitForMembers(endpoints);
  assert.ok(process.env.RECLAMATION_RUST_EXAMPLE && process.env.RECLAMATION_CSHARP_EXAMPLE, 'build both native examples first');
  const prefix = '/reclamation-handoff/';
  await child(process.env.RECLAMATION_RUST_EXAMPLE, [endpoints.join(','), prefix]);
  await child('dotnet', [process.env.RECLAMATION_CSHARP_EXAMPLE, endpoints.join(','), prefix]);
  for (const [runtime, flags] of [['node', ['--max-old-space-size=96']], ['bun', []], ['deno', ['run', '--allow-net', '--allow-read', '--allow-env']]])
    await child(runtime, [...flags, 'js/client/examples/journal-reclamation.mjs', endpoints.join(','), prefix]);
  await lossCases(); await readerCases();
  // Refresh immediately before the cut; startup leadership may have changed.
  const statuses = await waitForMembers(endpoints);
  const leader = statuses.findIndex((status) => status.header.member_id === status.leader); assert.ok(leader >= 0);
  assert.ok(statuses.every((status) => status.leader === statuses[leader].header.member_id));
  await stop(nodes[leader]);
  const survivors = nodes.map((node, index) => ({ node, index })).filter(({ index }) => index !== leader);
  const liveEndpoints = survivors.map(({ index }) => endpoints[index]); let elected = false;
  for (let attempt = 0; attempt < 30; attempt++) {
    try {
      const current = await waitForMembers(liveEndpoints, { attempts: 1 });
      if (current.every((status) => status.leader !== statuses[leader].header.member_id)) {
        await providers(prefix, {}, liveEndpoints).catalog.read(['__link_gc_epoch']); elected = true; break;
      }
    } catch { /* bounded read-only readiness retry */ }
    await new Promise((done) => setTimeout(done, 100));
  }
  assert.ok(elected);
  await child(process.env.RECLAMATION_RUST_EXAMPLE, [liveEndpoints.join(','), prefix]);
  await child('dotnet', [process.env.RECLAMATION_CSHARP_EXAMPLE, liveEndpoints.join(','), prefix]);
  await child('node', ['--max-old-space-size=96', 'js/client/examples/journal-reclamation.mjs', liveEndpoints.join(','), prefix]);
  const degraded = await open(providers(prefix, {}, liveEndpoints)); const before = await degraded.read(['a', 'b']);
  await stop(survivors[0].node);
  await assert.rejects(degraded.read(['a', 'b']));
  await assert.rejects(degraded.collectGarbage());
  await assert.rejects(degraded.compareExchange(['a', 'b'], before, [['must-not-ack'], null]));
  console.log('PASS real etcd: native/WASM handoffs after leader loss, majority loss gives no partial read or false publication/collection acknowledgment');
} finally {
  clearTimeout(deadline); await Promise.all(nodes.map(stop));
  await Promise.all(nodes.map(({ log }) => new Promise((done) => log.end(done))));
}
console.log(`PASS all ${chunked ? 'chunks2' : 'blob2'} reclamation integration contracts; retained server logs ${directory}`);
