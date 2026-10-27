// Original minimum control; run-reclaiming-chunks-before.sh supplies old source.
import assert from 'node:assert/strict';
import test from 'node:test';
import { ReclaimingPartitionedJournalStore } from '../../js/client/src/index.js';
import { MemoryMaintenanceStore } from './reclamation-memory.mjs';

test('selected reclaiming chunks reuse within an epoch and restage after a fence', { timeout: 10000 }, async () => {
  const catalog = new MemoryMaintenanceStore('catalog');
  const data = new MemoryMaintenanceStore('data');
  const store = await ReclaimingPartitionedJournalStore.open(catalog, { east: data }, () => 'east',
    { chunks: { maxRecords: 2, maxBytes: 64, maxChunks: 32 }, cacheBytes: 256 });
  const keys = ['a']; const seed = ['aa', 'bb', 'cc', 'dd'];
  assert.equal(await store.compareExchange(keys, await store.read(keys), [seed]), true);
  assert.equal(catalog.values.get('a').entries[0], 'chunks2');
  const original = structuredClone(catalog.values.get('a'));
  assert.equal(original.entries[3], '2');
  assert.equal(await store.compareExchange(keys, await store.read(keys), [[...seed, 'ee']]), true);
  const appended = structuredClone(catalog.values.get('a'));
  assert.deepEqual(appended.entries.slice(4, 10), original.entries.slice(4, 10));
  assert.equal(data.calls.filter(call => call.operation === 'swap').length, 3);
  const owned = await store.read(keys);
  assert.equal((await store.collectGarbage()).epoch, '1');
  assert.equal(await store.compareExchange(keys, owned, [[...seed, 'ee', 'ff']]), true);
  const fresh = catalog.values.get('a').entries;
  for (let i = 4; i < fresh.length; i += 3) assert.ok(fresh[i].startsWith('blob2-0000000000000001-'));
  assert.deepEqual(owned[0].entries, [...seed, 'ee']);
  assert.equal((await store.collectGarbage()).deleted, 3);
  assert.deepEqual((await store.read(keys))[0].entries, [...seed, 'ee', 'ff']);
});
