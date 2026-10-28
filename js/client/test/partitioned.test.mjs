import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { PartitionedJournalStore } from '../src/partitioned.js';

class Store {
  supportsDurableFlush = true;
  values = new Map(); writes = []; revision = 0; fail; after;
  async read(keys) { if (this.fail) throw Error('partition unavailable'); return keys.map((key) => structuredClone(this.values.get(key) ?? { generation: '0', entries: [] })); }
  async compareExchange(keys, expected, updates) {
    if (this.fail) throw Error('partition unavailable');
    if (keys.some((key, i) => (this.values.get(key)?.generation ?? '0') !== expected[i].generation)) return false;
    const generation = String(++this.revision);
    keys.forEach((key, i) => { if (updates[i] !== null) this.values.set(key, { generation, entries: [...updates[i]] }); });
    this.writes.push(structuredClone(updates)); this.after?.(); return true;
  }
}
function fixture() {
  const metadata = new Store(); const a = new Store(); const b = new Store();
  return { metadata, a, b, store: new PartitionedJournalStore(metadata, { east: a, west: b }, (key) => key === 'a' ? 'east' : 'west') };
}
test('portable object addresses use UTF-8 byte lengths and unambiguous entry framing', async () => {
  const vectors = JSON.parse(readFileSync(new URL('../../../docs/protocol/partitioned-journal-vectors.json', import.meta.url), 'utf8'));
  for (const { entries, address } of vectors) {
    const { a, store } = fixture();
    await store.compareExchange(['a'], await store.read(['a']), [entries]);
    assert.deepEqual([...a.values.keys()], [address]);
    assert.deepEqual((await store.read(['a']))[0].entries, entries);
  }
});
test('partitioned journals publish only small atomic pointers and preserve read-only dependencies', async () => {
  const { metadata, a, b, store } = fixture();
  const base = await store.read(['a', 'b']);
  assert.equal(store.supportsDurableFlush, true);
  assert.equal(await store.compareExchange(['a', 'b'], base, [['journal-é'], ['second']]), true);
  assert.equal(a.values.size, 1); assert.equal(b.values.size, 1);
  assert.equal(JSON.stringify([...metadata.values.values()]).includes('journal-é'), false);
  const stale = await store.read(['a', 'b']);
  assert.deepEqual(stale.map((v) => v.entries), [['journal-é'], ['second']]);
  assert.equal(await store.compareExchange(['b'], [stale[1]], [['changed']]), true);
  assert.equal(await store.compareExchange(['a', 'b'], stale, [['abandoned'], null]), false);
  assert.deepEqual((await store.read(['a', 'b'])).map((v) => v.entries), [['journal-é'], ['changed']]);
  // Staged objects are immutable and reusable, even when another commit lost.
  const fresh = await store.read(['a']); const count = a.values.size;
  assert.equal(await store.compareExchange(['a'], fresh, [['abandoned']]), true);
  assert.equal(a.values.size, count);
});
test('data failure cannot publish metadata and a lost metadata reply is discovered fresh', async () => {
  const { metadata, b, store } = fixture(); const base = await store.read(['a', 'b']);
  b.fail = true;
  await assert.rejects(store.compareExchange(['a', 'b'], base, [['one'], ['two']]), /unavailable/);
  assert.equal(metadata.values.size, 0);
  b.fail = false; metadata.after = () => { throw Error('reply lost'); };
  await assert.rejects(store.compareExchange(['a', 'b'], base, [['one'], ['two']]), /reply lost/);
  assert.deepEqual((await store.read(['a', 'b'])).map((v) => v.entries), [['one'], ['two']]);
  b.fail = true; await assert.rejects(store.read(['a', 'b']), /unavailable/);
});
test('missing, modified or malformed referenced objects fail closed', async () => {
  const { metadata, a, store } = fixture();
  await store.compareExchange(['a'], await store.read(['a']), [['one']]);
  const [key, blob] = [...a.values][0];
  a.values.delete(key); await assert.rejects(store.read(['a']), /object/);
  a.values.set(key, { ...blob, generation: '99' }); await assert.rejects(store.read(['a']), /object/);
  a.values.set(key, { ...blob, entries: ['different'] }); await assert.rejects(store.read(['a']), /object/);
  const current = metadata.values.get('a');
  for (const entries of [[], ['other', 'east', key, blob.generation], ['blob1', 'absent', key, '1'], ['blob1', 'east', '../bad', '1'], ['blob1', 'east', key, '01']]) {
    metadata.values.set('a', { ...current, entries }); await assert.rejects(store.read(['a']), /pointer/);
  }
});

test('configuration, malformed stores and collisions fail before publication', async () => {
  for (const args of [[null, {}, () => 'east'], [new Store(), null, () => 'east'], [new Store(), {}, null], [new Store(), {}, () => 'east'], [new Store(), { '../bad': new Store() }, () => 'east'], [new Store(), { east: {} }, () => 'east']])
    assert.throws(() => new PartitionedJournalStore(...args), TypeError);
  const { metadata, a, store } = fixture();
  for (const keys of [null, [], ['a', 'a'], ['../bad'], Array(33).fill('a')]) await assert.rejects(store.read(keys), TypeError);
  for (const [expected, updates] of [[null, []], [[], null], [[], []], [[{ generation: '01' }], [[]]], [[{ generation: '18446744073709551616' }], [[]]], [[{ generation: '0' }], [[7]]]])
    await assert.rejects(store.compareExchange(['a'], expected, updates), TypeError);
  const wrongRoute = new PartitionedJournalStore(metadata, { east: a }, () => 'absent');
  await assert.rejects(wrongRoute.compareExchange(['a'], [{ generation: '0' }], [[]]), /route/);
  metadata.supportsDurableFlush = false; assert.equal(store.supportsDurableFlush, false);
  metadata.supportsDurableFlush = true; a.supportsDurableFlush = false; assert.equal(store.supportsDurableFlush, false);
  for (const value of [null, [], [{ generation: '-1', entries: [] }], [{ generation: '0', entries: ['bad'] }], [{ generation: '1', entries: [false] }]]) {
    metadata.read = async () => value; await assert.rejects(store.read(['a']));
  }
  // A competing writer can create an object after our absent read.
  for (const competing of ['same', 'different', 'absent']) {
    const f = fixture();
    f.a.compareExchange = async (keys, expected, updates) => {
      if (competing !== 'absent') f.a.values.set(keys[0], { generation: '1', entries: competing === 'same' ? updates[0] : ['collision'] });
      return false;
    };
    if (competing === 'same') assert.equal(await f.store.compareExchange(['a'], await f.store.read(['a']), [['content']]), true);
    else await assert.rejects(f.store.compareExchange(['a'], await f.store.read(['a']), [['content']]), /collision/);
  }
  for (const value of [[], null, [{ generation: '-1', entries: [] }]]) {
    const f = fixture(); f.a.read = async () => value;
    await assert.rejects(f.store.compareExchange(['a'], [{ generation: '0' }], [[]]), /object/);
  }
});
