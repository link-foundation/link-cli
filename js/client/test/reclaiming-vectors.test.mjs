import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { ReclaimingPartitionedJournalStore as Reclaiming } from '../src/index.js';
import { MemoryMaintenanceStore } from '../../../experiments/issue-138/reclamation-memory.mjs';
const vectors = JSON.parse(readFileSync(new URL('../../../docs/protocol/reclamation-v1.json', import.meta.url)));
const epochKey = '__link_gc_epoch';
function providers(epoch = '0') {
  const catalog = new MemoryMaintenanceStore('catalog');
  catalog.values.set(epochKey, { generation: '1', entries: ['gc1', epoch] }); catalog.revision = 1n;
  return { catalog, east: new MemoryMaintenanceStore('east'), west: new MemoryMaintenanceStore('west') };
}
function open(p) { return Reclaiming.open(p.catalog, { east: p.east, west: p.west }, () => 'east'); }
for (const vector of vectors.objects) test(`blob2 independent address ${vector.key}`, { timeout: 10000 }, async () => {
  const p = providers(vector.epoch); const store = await open(p); const expected = await store.read(['a']);
  assert.ok(await store.compareExchange(['a'], expected, [vector.entries]));
  assert.deepEqual([...p.east.values.keys()], [vector.key]);
  assert.deepEqual((await store.read(['a']))[0].entries, vector.entries);
});
for (const [index, vector] of vectors.epochs.entries()) test(`reclamation epoch vector ${index}`, { timeout: 10000 }, async () => {
  const p = providers(); p.catalog.values.get(epochKey).entries = vector.entries;
  if (vector.valid) assert.ok(await open(p)); else await assert.rejects(open(p));
  assert.equal(p.catalog.calls.filter((call) => call.operation === 'swap').length, 0);
});
for (const [index, vector] of vectors.pointers.entries()) test(`reclamation pointer vector ${index}`, { timeout: 10000 }, async () => {
  const p = providers('18446744073709551615');
  p.catalog.values.set('a', { generation: '1', entries: vector.entries });
  if (vector.valid) {
    const parts = vector.entries; p[parts[1]].values.set(parts[2], { generation: parts[3], entries: vector.objectEntries });
    const store = await open(p); assert.deepEqual((await store.read(['a']))[0].entries, vector.objectEntries);
  } else await assert.rejects(open(p));
  assert.equal(p.catalog.calls.filter((call) => call.operation === 'swap').length, 0);
});
