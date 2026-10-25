import assert from 'node:assert/strict';
import test from 'node:test';
import { PartitionedJournalStore } from '../src/partitioned.js';

class Store {
  supportsDurableFlush = true; values = new Map(); revision = 0; reads = 0; fail; loseReply;
  async read(keys) {
    this.reads++; if (this.fail) throw Error('unavailable');
    return keys.map((key) => structuredClone(this.values.get(key) ?? { generation: '0', entries: [] }));
  }
  async compareExchange(keys, expected, updates) {
    if (this.fail) throw Error('unavailable');
    if (keys.some((key, i) => (this.values.get(key)?.generation ?? '0') !== expected[i].generation)) return false;
    const generation = String(++this.revision);
    keys.forEach((key, i) => { if (updates[i] !== null) this.values.set(key, { generation, entries: [...updates[i]] }); });
    if (this.loseReply) throw Error('reply lost');
    return true;
  }
}
function fixture() {
  const catalog = new Store(); const east = new Store(); const west = new Store();
  const open = (cacheBytes = 0) => new PartitionedJournalStore(catalog, { east, west }, (key) => key === 'a' ? 'east' : 'west', { cacheBytes });
  const writer = open();
  const put = async (key, entries) => assert.equal(await writer.compareExchange([key], await writer.read([key]), [entries]), true);
  return { catalog, east, west, open, put };
}
test('immutable cache retains fresh catalog reads, exact generations and owned caller arrays', async () => {
  const { catalog, east, open, put } = fixture(); await put('a', ['é']);
  const reader = open(4); const start = east.reads; const first = await reader.read(['a']);
  assert.equal(east.reads, start + 1); first[0].entries[0] = 'caller mutation';
  east.fail = true;
  assert.deepEqual((await reader.read(['a']))[0].entries, ['é']); assert.equal(east.reads, start + 1);
  east.fail = false;
  await put('a', ['x']); const next = await reader.read(['a']);
  assert.notEqual(next[0].generation, first[0].generation); assert.deepEqual(next[0].entries, ['x']);
  assert.equal(await reader.compareExchange(['a'], first, [null]), false);
  catalog.fail = true; await assert.rejects(reader.read(['a']), /unavailable/); catalog.fail = false;
  assert.deepEqual((await reader.read(['a']))[0].entries, ['x']);
});
test('empty objects have a positive weight and partition identities never alias', async () => {
  for (const budget of [1, 2]) {
    const { catalog, east, west, open, put } = fixture();
    await put('a', []); await put('b', []); await put('c', ['é']);
    const reader = open(budget); const start = east.reads + west.reads;
    await reader.read(['a']); await reader.read(['b']); await reader.read(['a']);
    assert.equal(east.reads + west.reads, start + (budget === 1 ? 3 : 2));
    await reader.read(['c']); await reader.read(['a']); // oversize object must not evict a
    assert.equal(east.reads + west.reads, start + (budget === 1 ? 4 : 3));
    const pointer = catalog.values.get('a');
    catalog.values.set('a', { ...pointer, generation: '99' });
    assert.equal((await reader.read(['a']))[0].generation, '99');
    catalog.values.delete('a');
    assert.deepEqual(await reader.read(['a']), [{ generation: '0', entries: [] }]);
  }
});
test('a failed object lookup is retried after repair and staging always checks full contents', async () => {
  const { catalog, east, open, put } = fixture(); await put('a', ['é']);
  const [address, object] = [...east.values][0]; const reader = open(4); const start = east.reads;
  east.values.set(address, { ...object, entries: ['broken'] });
  await assert.rejects(reader.read(['a']), /modified/);
  east.values.set(address, object);
  const expected = await reader.read(['a']); await reader.read(['a']); assert.equal(east.reads, start + 2);
  east.values.set(address, { ...object, entries: ['broken'] });
  await assert.rejects(reader.compareExchange(['a'], expected, [['é']]), /collision/);
  assert.equal(catalog.values.get('a').generation, expected[0].generation);
  east.values.set(address, object);
  assert.equal(await reader.compareExchange(['a'], expected, [['é']]), true);
  assert.notEqual((await reader.read(['a']))[0].generation, expected[0].generation);
});
test('rejected and uncertain publications are resolved from fresh catalog state', async () => {
  const { catalog, open, put } = fixture(); await put('a', ['old']); const reader = open(20);
  const old = await reader.read(['a']); await put('a', ['new']);
  assert.equal(await reader.compareExchange(['a'], old, [['orphan']]), false);
  const current = await reader.read(['a']); assert.deepEqual(current[0].entries, ['new']);
  catalog.fail = true; await assert.rejects(reader.compareExchange(['a'], current, [['next']]), /unavailable/);
  catalog.fail = false; assert.deepEqual((await reader.read(['a']))[0].entries, ['new']);
  catalog.loseReply = true; await assert.rejects(reader.compareExchange(['a'], current, [['next']]), /reply lost/);
  catalog.loseReply = false; assert.deepEqual((await reader.read(['a']))[0].entries, ['next']);
});
test('concurrent misses account for a single retained copy', async () => {
  const { east, west, open, put } = fixture(); await put('a', ['']); await put('b', []);
  const reader = open(3); const start = east.reads + west.reads;
  const [first, second] = await Promise.all([reader.read(['a']), reader.read(['a'])]);
  assert.equal(east.reads + west.reads, start + 2);
  first[0].entries[0] = 'caller'; assert.deepEqual(second[0].entries, ['']);
  await reader.read(['b']); await reader.read(['a']); await reader.read(['b']);
  assert.equal(east.reads + west.reads, start + 3);
});
test('invalid cache budgets are rejected before any storage access', () => {
  const { catalog, east, west } = fixture();
  for (const cacheBytes of [-1, 1.5, NaN, Infinity, Number.MAX_SAFE_INTEGER + 1, '4', null]) {
    assert.throws(() => new PartitionedJournalStore(catalog, { east, west }, () => 'east', { cacheBytes }), /cacheBytes/);
  }
  assert.equal(catalog.reads + east.reads + west.reads, 0);
});
test('finite independent LRU oracle matches Unicode payload costs', async () => {
  const rows = [[], [''], ['é'], ['abc'], ['雪'], ['😀']]; const costs = [1, 2, 4, 5, 5, 6]; const ids = ['a', 'b', 'c', 'd', 'e', 'f'];
  for (const budget of [0, 1, 2, 4, 9, 17]) {
    const { east, west, open, put } = fixture();
    for (const [index, id] of ids.entries()) await put(id, rows[index]);
    const reader = open(budget); const start = east.reads + west.reads;
    const order = []; let used = 0; let misses = 0; let seed = 138;
    for (let step = 0; step < 128; step++) {
      seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; const selected = (seed >>> 16) % 6;
      const position = order.indexOf(selected);
      if (position !== -1) { order.splice(position, 1); order.push(selected); }
      else {
        misses++;
        if (costs[selected] <= budget) {
          while (used + costs[selected] > budget) used -= costs[order.shift()];
          used += costs[selected]; order.push(selected);
        }
      }
      assert.deepEqual((await reader.read([ids[selected]]))[0].entries, rows[selected]);
      assert.equal(east.reads + west.reads, start + misses);
    }
  }
});
test('UTF-8 payload budgets and LRU promotion bound immutable object retention', async () => {
  const { east, west, open, put } = fixture(); await put('a', ['é']); await put('b', ['abc']); await put('c', ['']);
  const reader = open(9); const start = east.reads + west.reads;
  for (const key of ['a', 'b', 'a', 'c']) await reader.read([key]);
  assert.equal(east.reads + west.reads, start + 3);
  await reader.read(['b']); assert.equal(east.reads + west.reads, start + 4);
  await reader.read(['c']); assert.equal(east.reads + west.reads, start + 4);
  await reader.read(['a']); assert.equal(east.reads + west.reads, start + 5);
});
test('zero, oversized and mismatched generation lookups never hit or cache failures', async () => {
  for (const [budget, downloads] of [[0, 2], [3, 2], [4, 1]]) {
    const { catalog, east, open, put } = fixture(); await put('a', ['é']);
    const reader = open(budget); const start = east.reads;
    await reader.read(['a']); await reader.read(['a']); assert.equal(east.reads, start + downloads);
    const pointer = catalog.values.get('a'); pointer.entries[3] = '99';
    await assert.rejects(reader.read(['a']), /object/); await assert.rejects(reader.read(['a']), /object/);
    assert.equal(east.reads, start + downloads + 2);
  }
});
