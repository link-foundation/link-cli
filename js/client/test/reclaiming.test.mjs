// Copy to js/client/test/reclaiming.test.mjs at the start of the next batch.
import assert from 'node:assert/strict';
import test from 'node:test';
import { ReclaimingPartitionedJournalStore } from '../src/index.js';
import { MemoryMaintenanceStore } from '../../../experiments/issue-138/reclamation-memory.mjs';
const epochKey = '__link_gc_epoch';
function providers() {
  return { catalog: new MemoryMaintenanceStore('catalog'), east: new MemoryMaintenanceStore('east'), west: new MemoryMaintenanceStore('west') };
}
async function open(p, options = {}) {
  return ReclaimingPartitionedJournalStore.open(p.catalog, { east: p.east, west: p.west }, (key) => key === 'a' ? 'east' : 'west', options);
}
async function write(store, a, b) {
  const expected = await store.read(['a', 'b']);
  assert.ok(await store.compareExchange(['a', 'b'], expected, [[a], [b]]));
  return store.read(['a', 'b']);
}
test('explicit open initializes dedicated metadata and leaves collection caller-selected', { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p);
  assert.equal(store.supportsDurableFlush, false);
  assert.deepEqual(p.catalog.values.get(epochKey).entries, ['gc1', '0']);
  assert.deepEqual(await store.read(['a']), [{ generation: '0', entries: [] }]);
  const initial = await write(store, 'a0 雪', 'b0');
  const owned = structuredClone(initial); await write(store, 'a1', 'b1');
  assert.equal(p.east.values.size, 2); assert.equal(p.west.values.size, 2);
  assert.equal(p.east.calls.filter((call) => call.operation === 'delete').length, 0);
  const result = await store.collectGarbage();
  assert.deepEqual(result, { epoch: '1', examined: 4, deleted: 2 });
  assert.equal(p.east.values.size, 1); assert.equal(p.west.values.size, 1);
  assert.deepEqual((await store.read(['a', 'b'])).map((item) => item.entries), [['a1'], ['b1']]);
  assert.deepEqual(owned.map((item) => item.entries), [['a0 雪'], ['b0']]);
  assert.equal(await store.compareExchange(['a', 'b'], initial, [['stale'], null]), false);
  const counts = [p.east.values.size, p.west.values.size];
  await assert.rejects(store.read([epochKey]));
  await assert.rejects(store.read(Array.from({ length: 32 }, (_, i) => `s${i}`)));
  assert.deepEqual([p.east.values.size, p.west.values.size], counts);
});
test('an epoch advance fences an in-flight publisher after staging', { timeout: 10000 }, async () => {
  const p = providers(); const writer = await open(p); const collector = await open(p);
  await write(writer, 'a0', 'b0'); const before = await writer.read(['a']);
  let fence;
  p.east.on('swap', async () => { fence = await collector.collectGarbage(); }, { after: true });
  // The collector can retire the just-staged object before its confirming read.
  // Either a known publication conflict or a conservative staging error is safe.
  let published;
  try { published = await writer.compareExchange(['a'], before, [['in-flight']]); }
  catch { published = false; }
  assert.equal(published, false); assert.equal(fence.epoch, '1');
  assert.deepEqual((await writer.read(['a', 'b'])).map((item) => item.entries), [['a0'], ['b0']]);
  assert.ok(await writer.compareExchange(['a'], await writer.read(['a']), [['following']]));
  assert.deepEqual((await writer.read(['a']))[0].entries, ['following']);
});
test('a retired missing shard refreshes the entire owned group', { timeout: 10000 }, async () => {
  const p = providers(); const writer = await open(p); const reader = await open(p);
  await write(writer, 'a0', 'b0');
  p.west.on('read', async () => {
    await write(writer, 'a1', 'b1');
    assert.equal((await writer.collectGarbage()).deleted, 2);
  });
  const group = await reader.read(['a', 'b']);
  assert.deepEqual(group.map((item) => item.entries), [['a1'], ['b1']]);
});
test('complete listing failure deletes nothing and a lost delete is not replayed', { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p);
  await write(store, 'a0', 'b0'); await write(store, 'a1', 'b1');
  p.west.on('scan', () => { throw Error('incomplete provider snapshot'); });
  await assert.rejects(store.collectGarbage(), /incomplete provider snapshot/);
  assert.equal(p.east.calls.filter((call) => call.operation === 'delete').length, 0);
  assert.equal(p.west.calls.filter((call) => call.operation === 'delete').length, 0);
  p.east.on('delete', () => { throw Error('delete response lost'); }, { after: true });
  await assert.rejects(store.collectGarbage(), /delete response lost/);
  assert.equal(p.east.calls.filter((call) => call.operation === 'delete').length, 1);
  assert.equal(p.east.values.size, 1);
  assert.deepEqual((await store.read(['a', 'b'])).map((item) => item.entries), [['a1'], ['b1']]);
  assert.equal((await store.collectGarbage()).deleted, 1);
});

test('all sparse updates and record arrays reject before provider I/O', { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p); const expected = await store.read(['a', 'b']);
  const calls = () => p.catalog.calls.length + p.east.calls.length + p.west.calls.length;
  for (const updates of [Object.assign(Array(2), { 0: ['must-not-stage'] }), [Array(1), null]]) {
    const count = calls(); await assert.rejects(store.compareExchange(['a', 'b'], expected, updates));
    assert.equal(calls(), count);
  }
});
test('contradictory protected generations abort before any deletion', { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p);
  assert.ok(await store.compareExchange(['a'], await store.read(['a']), [['live']]));
  const pointer = structuredClone(p.catalog.values.get('a'));
  p.catalog.values.set('b', structuredClone(pointer));
  p.catalog.values.get('a').entries[3] = '999';
  await assert.rejects(store.collectGarbage(), /contradictory protected/);
  assert.equal(p.east.calls.filter((call) => call.operation === 'delete').length, 0);
});
