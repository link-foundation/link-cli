// Called by the existing bounded real-etcd harness after every member is ready.
// Includes ordinary links operations through the actual WASM workspace.
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { AddressAllocator } from '../../js/client/src/engine-address-allocator.js';
import { CausalBatch } from '../../js/client/src/engine-causal-batch.js';
import { CausalSession } from '../../js/client/src/engine-causal-session.js';
import { EtcdJournalStore } from '../../js/client/src/cluster.js';

const A = '00000000000000000000000000000001', B = '00000000000000000000000000000002';
const token = n => n.toString(16).padStart(32, '0');
const execute = promisify(execFile);
async function command(executable, args) {
  // execFile bounds both streams, kills on overflow and waits for child close.
  const { stdout } = await execute(executable, args, { maxBuffer: 65536,
    env: { ...process.env, DOTNET_GCHeapHardLimit: '0x18000000' } });
  return JSON.parse(stdout);
}
export async function verifyAddressAllocation(endpoints, binding) {
  const prefix = '/address-reservations/', options = { prefix, timeoutMs: 1000 };
  const physical = new EtcdJournalStore(endpoints, options);
  const owner = new AddressAllocator(physical, 'addresses', '1', '18446744073709551615', { requireDurableFlush: true });
  const first = await owner.reserve(A, token(10), '3');
  assert.deepEqual([first.first, first.last], ['1', '3']);

  // A cached block can be partly consumed. Its abandoned suffix stays burned.
  const batches = new CausalBatch(physical, 'allocated-owner', 'allocated-batches', A, { requireDurableFlush: true });
  const session = await CausalSession.open(binding, batches, first.first, first.last, 'reject');
  try {
    const client = session.client(), before = await physical.read(['addresses', 'allocated-owner', 'allocated-batches']);
    assert.equal((await client.create(0n, 0n)).index, 1n);
    assert.equal((await client.create(0n, 0n)).index, 2n);
    await client.setName(1n, 'allocated 雪😀\0');
    assert.deepEqual(await physical.read(['addresses', 'allocated-owner', 'allocated-batches']), before);
    const versions = await session.commit(token(80));
    assert.deepEqual(versions.map(v => v.address), ['1', '2']);
  } finally { await session.close(); }

  let lost = false, attempts = 0;
  const lossy = new EtcdJournalStore(endpoints, { ...options, fetch: async (url, request) => {
    const reply = await fetch(url, request);
    if (JSON.parse(request.body).compare) {
      attempts++;
      if (!lost) { lost = true; await reply.body.cancel(); throw Error('lost allocation reply'); }
    }
    return reply;
  } });
  await assert.rejects(new AddressAllocator(lossy, 'addresses', '1', '18446744073709551615').reserve(B, token(20), '2'), /indeterminate/i);
  assert.equal(attempts, 1);
  const second = { author: B, token: token(20), count: '2', first: '4', last: '5' };

  const native = [];
  if (process.env.ALLOCATION_RUST_EXAMPLE) native.push({ executable: process.env.ALLOCATION_RUST_EXAMPLE, args: [], author: A, token: token(30), count: '2' });
  if (process.env.CLUSTER_CSHARP_EXAMPLE) native.push({ executable: 'dotnet', args: [process.env.CLUSTER_CSHARP_EXAMPLE, '--reserve-addresses'], author: B, token: token(40), count: '3' });
  for (const request of native) {
    const args = [...request.args, endpoints.join(','), prefix, request.author, request.token, request.count];
    const grant = await command(request.executable, args);
    assert.deepEqual(await owner.reserve(request.author, request.token, request.count), grant);
    request.args = args; request.grant = grant;
  }
  await owner.reserve(A, token(50), '1');
  const beforeRetry = await physical.read(['addresses']);
  assert.deepEqual(await owner.reserve(B, token(20), '2'), second);
  for (const request of native) assert.deepEqual(await command(request.executable, request.args), request.grant);
  assert.deepEqual(await physical.read(['addresses']), beforeRetry);

  const outcomes = await Promise.allSettled([owner.reserve(A, token(60), '2'), owner.reserve(B, token(70), '2')]);
  assert.ok(outcomes.some(value => value.status === 'fulfilled'));
  for (const outcome of outcomes) if (outcome.status === 'rejected') assert.match(outcome.reason.message, /conflict/i);
  await owner.reserve(A, token(60), '2'); await owner.reserve(B, token(70), '2');
  const snapshot = await owner.snapshot(); let next = 1n;
  for (const grant of snapshot.reservations) { assert.equal(BigInt(grant.first), next); next += BigInt(grant.count); assert.equal(BigInt(grant.last), next - 1n); }
  assert.equal(snapshot.next, next.toString());
  assert.deepEqual((await batches.snapshot()).versions.map(v => v.address), ['1', '2']);
  console.log(`PASS real etcd address reservations: ${native.length} native ports, actual WASM creates, abandoned gap, competing writers, lost reply and exact retained retries`);
}
