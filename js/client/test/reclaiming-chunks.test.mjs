import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { ReclaimingPartitionedJournalStore as Reclaiming } from '../src/index.js';
import { MemoryMaintenanceStore } from '../../../experiments/issue-138/reclamation-memory.mjs';
const epochKey = '__link_gc_epoch';
const selected = { maxRecords: 2, maxBytes: 64, maxChunks: 32 };
function providers(epoch = '0') {
  const catalog = new MemoryMaintenanceStore('catalog');
  catalog.values.set(epochKey, { generation: '1', entries: ['gc1', epoch] }); catalog.revision = 1n;
  return { catalog, east: new MemoryMaintenanceStore('east'), west: new MemoryMaintenanceStore('west') };
}
const open = (p, options = {}) => Reclaiming.open(p.catalog, { east: p.east, west: p.west }, (key) => key === 'a' ? 'east' : 'west', { chunks: selected, ...options });
const count = (p, operation) => p.calls.filter((call) => call.operation === operation).length;
const values = (items) => items.map((item) => item.entries);

test('reclaiming chunk manifests match independent UTF-8 and full-width epoch vectors', { timeout: 10000 }, async () => {
  const vectors = JSON.parse(readFileSync(new URL('../../../docs/protocol/reclaiming-chunks-vectors.json', import.meta.url), 'utf8'));
  assert.equal(vectors.length, 18);
  for (const v of vectors) {
    const p = providers(v.epoch); const store = await open(p, { chunks: { maxRecords: v.maxRecords, maxBytes: v.maxBytes } });
    assert.equal(await store.compareExchange(['a'], await store.read(['a']), [v.entries]), true);
    assert.deepEqual(p.catalog.values.get('a').entries, ['chunks2', 'east', String(v.entries.length), String(v.chunks.length),
      ...v.chunks.flatMap((chunk, index) => [chunk.address, String(index + 1), String(chunk.entries.length)])]);
    assert.deepEqual([...p.east.values.keys()], v.chunks.map((chunk) => chunk.address));
    assert.deepEqual(values(await store.read(['a'])), [v.entries]);
    assert.deepEqual(values(await (await open(p, { chunks: null })).read(['a'])), [v.entries]);
  }
});

test('chunks reuse within an epoch, own cached data and restage complete prefixes after a fence', { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p, { cacheBytes: 256 }); const seed = ['aa', 'bb', 'cc', 'dd'];
  assert.ok(await store.compareExchange(['a'], await store.read(['a']), [seed]));
  const original = structuredClone(p.catalog.values.get('a')); const owned = await store.read(['a']);
  const reads = count(p.east, 'read'); owned[0].entries[0] = 'caller';
  assert.deepEqual(values(await store.read(['a'])), [seed]); assert.equal(count(p.east, 'read'), reads);
  assert.ok(await store.compareExchange(['a'], owned, [[...seed, 'ee']]));
  assert.deepEqual(p.catalog.values.get('a').entries.slice(4, 10), original.entries.slice(4));
  assert.equal(count(p.east, 'swap'), 3); const prior = await store.read(['a']);
  assert.equal((await store.collectGarbage()).epoch, '1');
  const fresh = [...seed, 'ee', 'ff']; assert.ok(await store.compareExchange(['a'], prior, [fresh]));
  const manifest = p.catalog.values.get('a').entries;
  for (let i = 4; i < manifest.length; i += 3) assert.ok(manifest[i].startsWith('blob2-0000000000000001-'));
  assert.deepEqual(values(await store.read(['a'])), [fresh]); assert.deepEqual(values(prior), [[...seed, 'ee']]);
  assert.equal(await store.compareExchange(['a'], prior, [['stale']]), false);
  // Staging must read actual bytes even if this object is cached and verified.
  const current = await store.read(['a']); const key = manifest[4]; const actual = p.east.values.get(key).entries[0]; p.east.values.get(key).entries[0] = 'damaged';
  await assert.rejects(store.compareExchange(['a'], current, [fresh]), /collision/);
  p.east.values.get(key).entries[0] = actual;
  assert.equal((await store.collectGarbage()).deleted, 3);
});

test('chunk selection and every update are admitted before initialization or staging I/O', { timeout: 10000 }, async () => {
  for (const chunks of [false, [], 3, 'bad', { maxRecords: 0 }, { maxBytes: 8388609 }, { maxChunks: 4097 }]) {
    const p = providers(); await assert.rejects(open(p, { chunks }), TypeError);
    assert.equal(p.catalog.calls.length + p.east.calls.length + p.west.calls.length, 0);
  }
  const p = providers(); const store = await open(p, { chunks: { maxRecords: 1, maxBytes: 4, maxChunks: 2 } });
  const calls = () => p.catalog.calls.length + p.east.calls.length + p.west.calls.length;
  for (const bad of [['12345'], ['a', 'b', 'c'], ['\ud800'], Array(1)]) {
    const n = calls(); await assert.rejects(store.compareExchange(['a', 'b'], [{ generation: '0' }, { generation: '0' }], [['a'], bad]));
    assert.equal(calls(), n);
  }
});

test('all selected manifests reject malformed or future references before any chunk read or delete', { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p);
  assert.ok(await store.compareExchange(['a'], await store.read(['a']), [['a']]));
  const [key, gen] = p.catalog.values.get('a').entries.slice(4, 6);
  const invalid = [
    ['chunks2'], ['chunks2', 'missing', '0', '0'], ['chunks2', 'east', '00', '0'],
    ['chunks2', 'east', '4294967296', '0'], ['chunks2', 'east', '0', '4097'],
    ['chunks2', 'east', '0', '01'], ['chunks2', 'east', '0', '1'], ['chunks2', 'east', '1', '0'],
    ['chunks2', 'east', '1', '1', '../bad', gen, '1'], ['chunks2', 'east', '1', '1', key, '0', '1'],
    ['chunks2', 'east', '1', '1', key, '01', '1'], ['chunks2', 'east', '1', '1', key, '18446744073709551616', '1'],
    ['chunks2', 'east', '1', '1', key, gen, '0'], ['chunks2', 'east', '1', '1', key, gen, '65537'],
    ['chunks2', 'east', '1', '1', key, gen, '-1'], ['chunks2', 'east', '2', '1', key, gen, '1'],
    ['chunks2', 'east', '1', '1', key.replace('0000000000000000', 'ffffffffffffffff'), gen, '1'],
  ];
  for (const entries of invalid) {
    p.catalog.values.set('b', { generation: '1', entries }); const reads = count(p.east, 'read');
    await assert.rejects(store.read(['a', 'b'])); assert.equal(count(p.east, 'read'), reads);
    await assert.rejects(store.collectGarbage()); assert.equal(count(p.east, 'delete') + count(p.west, 'delete'), 0);
    // Reset only the test epoch; future-reference rejection stays meaningful.
    p.catalog.values.get(epochKey).entries[1] = '0';
  }
});

test('duplicate chunk references preserve ordered records and all exact protected generations', { timeout: 10000 }, async () => {
  const p = providers(); p.west = p.east; const store = await open(p, { chunks: { maxRecords: 1 }, cacheBytes: 256 });
  assert.ok(await store.compareExchange(['a'], await store.read(['a']), [['same', 'same']]));
  assert.equal(p.east.values.size, 1); assert.equal((await store.collectGarbage()).deleted, 0);
  assert.deepEqual(values(await store.read(['a'])), [['same', 'same']]);
  const pointer = p.catalog.values.get('a'); pointer.entries[8] = '99';
  await assert.rejects(store.collectGarbage(), /contradictory/); assert.equal(count(p.east, 'delete'), 0);
});

test('cached and uncached chunks validate declared counts and actual generations before returning a group', { timeout: 10000 }, async () => {
  for (const kind of ['count', 'cached-count', 'generation', 'contents', 'missing']) {
    const p = providers(); const store = await open(p, { cacheBytes: kind === 'cached-count' ? 256 : 0 });
    assert.ok(await store.compareExchange(['a'], await store.read(['a']), [['one']]));
    await store.read(['a']); const pointer = p.catalog.values.get('a'); const key = pointer.entries[4];
    if (kind.includes('count')) pointer.entries[2] = pointer.entries[6] = '2';
    if (kind === 'generation') p.east.values.get(key).generation = '99';
    if (kind === 'contents') p.east.values.get(key).entries = ['other'];
    if (kind === 'missing') p.east.values.delete(key);
    const reads = count(p.east, 'read'); await assert.rejects(store.read(['a']), /chunk|object/);
    if (kind === 'cached-count') assert.equal(count(p.east, 'read'), reads);
  }
});

test('retired suffix discards previously fetched or cached prefixes and every other shard', { timeout: 10000 }, async () => {
  for (const cacheBytes of [0, 256]) {
    const p = providers(); const writer = await open(p); const reader = await open(p, { cacheBytes });
    const old = [['a0', 'a0-tail', 'a0-last'], ['b0', 'b0-tail', 'b0-last']];
    assert.ok(await writer.compareExchange(['a', 'b'], await writer.read(['a', 'b']), old));
    // Retain the first shard in cache; the later missing suffix must discard it.
    if (cacheBytes) await reader.read(['a']);
    const base = await writer.read(['a', 'b']); const fresh = [['a1', 'a1-tail', 'a1-last'], ['b1', 'b1-tail', 'b1-last']];
    p.west.on('read', () => {});
    p.west.on('read', async () => {
      assert.ok(await writer.compareExchange(['a', 'b'], base, fresh));
      assert.equal((await writer.collectGarbage()).deleted, 4);
    });
    assert.deepEqual(values(await reader.read(['a', 'b'])), fresh);
  }
});

test('a collector fence after the last chunk staging prevents catalog publication without retry', { timeout: 10000 }, async () => {
  const p = providers(); const writer = await open(p); const collector = await open(p);
  const before = await writer.read(['a']); const swaps = count(p.catalog, 'swap');
  p.catalog.on('swap', async () => { assert.equal((await collector.collectGarbage()).epoch, '1'); });
  assert.equal(await writer.compareExchange(['a'], before, [['a', 'b', 'c']]), false);
  assert.equal(count(p.catalog, 'swap') - swaps, 2); assert.deepEqual(values(await writer.read(['a'])), [[]]);
  assert.ok(await writer.compareExchange(['a'], before, [['following']]));
});

test('all finite before/after chunk staging and publication cuts retain complete groups', { timeout: 10000 }, async () => {
  for (let cut = 0; cut < 5; cut++) for (const after of [false, true]) {
    const p = providers(); const store = await open(p); const keys = ['a', 'b'];
    assert.ok(await store.compareExchange(keys, await store.read(keys), [['old-a'], ['old-b']]));
    const base = await store.read(keys); let mutation = 0;
    const install = (provider) => provider.on('swap', () => {
      if (mutation++ === cut) throw Error('finite chunk cut'); install(provider);
    }, { after });
    for (const provider of [p.catalog, p.east, p.west]) install(provider);
    const updates = [['a', 'b', 'c'], ['d', 'e', 'f']];
    await assert.rejects(store.compareExchange(keys, base, updates), (error) => error.outcome === 'unknown' && /finite chunk cut/.test(String(error)));
    for (const provider of [p.catalog, p.east, p.west]) { provider.before.clear(); provider.after.clear(); }
    const fresh = await store.read(keys); assert.deepEqual(values(fresh), after && cut === 4 ? updates : values(base));
    assert.ok(await store.compareExchange(keys, fresh, [['following'], null]));
    assert.equal(await store.compareExchange(keys, base, [['stale'], null]), false);
  }
});
