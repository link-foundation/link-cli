import assert from 'node:assert/strict';
import test from 'node:test';
import { ReclaimingPartitionedJournalStore as Reclaiming } from '../src/index.js';
import { MemoryMaintenanceStore as Memory } from '../../../experiments/issue-138/reclamation-memory.mjs';
const fixture = () => ({ catalog: new Memory('catalog'), east: new Memory('east'), west: new Memory('west') });
const open = (p, options = {}) => Reclaiming.open(p.catalog, { east: p.east, west: p.west }, () => 'east', { chunks: { maxRecords: 1 }, ...options });
const publish = async (store, records) => { assert.ok(await store.compareExchange(['a'], await store.read(['a']), [records])); };
const deletes = (p) => p.east.calls.filter((c) => c.operation === 'delete').length;

test('every four-chunk cache mask returns one whole catalog snapshot after retirement', { timeout: 10000 }, async () => {
  for (let mask = 0; mask < 16; mask++) {
    const p = fixture(); const writer = await open(p); const reader = await open(p, { cacheBytes: 4096 });
    const old = [['a-old', 'a-tail'], ['b-old', 'b-tail']]; const fresh = [['a-new', 'a-next'], ['b-new', 'b-next']];
    assert.ok(await writer.compareExchange(['a', 'b'], await writer.read(['a', 'b']), old));
    // Canonical single-reference fixture roots warm exactly the selected bits
    // through the public reader. Remove them before the retirement schedule.
    for (let bit = 0; bit < 4; bit++) if (mask & (1 << bit)) {
      const root = p.catalog.values.get(bit < 2 ? 'a' : 'b'); const offset = 4 + (bit % 2) * 3;
      p.catalog.values.set('warm', { generation: root.generation, entries: ['chunks2', root.entries[1], '1', '1', ...root.entries.slice(offset, offset + 3)] });
      assert.deepEqual((await reader.read(['warm']))[0].entries, [old[Math.floor(bit / 2)][bit % 2]]);
    }
    p.catalog.values.delete('warm');
    p.catalog.on('read', async () => {
      assert.ok(await writer.compareExchange(['a', 'b'], await writer.read(['a', 'b']), fresh));
      assert.equal((await writer.collectGarbage()).deleted, 4);
    }, { after: true });
    const actual = (await reader.read(['a', 'b'])).map((value) => value.entries);
    assert.deepEqual(actual, mask === 15 ? old : fresh, `cache mask ${mask}`);
    assert.deepEqual((await reader.read(['a', 'b'])).map((value) => value.entries), fresh);
  }
});

test('a second chunk retirement stops after one complete group retry', { timeout: 10000 }, async () => {
  const p = fixture(); const writer = await open(p); const second = await open(p); const reader = await open(p);
  await publish(writer, ['zero', 'zero-tail']);
  p.east.on('read', async () => {
    await publish(writer, ['one', 'one-tail']); await writer.collectGarbage();
    p.east.on('read', async () => { await publish(second, ['two', 'two-tail']); await second.collectGarbage(); });
  });
  await assert.rejects(reader.read(['a']), /bounded retry/);
  assert.deepEqual((await reader.read(['a']))[0].entries, ['two', 'two-tail']);
});

test('overlapping collectors retain newer orphan chunks until a later own fence', { timeout: 10000 }, async () => {
  const p = fixture(); const writer = await open(p); const collector = await open(p); const other = await open(p);
  await publish(writer, ['live', 'tail']);
  p.catalog.on('scan', async () => {
    p.catalog.on('swap', () => { throw Error('lost publication'); });
    await assert.rejects(publish(writer, ['orphan', 'orphan-tail']), /lost publication/);
    p.east.on('scan', () => { throw Error('incomplete next collector'); });
    await assert.rejects(other.collectGarbage(), /incomplete next collector/);
  });
  assert.deepEqual(await collector.collectGarbage(), { epoch: '1', examined: 4, deleted: 0 });
  assert.equal([...p.east.values.keys()].filter((key) => key.startsWith('blob2-0000000000000001-')).length, 2);
  assert.equal((await collector.collectGarbage()).deleted, 2);
  assert.deepEqual((await writer.read(['a']))[0].entries, ['live', 'tail']);
});

test('complete later namespace validation and exact generations guard chunk deletion', { timeout: 10000 }, async () => {
  const p = fixture(); const store = await open(p); await publish(store, ['old', 'old-tail']);
  const oldKey = p.catalog.values.get('a').entries[4]; await publish(store, ['live', 'live-tail']);
  p.west.on('scan', () => { throw Error('incomplete later namespace'); });
  await assert.rejects(store.collectGarbage(), /incomplete later namespace/); assert.equal(deletes(p), 0);
  p.east.on('delete', () => { p.east.revision++; p.east.values.set(oldKey, { generation: String(p.east.revision), entries: ['ABA replacement'] }); });
  assert.equal((await store.collectGarbage()).deleted, 1); assert.deepEqual(p.east.values.get(oldKey).entries, ['ABA replacement']);
  assert.deepEqual((await store.read(['a']))[0].entries, ['live', 'live-tail']);
});

test('mixed whole-object readers and live-empty chunk roots preserve following edits', { timeout: 10000 }, async () => {
  const p = fixture(); const legacy = await open(p, { chunks: null }); await publish(legacy, ['legacy']);
  assert.equal(p.catalog.values.get('a').entries[0], 'blob2'); const chunked = await open(p);
  const owned = await chunked.read(['a']); assert.deepEqual(owned[0].entries, ['legacy']);
  assert.ok(await chunked.compareExchange(['a'], owned, [[]]));
  assert.deepEqual(p.catalog.values.get('a').entries, ['chunks2', 'east', '0', '0']);
  assert.equal((await chunked.collectGarbage()).deleted, 1); assert.equal(p.east.values.size, 0);
  assert.deepEqual((await legacy.read(['a']))[0].entries, []);
  assert.equal(await chunked.compareExchange(['a'], owned, [['stale']]), false);
  await publish(chunked, ['following', 'tail']); assert.deepEqual((await legacy.read(['a']))[0].entries, ['following', 'tail']);
});
