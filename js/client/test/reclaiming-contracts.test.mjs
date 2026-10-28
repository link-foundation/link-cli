import assert from 'node:assert/strict';
import test from 'node:test';
import { ReclaimingPartitionedJournalStore as Reclaiming } from '../src/index.js';
import { MemoryMaintenanceStore } from '../../../experiments/issue-138/reclamation-memory.mjs';
const epochKey = '__link_gc_epoch'; const keys = ['a', 'b'];
const providers = () => ({ catalog: new MemoryMaintenanceStore('catalog'), east: new MemoryMaintenanceStore('east'), west: new MemoryMaintenanceStore('west') });
const open = (p) => Reclaiming.open(p.catalog, { east: p.east, west: p.west }, (key) => key === 'a' ? 'east' : 'west');
async function write(store, a, b) { assert.ok(await store.compareExchange(keys, await store.read(keys), [[a], [b]])); }
test('invalid partition configuration performs no I/O', { timeout: 10000 }, async () => {
  for (const name of [null, '', 'bad/name', 'a'.repeat(129)]) {
    const p = providers();
    await assert.rejects(Reclaiming.open(p.catalog, name === null ? {} : { [name]: p.east }, () => 'east'), TypeError);
    assert.deepEqual(p.catalog.calls, []); assert.deepEqual(p.east.calls, []);
  }
});
test('all routes validate before I/O and read-only dependencies need no route', { timeout: 10000 }, async () => {
  const p = providers(); const store = await Reclaiming.open(p.catalog, { east: p.east }, (key) => key === 'a' ? 'east' : 'unconfigured');
  const expected = await store.read(keys); const counts = [p.catalog.calls.length, p.east.calls.length];
  await assert.rejects(store.compareExchange(keys, expected, [['first'], ['second']]), TypeError);
  assert.deepEqual([p.catalog.calls.length, p.east.calls.length], counts);
  assert.ok(await store.compareExchange(keys, expected, [null, null])); assert.equal(p.east.values.size, 0);
  assert.ok(await store.compareExchange(keys, expected, [['following'], null])); assert.deepEqual((await store.read(keys))[0].entries, ['following']);
});
test('durability requires every provider without sending requests', { timeout: 10000 }, async () => {
  for (let mask = 0; mask < 8; mask++) {
    const p = providers(); const store = await open(p); const selected = Object.values(p);
    selected.forEach((provider, bit) => { provider.supportsDurableFlush = (mask & (1 << bit)) !== 0; });
    const counts = selected.map((provider) => provider.calls.length);
    assert.equal(store.supportsDurableFlush, mask === 7); assert.deepEqual(selected.map((provider) => provider.calls.length), counts);
  }
});
test('failed initialization confirmation preserves one acknowledged decision', { timeout: 10000 }, async () => {
  const p = providers(); p.catalog.on('swap', () => p.catalog.on('scan', () => { throw Error('initialization confirmation unavailable'); }), { after: true });
  await assert.rejects(open(p), /initialization confirmation unavailable/);
  assert.deepEqual(p.catalog.values.get(epochKey).entries, ['gc1', '0']); assert.equal(p.catalog.calls.filter((call) => call.operation === 'swap').length, 1);
  const fresh = await open(p); assert.deepEqual((await fresh.read(keys)).map((item) => item.generation), ['0', '0']);
  assert.equal(p.catalog.calls.filter((call) => call.operation === 'swap').length, 1);
});
test('failed staging confirmation leaves collectable orphan without publication', { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p); const expected = await store.read(['a']);
  p.east.on('swap', () => p.east.on('read', () => { throw Error('staging confirmation unavailable'); }), { after: true });
  await assert.rejects(store.compareExchange(['a'], expected, [['orphan']]), /staging confirmation unavailable/);
  assert.equal(p.east.calls.filter((call) => call.operation === 'swap').length, 1); assert.equal(p.east.values.size, 1);
  assert.equal(p.catalog.calls.filter((call) => call.operation === 'swap').length, 1); assert.equal(p.catalog.values.has('a'), false);
  assert.deepEqual(await store.read(['a']), expected); assert.equal((await store.collectGarbage()).deleted, 1); assert.equal(p.east.values.size, 0);
  await write(store, 'following', 'b');
});
for (const phase of ['catalog', 'data']) test(`interrupted ${phase} refresh returns no partial group`, { timeout: 10000 }, async () => {
  const p = providers(); const writer = await open(p); const reader = await open(p); await write(writer, 'old-a', 'old-b');
  p.west.on('read', async () => {
    await write(writer, 'new-a', 'new-b'); assert.equal((await writer.collectGarbage()).deleted, 2);
    (phase === 'catalog' ? p.catalog : p.east).on('read', () => { throw Error('refresh unavailable'); });
  });
  await assert.rejects(reader.read(keys), /refresh unavailable/);
  assert.equal(p.east.calls.filter((call) => call.operation === 'delete').length + p.west.calls.filter((call) => call.operation === 'delete').length, 2);
  assert.deepEqual((await reader.read(keys)).map((item) => item.entries), [['new-a'], ['new-b']]);
});
