import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { PartitionedJournalStore } from '../src/partitioned.js';

class Store {
  supportsDurableFlush = true;
  values = new Map(); reads = 0; writes = 0; payloadBytes = 0; revision = 0; after; before;
  async read(keys) { this.reads++; return keys.map((key) => structuredClone(this.values.get(key) ?? { generation: '0', entries: [] })); }
  async compareExchange(keys, expected, updates) {
    this.before?.();
    if (keys.some((key, i) => (this.values.get(key)?.generation ?? '0') !== expected[i].generation)) return false;
    const generation = String(++this.revision);
    keys.forEach((key, i) => {
      if (updates[i] !== null) {
        this.values.set(key, { generation, entries: [...updates[i]] });
        this.payloadBytes += updates[i].reduce((sum, record) => sum + Buffer.byteLength(record), 0);
      }
    });
    this.writes++; this.after?.(); return true;
  }
}
function fixture(chunks = { maxRecords: 2, maxBytes: 8, maxChunks: 32 }, cacheBytes = 0) {
  const catalog = new Store(); const data = new Store();
  const store = new PartitionedJournalStore(catalog, { east: data }, () => 'east', { chunks, cacheBytes });
  return { catalog, data, store };
}

test('selected journal chunks match independent shared byte vectors', async () => {
  const vectors = JSON.parse(readFileSync(new URL('../../../docs/protocol/chunked-journal-vectors.json', import.meta.url), 'utf8'));
  for (const v of vectors) {
    const { catalog, data, store } = fixture({ maxRecords: v.maxRecords, maxBytes: v.maxBytes, maxChunks: 32 });
    assert.equal(await store.compareExchange(['a'], await store.read(['a']), [v.entries]), true);
    const manifest = catalog.values.get('a').entries;
    assert.deepEqual(manifest, ['chunks1', 'east', String(v.entries.length), String(v.chunks.length),
      ...v.chunks.flatMap((chunk, i) => [chunk.address, String(i + 1), String(chunk.entries.length)])]);
    assert.deepEqual([...data.values.keys()], v.chunks.map((chunk) => chunk.address));
    assert.deepEqual((await store.read(['a']))[0].entries, v.entries);
    // An upgraded reader can read manifests without selecting chunk writes.
    const legacy = new PartitionedJournalStore(catalog, { east: data }, () => 'east');
    assert.deepEqual((await legacy.read(['a']))[0].entries, v.entries);
  }
});

test('chunk appends reuse complete prefixes, preserve old snapshots and cache owned reads', async () => {
  const { catalog, data, store } = fixture(undefined, 256);
  const seed = ['aa', 'bb', 'cc', 'dd'];
  assert.equal(await store.compareExchange(['a'], await store.read(['a']), [seed]), true);
  const oldPointer = structuredClone(catalog.values.get('a'));
  const owned = await store.read(['a']);
  const readCount = data.reads; await store.read(['a']); assert.equal(data.reads, readCount);
  const bytes = data.payloadBytes; const writes = data.writes;
  assert.equal(await store.compareExchange(['a'], owned, [[...seed, 'ee']]), true);
  assert.equal(data.payloadBytes - bytes, 2); assert.equal(data.writes - writes, 1);
  const fresh = await store.read(['a']); assert.deepEqual(fresh[0].entries, [...seed, 'ee']);
  fresh[0].entries[0] = 'caller'; assert.deepEqual((await store.read(['a']))[0].entries, [...seed, 'ee']);
  assert.deepEqual(owned[0].entries, seed);
  catalog.values.set('a', oldPointer); assert.deepEqual((await store.read(['a']))[0].entries, seed);
  // A cached object never bypasses fresh complete staging validation.
  const firstKey = oldPointer.entries[4]; const prior = structuredClone(data.values.get(firstKey));
  data.values.set(firstKey, { ...prior, entries: ['collision'] });
  await assert.rejects(store.compareExchange(['a'], owned, [seed]), /collision/);
  data.values.set(firstKey, prior);
});

test('all chunk options, journals and routes are admitted before storage access', async () => {
  for (const value of [false, [], 2, 'bad', { maxRecords: 0 }, { maxRecords: 65537 }, { maxRecords: 1.5 }, { maxBytes: -1 }, { maxBytes: 8388609 }, { maxChunks: 0 }, { maxChunks: 4097 }, { maxBytes: NaN }])
    assert.throws(() => fixture(value), TypeError);
  const { catalog, data, store } = fixture({ maxRecords: 1, maxBytes: 4, maxChunks: 2 });
  for (const records of [['12345'], ['a', 'b', 'c'], ['\ud800'], Array(1), [null]]) {
    await assert.rejects(store.compareExchange(['a', 'b'], [{ generation: '0' }, { generation: '0' }], [['a'], records]), TypeError);
    assert.equal(catalog.reads + catalog.writes + data.reads + data.writes, 0);
  }
  const options = { maxRecords: 1, maxBytes: 4, maxChunks: 2 };
  const selected = new PartitionedJournalStore(catalog, { east: data }, (key) => key === 'a' ? 'east' : 'missing', { chunks: options });
  options.maxBytes = 100; // construction owns the policy
  await assert.rejects(selected.compareExchange(['a', 'b'], [{ generation: '0' }, { generation: '0' }], [['a'], ['b']]), /route/);
  assert.equal(data.reads + data.writes, 0);
  const defaults = fixture({});
  await defaults.store.compareExchange(['a'], [{ generation: '0' }], [['one']]);
  assert.equal(defaults.catalog.values.get('a').entries[0], 'chunks1');
});

test('every selected chunk manifest is validated before the first data fetch', async () => {
  const { catalog, data, store } = fixture();
  await store.compareExchange(['a'], [{ generation: '0' }], [['a']]);
  const valid = catalog.values.get('a'); const key = valid.entries[4]; const gen = valid.entries[5];
  const invalid = [
    ['chunks1'], ['chunks1', 'missing', '0', '0'], ['chunks1', 'east', '00', '0'],
    ['chunks1', 'east', '4294967296', '0'], ['chunks1', 'east', '0', '4097'],
    ['chunks1', 'east', '0', '01'], ['chunks1', 'east', '0', '1'],
    ['chunks1', 'east', '1', '0'], ['chunks1', 'east', '1', '1', '../bad', gen, '1'],
    ['chunks1', 'east', '1', '1', key, '0', '1'], ['chunks1', 'east', '1', '1', key, '01', '1'],
    ['chunks1', 'east', '1', '1', key, '18446744073709551616', '1'],
    ['chunks1', 'east', '1', '1', key, gen, '0'], ['chunks1', 'east', '1', '1', key, gen, '65537'],
    ['chunks1', 'east', '1', '1', key, gen, '-1'], ['chunks1', 'east', '2', '1', key, gen, '1'],
  ];
  for (const entries of invalid) {
    catalog.values.set('b', { generation: '1', entries }); const count = data.reads;
    await assert.rejects(store.read(['a', 'b']), /chunk/); assert.equal(data.reads, count);
  }
  catalog.values.set('b', { generation: '0', entries: valid.entries });
  await assert.rejects(store.read(['b']), /pointer/);
});

test('chunk count, exact generation, content and Unicode checks reject modified objects', async () => {
  for (const kind of ['missing', 'generation', 'contents', 'count', 'unicode', 'bytes', 'absent-records']) {
    const { catalog, data, store } = fixture(null);
    const records = kind === 'unicode' ? ['\ud800'] : kind === 'bytes' ? ['x'.repeat(8388609)] : ['one'];
    await store.compareExchange(['a'], [{ generation: '0' }], [records]);
    const pointer = catalog.values.get('a'); const [tag, partition, key, gen] = pointer.entries;
    assert.equal(tag, 'blob1');
    catalog.values.set('a', { generation: pointer.generation, entries: ['chunks1', partition, kind === 'count' ? '2' : '1', '1', key, gen, kind === 'count' ? '2' : '1'] });
    if (kind === 'missing') data.values.delete(key);
    if (kind === 'generation') data.values.get(key).generation = '99';
    if (kind === 'contents') data.values.get(key).entries = ['other'];
    if (kind === 'absent-records') data.values.get(key).generation = '0';
    await assert.rejects(store.read(['a']), /object|chunk/);
  }
});

test('finite before/after staging and catalog cuts publish only complete groups', async () => {
  for (let cut = 0; cut < 5; cut++) for (const after of [false, true]) {
    const { catalog, data, store } = fixture();
    await store.compareExchange(['a', 'b'], await store.read(['a', 'b']), [['old'], ['old-b']]);
    const base = await store.read(['a', 'b']); let mutation = 0;
    const fault = () => { if (mutation++ === cut) throw Error('finite publication cut'); };
    const hook = after ? 'after' : 'before'; catalog[hook] = fault; data[hook] = fault;
    const updates = [['a', 'b', 'c'], ['d', 'e', 'f']];
    await assert.rejects(store.compareExchange(['a', 'b'], base, updates), /finite publication cut/);
    catalog[hook] = undefined; data[hook] = undefined;
    const fresh = await store.read(['a', 'b']);
    assert.deepEqual(fresh.map((v) => v.entries), after && cut === 4 ? updates : base.map((v) => v.entries));
    assert.equal(await store.compareExchange(['a', 'b'], fresh, [['next'], null]), true);
    assert.equal(await store.compareExchange(['a', 'b'], base, [['stale'], null]), false);
  }
});

test('cached objects still validate every manifest record count', async () => {
  const { catalog, data, store } = fixture(undefined, 256);
  await store.compareExchange(['a'], [{ generation: '0' }], [['one']]);
  await store.read(['a']); const reads = data.reads;
  const pointer = catalog.values.get('a'); pointer.entries[2] = pointer.entries[6] = '2';
  await assert.rejects(store.read(['a']), /chunk/); assert.equal(data.reads, reads);
});

test('sparse keys, dependencies, updates and records fail before staging', async () => {
  for (const kind of ['keys', 'expected', 'updates', 'records']) {
    const { catalog, data, store } = fixture(null);
    const keys = kind === 'keys' ? Array(1) : ['a'];
    const expected = kind === 'expected' ? Array(1) : [{ generation: '0' }];
    const updates = kind === 'updates' ? Array(1) : [kind === 'records' ? Array(1) : ['one']];
    await assert.rejects(store.compareExchange(keys, expected, updates), TypeError);
    assert.equal(catalog.reads + catalog.writes + data.reads + data.writes, 0, kind);
  }
  const { catalog, data, store } = fixture();
  await store.compareExchange(['a'], [{ generation: '0' }], [['one']]);
  const valid = catalog.values.get('a');
  catalog.read = async () => { const result = Array(2); result[0] = valid; return result; };
  const reads = data.reads;
  await assert.rejects(store.read(['a', 'b']), /generation/);
  assert.equal(data.reads, reads);
});
