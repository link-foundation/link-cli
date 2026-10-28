import assert from 'node:assert/strict';
import test from 'node:test';
import { ReclaimingPartitionedJournalStore as Reclaiming } from '../src/index.js';
import { MemoryMaintenanceStore } from '../../../experiments/issue-138/reclamation-memory.mjs';
const keys = ['a', 'b'];
async function layout(chunked) {
  const catalog = new MemoryMaintenanceStore('catalog'); const east = new MemoryMaintenanceStore('east'); const west = new MemoryMaintenanceStore('west');
  const store = await Reclaiming.open(catalog, { east, west }, (key) => key === 'a' ? 'east' : 'west', { chunks: chunked ? { maxRecords: 2, maxBytes: 64, maxChunks: 32 } : null });
  return { catalog, east, west, store };
}
async function write(store, a, b) {
  assert.ok(await store.compareExchange(keys, await store.read(keys), [[a], [b]])); return store.read(keys);
}
const count = (provider, operation) => provider.calls.filter((call) => call.operation === operation).length;
test('mismatched arrays reject before I/O and preserve following writes', { timeout: 10000 }, async () => {
  for (const chunked of [false, true]) {
    const { catalog, east, west, store } = await layout(chunked); const expected = await write(store, 'old-a', 'old-b');
    const providers = [catalog, east, west]; const before = providers.map((p) => p.calls.length);
    for (const [guards, updates] of [[1, 2], [3, 2], [2, 1], [2, 3]]) {
      await assert.rejects(store.compareExchange(keys, Array(guards).fill(expected[0]), Array(updates).fill(['must-not-stage'])), TypeError);
      assert.deepEqual(providers.map((p) => p.calls.length), before);
    }
    assert.deepEqual(await store.read(keys), expected); await write(store, 'following-a', 'following-b');
  }
});
test('generation zero with payload never becomes an absent value', { timeout: 10000 }, async () => {
  for (const chunked of [false, true]) for (const phase of ['shard', 'epoch', 'object']) {
    const { catalog, east, west, store } = await layout(chunked); const expected = await write(store, 'live-a', 'live-b');
    const provider = phase === 'object' ? east : catalog; const key = phase === 'object' ? [...east.values.keys()][0] : phase === 'epoch' ? '__link_gc_epoch' : 'a';
    const saved = provider.values.get(key); provider.values.set(key, { ...saved, generation: '0' });
    const reads = count(east, 'read'); const swaps = count(catalog, 'swap');
    await assert.rejects(store.read(keys)); assert.equal(count(east, 'read') - reads, phase === 'object' ? 1 : 0);
    assert.equal(count(catalog, 'swap'), swaps); assert.equal(count(east, 'delete') + count(west, 'delete'), 0);
    provider.values.set(key, saved); assert.deepEqual(await store.read(keys), expected); await write(store, 'following-a', 'following-b');
  }
});
test('noncanonical object keys reject before download or deletion', { timeout: 10000 }, async () => {
  for (const chunked of [false, true]) for (const position of [6, 21, 23, 38]) {
    const { catalog, east, west, store } = await layout(chunked); await write(store, 'orphan-a', 'orphan-b');
    const expected = await write(store, 'live-a', 'live-b'); const original = catalog.values.get('a');
    const keyIndex = chunked ? 4 : 2; const entries = [...original.entries]; const goodKey = entries[keyIndex]; const badKey = goodKey.slice(0, position) + 'F' + goodKey.slice(position + 1);
    entries[keyIndex] = badKey; catalog.values.set('a', { ...original, entries }); const reads = count(east, 'read') + count(west, 'read');
    await assert.rejects(store.read(keys)); assert.equal(count(east, 'read') + count(west, 'read'), reads);
    await assert.rejects(store.collectGarbage()); assert.equal(count(east, 'delete') + count(west, 'delete'), 0); catalog.values.set('a', original);
    assert.deepEqual(await store.read(keys), expected); east.values.set(badKey, east.values.get(goodKey));
    await assert.rejects(store.collectGarbage()); assert.equal(count(east, 'delete') + count(west, 'delete'), 0); east.values.delete(badKey);
    await write(store, 'following-a', 'following-b');
  }
});
test('unchanged pointer cannot turn a missing object into a refresh', { timeout: 10000 }, async () => {
  for (const chunked of [false, true]) {
    const { catalog, east, west, store } = await layout(chunked); const expected = await write(store, 'live-a', 'live-b'); const original = catalog.values.get('a');
    const [objectKey, saved] = [...east.values][0];
    east.on('read', async () => { assert.ok(await catalog.compareExchange(['a'], [original], [original.entries])); east.values.delete(objectKey); });
    const reads = [count(catalog, 'read'), count(east, 'read'), count(west, 'read')];
    await assert.rejects(store.read(keys)); assert.deepEqual([count(catalog, 'read'), count(east, 'read'), count(west, 'read')], [reads[0] + 2, reads[1] + 1, reads[2]]);
    east.values.set(objectKey, saved); const fresh = await store.read(keys); assert.deepEqual(fresh[0].entries, expected[0].entries); assert.notEqual(fresh[0].generation, expected[0].generation);
    await write(store, 'following-a', 'following-b');
  }
});
