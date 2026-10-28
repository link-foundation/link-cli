// Reuse the bounded real-etcd harness and actual Rust/WASM workspace.
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { AuthorFence } from '../../js/client/src/engine-author-fence.js';
import { AddressAllocator } from '../../js/client/src/engine-address-allocator.js';
import { CausalBatch } from '../../js/client/src/engine-causal-batch.js';
import { CausalSession } from '../../js/client/src/engine-causal-session.js';
import { EtcdJournalStore } from '../../js/client/src/cluster.js';
const A = '00000000000000000000000000000001', token = n => n.toString(16).padStart(32, '0');
const execute = promisify(execFile);
export async function verifyAuthorFence(endpoints, binding) {
  const prefix = '/author-fence/', physical = new EtcdJournalStore(endpoints, { prefix, timeoutMs: 1000 });
  const f = new AuthorFence(physical, 'fence', A, { requireDurableFlush: true });
  const first = await f.claim('0', token(100)), oldStore = f.bind(first);
  const allocator = store => new AddressAllocator(store, 'addresses', '1', '18446744073709551615');
  const batch = store => new CausalBatch(store, 'owner', 'receipts', A);
  const block = await allocator(oldStore).reserve(A, token(110), '4');
  const committed = await CausalSession.open(binding, batch(oldStore), block.first, block.last, 'reject');
  try { assert.equal((await committed.client().create(0n, 0n)).index, 1n); await committed.commit(token(300)); }
  finally { await committed.close(); }
  const stale = await CausalSession.open(binding, batch(oldStore), '2', '4', 'reject');
  try {
    assert.equal((await stale.client().create(0n, 0n)).index, 2n); await stale.prepare(token(301));
    let next;
    const args = [endpoints.join(','), prefix, A, '1', token(200)];
    const native = [];
    if (process.env.FENCE_RUST_EXAMPLE) native.push([process.env.FENCE_RUST_EXAMPLE, args]);
    if (process.env.CLUSTER_CSHARP_EXAMPLE) native.push(['dotnet', [process.env.CLUSTER_CSHARP_EXAMPLE, '--claim-author', ...args]]);
    for (const [executable, argv] of native) {
      const { stdout } = await execute(executable, argv, { maxBuffer: 65536, env: { ...process.env, DOTNET_GCHeapHardLimit: '0x18000000' } });
      const claim = JSON.parse(stdout); if (next) assert.deepEqual(claim, next); next = claim;
    }
    next ??= await f.claim('1', token(200));
    assert.deepEqual(next, { author: A, token: token(200), epoch: '2' });
    const before = await physical.read(['fence', 'addresses', 'owner', 'receipts']);
    await assert.rejects(stale.commit(token(301)), /fenced/i);
    await assert.rejects(oldStore.read(['owner']), /fenced/i);
    await assert.rejects(f.claim('0', token(100)), /fenced/i);
    assert.deepEqual(await physical.read(['fence', 'addresses', 'owner', 'receipts']), before);
    const current = f.bind(next), fresh = await allocator(current).reserve(A, token(210), '2');
    assert.deepEqual([fresh.first, fresh.last], ['5', '6']);
    const tx = await CausalSession.open(binding, batch(current), fresh.first, fresh.last, 'reject');
    try { assert.equal((await tx.client().create(0n, 0n)).index, 5n); await tx.commit(token(302)); }
    finally { await tx.close(); }
    const state = await batch(current).snapshot();
    assert.equal(state.lastCounter, '2'); assert.deepEqual(state.versions.map(v => v.address), ['1', '5']);
    assert.deepEqual(await f.claim('1', token(200)), next);
    assert.equal((await physical.read(['fence']))[0].generation, before[0].generation);
    console.log(`PASS real etcd author fencing: ${native.length} native claim ports, actual WASM staging, stale preparation refusal, burned suffix, continuing counter and unchanged fence generation`);
  } finally { await stale.close(); }
}
