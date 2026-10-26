import assert from 'node:assert/strict';
import test from 'node:test';
import { LinksError, ReclaimingPartitionedJournalStore as Reclaiming } from '../src/index.js';
import { MemoryMaintenanceStore } from '../../../experiments/issue-138/reclamation-memory.mjs';
const epochKey = '__link_gc_epoch';
function providers() { return { catalog: new MemoryMaintenanceStore('catalog'), east: new MemoryMaintenanceStore('east'), west: new MemoryMaintenanceStore('west') }; }
const open = (p, options) => Reclaiming.open(p.catalog, { east: p.east, west: p.west }, (key) => key === 'a' ? 'east' : 'west', options);
async function publish(store, text) { const before = await store.read(['a']); assert.ok(await store.compareExchange(['a'], before, [[text]])); return store.read(['a']); }
const deletions = (p) => p.east.calls.filter((call) => call.operation === 'delete').length + p.west.calls.filter((call) => call.operation === 'delete').length;
for (const outcome of ['not-sent', 'failed']) for (const phase of ['initialization', 'staging', 'fence', 'deletion']) test(`composite ${phase} preserves earlier effects when a later request is ${outcome}`, { timeout: 10000 }, async () => {
  const p = providers(); const cause = new LinksError('later request failed', { outcome });
  const stop = () => { throw cause; }; let work;
  if (phase === 'initialization') {
    p.catalog.on('swap', () => p.catalog.on('scan', stop), { after: true }); work = open(p);
  } else {
    const store = await open(p);
    if (phase === 'staging') {
      p.east.on('swap', () => p.east.on('read', stop), { after: true });
      work = store.compareExchange(['a'], await store.read(['a']), [['staged']]);
    } else {
      if (phase === 'fence') p.catalog.on('swap', () => p.catalog.on('scan', stop), { after: true });
      else {
        await publish(store, 'old'); await publish(store, 'middle'); await publish(store, 'current');
        p.east.on('delete', () => p.east.on('delete', stop), { after: true });
      }
      work = store.collectGarbage();
    }
  }
  await assert.rejects(work, (error) => error.outcome === 'unknown' && error.cause === cause);
  assert.equal(p.catalog.values.get(epochKey).entries[1], phase === 'fence' || phase === 'deletion' ? '1' : '0');
  if (phase === 'staging') { assert.equal(p.east.values.size, 1); assert.equal(p.catalog.values.has('a'), false); }
  if (phase === 'deletion') { assert.equal(p.east.values.size, 2); assert.equal(deletions(p), 2); }
});
for (const field of ['partition name', 'shard name', 'caller generation', 'epoch', 'object key', 'pointer generation']) test(`canonical ${field} rejects every trailing line terminator`, { timeout: 10000 }, async () => {
  for (const ending of ['\n', '\r', '\r\n', '\u2028', '\u2029']) {
    const p = providers();
    if (field === 'partition name') {
      await assert.rejects(Reclaiming.open(p.catalog, { ['east' + ending]: p.east }, () => 'east'), TypeError);
      assert.equal(p.catalog.calls.length + p.east.calls.length, 0); continue;
    }
    const store = await open(p);
    if (field === 'shard name' || field === 'caller generation') {
      const n = p.catalog.calls.length;
      if (field === 'shard name') {
        await assert.rejects(store.read(['a' + ending]), TypeError);
        await assert.rejects(store.compareExchange(['a' + ending], [{ generation: '0' }], [['new']]), TypeError);
      } else await assert.rejects(store.compareExchange(['a'], [{ generation: '0' + ending }], [['new']]), TypeError);
      assert.equal(p.catalog.calls.length, n); assert.equal(p.east.calls.length, 1); continue;
    }
    if (field === 'epoch') p.catalog.values.get(epochKey).entries[1] += ending;
    else {
      await publish(store, 'live');
      p.catalog.values.get('a').entries[field === 'object key' ? 2 : 3] += ending;
    }
    const n = p.east.calls.length;
    await assert.rejects(store.read(['a']), /invalid/);
    assert.equal(p.east.calls.length, n); assert.equal(deletions(p), 0);
  }
});
test('configuration and every update validate before I/O', { timeout: 10000 }, async () => {
  const p = providers();
  for (const options of [{ maxKeys: 0 }, { maxBytes: 0 }, { pageKeys: 0 }, { pageKeys: 257 }, { cacheBytes: -1 }, { cacheBytes: 0.5 }, { maxBytes: Infinity }]) await assert.rejects(open(p, options), TypeError);
  await assert.rejects(Reclaiming.open(null, { east: p.east }, () => 'east'), TypeError);
  await assert.rejects(Reclaiming.open(p.catalog, {}, () => 'east'), TypeError);
  await assert.rejects(Reclaiming.open(p.catalog, { east: {} }, () => 'east'), TypeError);
  await assert.rejects(Reclaiming.open(p.catalog, { '../bad': p.east }, () => 'east'), TypeError);
  assert.throws(() => new Reclaiming(), TypeError); assert.deepEqual(p.catalog.calls, []);
  const store = await open(p); const expected = await store.read(['a', 'b']);
  const count = () => p.catalog.calls.length + p.east.calls.length + p.west.calls.length;
  for (const [keys, previous, updates] of [
    [[], [], []], [['a', 'a'], expected, [null, null]], [[epochKey], [expected[0]], [null]], [Array(1), [expected[0]], [null]],
    [['a'], [], [null]], [['a'], [{ generation: '01' }], [null]], [['a'], [expected[0]], [['\ud800']]],
    [['a', 'b'], expected, [['valid'], [null]]], [['a'], Array(1), [null]],
  ]) { const n = count(); await assert.rejects(store.compareExchange(keys, previous, updates), TypeError); assert.equal(count(), n); }
  const badRoute = await Reclaiming.open(p.catalog, { east: p.east }, () => 'absent'); const n = count();
  await assert.rejects(badRoute.compareExchange(['a'], [expected[0]], [['data']]), TypeError); assert.equal(count(), n);
});
for (const after of [false, true]) test(`unknown initialization after=${after} is never replayed`, { timeout: 10000 }, async () => {
  const p = providers(); p.catalog.on('swap', () => { throw Error('initialization response lost'); }, { after });
  await assert.rejects(open(p), /initialization response lost/);
  assert.equal(p.catalog.calls.filter((call) => call.operation === 'swap').length, 1);
  assert.equal(p.catalog.values.has(epochKey), after); assert.equal(deletions(p), 0);
  assert.ok(await open(p));
});
test('fresh ownership requires empty data and a recognized catalog', { timeout: 10000 }, async () => {
  const p = providers(); p.east.values.set('foreign', { generation: '1', entries: [] });
  await assert.rejects(open(p), /must be empty/); assert.equal(p.catalog.values.size, 0);
  p.east.values.clear(); p.catalog.values.set('a', { generation: '1', entries: ['blob1', 'east', 'legacy', '1'] });
  await assert.rejects(open(p), /missing reclamation epoch/); assert.equal(p.catalog.calls.filter((call) => call.operation === 'swap').length, 0);
});
test('a known initialization competitor is discovered by a fresh complete scan', { timeout: 10000 }, async () => {
  const p = providers();
  p.catalog.on('swap', () => { p.catalog.values.set(epochKey, { generation: '1', entries: ['gc1', '7'] }); p.catalog.revision = 1n; });
  const store = await open(p); assert.deepEqual(await store.collectGarbage(), { epoch: '8', examined: 0, deleted: 0 });
});
test('31 read-only shards use the final slot for the epoch and stale guards stage nothing', { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p); const keys = Array.from({ length: 31 }, (_, i) => `s${i}`);
  const expected = await store.read(keys); assert.ok(await store.compareExchange(keys, expected, keys.map(() => null)));
  const call = p.catalog.calls.at(-1); assert.equal(call.keys.length, 32); assert.equal(call.keys.at(-1), epochKey);
  assert.equal(p.east.values.size, 0); const stale = await store.read(['a']); await publish(store, 'new');
  const n = p.east.calls.length; assert.equal(await store.compareExchange(['a'], stale, [['orphan']]), false); assert.equal(p.east.calls.length, n);
});
for (const operation of ['stage', 'publish', 'fence']) for (const after of [false, true]) test(`unknown ${operation} after=${after} is never replayed`, { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p); await publish(store, 'old');
  const provider = operation === 'stage' ? p.east : p.catalog; const before = provider.calls.filter((call) => call.operation === 'swap').length;
  provider.on('swap', () => { throw Error(`${operation} response lost`); }, { after });
  if (operation === 'fence') await assert.rejects(store.collectGarbage(), /response lost/);
  else await assert.rejects(store.compareExchange(['a'], await store.read(['a']), [['new']]), /response lost/);
  assert.equal(provider.calls.filter((call) => call.operation === 'swap').length, before + 1); assert.equal(deletions(p), 0);
  assert.deepEqual((await store.read(['a']))[0].entries, [operation === 'publish' && after ? 'new' : 'old']);
  await publish(store, 'following'); assert.deepEqual((await store.read(['a']))[0].entries, ['following']);
});
test('known competing fence returns null and final epoch exhaustion sends no mutation', { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p); const other = await open(p);
  p.catalog.on('swap', async () => { assert.equal((await other.collectGarbage()).epoch, '1'); });
  assert.equal(await store.collectGarbage(), null); assert.equal(deletions(p), 0);
  p.catalog.values.get(epochKey).entries[1] = '18446744073709551615';
  const count = p.catalog.calls.filter((call) => call.operation === 'swap').length;
  await assert.rejects(store.collectGarbage(), /epoch exhausted/); assert.equal(p.catalog.calls.filter((call) => call.operation === 'swap').length, count);
});
test('generation-conditional deletion skips a replacement without replay', { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p); await publish(store, 'old'); await publish(store, 'new');
  const [retired] = p.east.values.keys();
  p.east.on('delete', () => { p.east.revision++; p.east.values.set(retired, { generation: String(p.east.revision), entries: ['ABA replacement'] }); });
  const result = await store.collectGarbage(); assert.equal(result.deleted, 0); assert.deepEqual(p.east.values.get(retired).entries, ['ABA replacement']);
  assert.deepEqual((await store.read(['a']))[0].entries, ['new']);
});
for (const damage of ['absent', 'generation', 'content', 'transport', 'short']) test(`reader fails closed on ${damage} without a mutation`, { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p); await publish(store, 'live'); const [key, value] = [...p.east.values][0];
  if (damage === 'absent') p.east.values.delete(key);
  if (damage === 'generation') value.generation = '999';
  if (damage === 'content') value.entries = ['changed'];
  if (damage === 'transport') p.east.on('read', () => { throw Error('unavailable'); });
  if (damage === 'short') p.east.read = async () => [];
  const count = p.catalog.calls.filter((call) => call.operation === 'read').length;
  await assert.rejects(store.read(['a']));
  assert.equal(p.catalog.calls.filter((call) => call.operation === 'read').length - count, damage === 'absent' ? 2 : 1); assert.equal(deletions(p), 0);
});
test('cache serves verified current objects during data outage after collection', { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p, { cacheBytes: 64 }); await publish(store, 'old'); const owned = await store.read(['a']);
  await publish(store, 'new'); assert.equal((await store.collectGarbage()).deleted, 1);
  p.east.read = async () => { throw Error('data outage'); };
  assert.deepEqual((await store.read(['a']))[0].entries, ['new']); assert.deepEqual(owned[0].entries, ['old']);
  p.catalog.on('read', () => { throw Error('catalog outage'); }); await assert.rejects(store.read(['a']), /catalog outage/);
});
test('aliased partition providers cannot reclaim another partition live root', { timeout: 10000 }, async () => {
  const p = providers(); const store = await Reclaiming.open(p.catalog, { east: p.east, west: p.east }, () => 'east');
  await publish(store, 'live'); const result = await store.collectGarbage();
  assert.equal(result.deleted, 0); assert.deepEqual((await store.read(['a']))[0].entries, ['live']);
});
for (const phase of ['catalog', 'east', 'west']) for (const damage of ['empty', 'zero', 'unordered', 'duplicate', 'name', 'format', 'entries', 'generation']) test(`complete ${phase} snapshot rejects ${damage} before deletion`, { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p); await publish(store, 'old'); await publish(store, 'new');
  // The second partition is validated even when there are no roots routed there.
  const provider = p[phase]; const scan = provider.scanNamespace.bind(provider);
  provider.scanNamespace = async (options) => {
    const items = await scan(options);
    if (damage === 'empty') return phase === 'catalog' ? [] : null;
    if (damage === 'zero') return [{ key: 'a', version: { generation: '0', entries: [] } }];
    if (damage === 'unordered') return [{ key: 'z', version: { generation: '1', entries: [] } }, { key: 'a', version: { generation: '2', entries: [] } }];
    if (damage === 'duplicate') return [items[0] ?? { key: 'a', version: { generation: '1', entries: [] } }, items[0] ?? { key: 'a', version: { generation: '1', entries: [] } }];
    if (damage === 'name') return [{ key: '../bad', version: { generation: '1', entries: [] } }];
    if (damage === 'format') return [{ key: 'unrecognized', version: { generation: '1', entries: [] } }];
    if (damage === 'entries') return [{ key: phase === 'catalog' ? epochKey : 'a', version: { generation: '1', entries: ['extra'] } }];
    if (damage === 'generation') return [{ key: phase === 'catalog' ? epochKey : 'a', version: { generation: '01', entries: [] } }];
    return items;
  };
  await assert.rejects(store.collectGarbage(), (error) => error.outcome === 'unknown');
  // The acknowledged fence already changed the backend before validation failed.
  assert.equal(p.catalog.values.get(epochKey).entries[1], '1'); assert.equal(deletions(p), 0);
});
for (const phase of ['stage', 'publish', 'fence', 'delete']) test(`indeterminate ${phase} acknowledgement never causes replay`, { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p); await publish(store, 'old'); await publish(store, 'new');
  const provider = phase === 'stage' || phase === 'delete' ? p.east : p.catalog; const method = phase === 'delete' ? 'compareDelete' : 'compareExchange'; const original = provider[method].bind(provider); let sends = 0;
  provider[method] = async (...args) => { sends++; await original(...args); return undefined; };
  const work = phase === 'delete' || phase === 'fence' ? store.collectGarbage() : store.compareExchange(['a'], await store.read(['a']), [['following']]);
  await assert.rejects(work, (error) => error.outcome === 'unknown'); assert.equal(sends, 1);
});
test('collector rejects future catalog roots and snapshots older than its fence', { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p); await publish(store, 'live'); const original = p.catalog.scanNamespace.bind(p.catalog);
  p.catalog.scanNamespace = async (options) => (await original(options)).map((item) => item.key === epochKey ? { key: epochKey, version: { generation: '1', entries: ['gc1', '0'] } } : item);
  await assert.rejects(store.collectGarbage(), /predates/); assert.equal(deletions(p), 0);
  p.catalog.scanNamespace = original; const pointer = p.catalog.values.get('a').entries; pointer[2] = 'blob2-ffffffffffffffff-' + pointer[2].slice(23);
  await assert.rejects(store.collectGarbage(), /exceeds catalog epoch/); assert.equal(deletions(p), 0);
});
for (const limit of [{ maxKeys: 1 }, { maxBytes: 32 }]) test(`provider cannot bypass collector ${JSON.stringify(limit)} budget`, { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p, limit); await publish(store, 'old'); await publish(store, 'new');
  const original = p.catalog.scanNamespace.bind(p.catalog); p.catalog.scanNamespace = async (options) => original({ ...options, maxKeys: 4096, maxBytes: 8388608 });
  await assert.rejects(store.collectGarbage(), /snapshot|limit/); assert.equal(deletions(p), 0);
});
test('unchanged staging always compares contents and collision sends no publication', { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p); await publish(store, 'live'); const expected = await store.read(['a']); const before = p.east.calls.filter((call) => call.operation === 'swap').length;
  assert.ok(await store.compareExchange(['a'], expected, [['live']])); assert.equal(p.east.calls.filter((call) => call.operation === 'swap').length, before);
  p.east.values.values().next().value.entries = ['collision']; const count = p.catalog.calls.filter((call) => call.operation === 'swap').length;
  await assert.rejects(store.compareExchange(['a'], [{ generation: p.catalog.values.get('a').generation }], [['live']]), /collision/);
  assert.equal(p.catalog.calls.filter((call) => call.operation === 'swap').length, count);
});
for (const cacheBytes of [1, 2, 3, 5, 8]) test(`cache has a finite UTF-8 payload budget ${cacheBytes}`, { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p, { cacheBytes });
  // Empty payloads cost one byte, each record adds UTF-8 bytes plus one.
  const expected = await store.read(['a', 'b']); assert.ok(await store.compareExchange(['a', 'b'], expected, [[], ['雪']]));
  await store.read(['a', 'b']); const reads = p.east.calls.filter((call) => call.operation === 'read').length;
  await store.read(['a']); assert.equal(p.east.calls.filter((call) => call.operation === 'read').length, reads + (cacheBytes === 5 ? 1 : 0));
  const current = await store.read(['a']); assert.ok(await store.compareExchange(['a'], current, [['x']])); await store.read(['a']);
  await store.read(['b']); await store.read(['a']);
});
test('durability is advertised only when every selected provider supports it', { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p); assert.equal(store.supportsDurableFlush, false);
  p.catalog.supportsDurableFlush = p.east.supportsDurableFlush = p.west.supportsDurableFlush = true; assert.equal(store.supportsDurableFlush, true);
  p.west.supportsDurableFlush = false; assert.equal(store.supportsDurableFlush, false);
});
test('a changed pointer cannot reuse its catalog generation during reader refresh', { timeout: 10000 }, async () => {
  const p = providers(); const writer = await open(p); const reader = await open(p); await publish(writer, 'old'); const generation = p.catalog.values.get('a').generation;
  p.east.on('read', async () => { await publish(writer, 'new'); p.catalog.values.get('a').generation = generation; await writer.collectGarbage(); });
  await assert.rejects(reader.read(['a']), /missing|modified/);
});
for (const damage of ['absent', 'generation']) test(`collector rejects ${damage} protected data before any deletion`, { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p); await publish(store, 'old'); await publish(store, 'new'); const key = p.catalog.values.get('a').entries[2];
  if (damage === 'absent') p.east.values.delete(key); else p.east.values.get(key).generation = '999';
  await assert.rejects(store.collectGarbage(), /protected object/); assert.equal(deletions(p), 0);
});
test('malformed absent data cannot contain records', { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p); await publish(store, 'live');
  p.east.read = async () => [{ generation: '0', entries: ['invented'] }];
  await assert.rejects(store.read(['a']), /absent journal/);
});
test('concurrent cache misses keep one owned entry and ordinal routing order', { timeout: 10000 }, async () => {
  const p = providers(); const writer = await open(p); await publish(writer, 'live');
  const reader = await Reclaiming.open(p.catalog, { west: p.west, east: p.east }, () => 'east', { cacheBytes: 16 });
  const [a, b] = await Promise.all([reader.read(['a']), reader.read(['a'])]); a[0].entries[0] = 'caller mutation'; assert.deepEqual(b[0].entries, ['live']); assert.deepEqual((await reader.read(['a']))[0].entries, ['live']);
});
test('a second retired object stops after one whole-group retry', { timeout: 10000 }, async () => {
  const p = providers(); const writer = await open(p); const reader = await open(p); await publish(writer, 'zero');
  p.east.on('read', async () => {
    await publish(writer, 'one'); await writer.collectGarbage();
    p.east.on('read', async () => { await publish(writer, 'two'); await writer.collectGarbage(); });
  });
  await assert.rejects(reader.read(['a']), /bounded retry/);
  assert.deepEqual((await reader.read(['a']))[0].entries, ['two']);
});
test('a missing object with unchanged pointer rejects even if its catalog generation changes', { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p); await publish(store, 'old');
  p.east.on('read', () => { p.east.values.clear(); p.catalog.values.get('a').generation = '999'; });
  await assert.rejects(store.read(['a']), /missing/);
});
test('whole-group refresh may return a freshly absent shard', { timeout: 10000 }, async () => {
  const p = providers(); const store = await open(p); await publish(store, 'old');
  p.east.on('read', () => { p.east.values.clear(); p.catalog.values.delete('a'); });
  assert.deepEqual(await store.read(['a']), [{ generation: '0', entries: [] }]);
});
for (const phase of ['catalog-scan', 'east-scan', 'west-scan', 'delete']) test(`concurrent publication at ${phase} remains reachable`, { timeout: 10000 }, async () => {
  const p = providers(); const writer = await open(p); const collector = await open(p); await publish(writer, 'old'); await publish(writer, 'current');
  const [provider, operation] = phase === 'catalog-scan' ? [p.catalog, 'scan'] : phase === 'east-scan' ? [p.east, 'scan'] : phase === 'west-scan' ? [p.west, 'scan'] : [p.east, 'delete'];
  provider.on(operation, async () => { await publish(writer, 'concurrent'); });
  assert.equal((await collector.collectGarbage()).epoch, '1'); assert.deepEqual((await writer.read(['a']))[0].entries, ['concurrent']);
  await writer.collectGarbage(); assert.deepEqual((await writer.read(['a']))[0].entries, ['concurrent']);
});
test('a publication fenced immediately before catalog CAS returns false and fresh retry succeeds', { timeout: 10000 }, async () => {
  const p = providers(); const writer = await open(p); const collector = await open(p); await publish(writer, 'old');
  const expected = await writer.read(['a']); p.catalog.on('swap', async () => { await collector.collectGarbage(); });
  assert.equal(await writer.compareExchange(['a'], expected, [['in-flight']]), false);
  assert.deepEqual((await writer.read(['a']))[0].entries, ['old']); await publish(writer, 'following');
});
test('collector uses its own fence when a later incomplete collector advances the epoch', { timeout: 10000 }, async () => {
  const p = providers(); const writer = await open(p); const collector = await open(p); const other = await open(p); await publish(writer, 'live');
  p.catalog.on('scan', async () => {
    p.catalog.on('swap', () => { throw Error('lost publish'); });
    await assert.rejects(writer.compareExchange(['a'], await writer.read(['a']), [['orphan at one']]), /lost publish/);
    p.east.on('scan', () => { throw Error('incomplete next collector'); });
    await assert.rejects(other.collectGarbage(), /incomplete next collector/);
  });
  assert.deepEqual(await collector.collectGarbage(), { epoch: '1', examined: 2, deleted: 0 });
  assert.equal([...p.east.values.keys()].some((key) => key.startsWith('blob2-0000000000000001-')), true);
  assert.equal((await collector.collectGarbage()).deleted, 1); assert.deepEqual((await writer.read(['a']))[0].entries, ['live']);
});
test('resurrecting retired contents stages a new epoch object and stale owners cannot publish', { timeout: 10000 }, async () => {
  const p = providers(); const writer = await open(p); const stale = await open(p); await publish(writer, 'old'); const old = await stale.read(['a']); const oldKey = p.catalog.values.get('a').entries[2];
  await publish(writer, 'new'); assert.equal((await writer.collectGarbage()).deleted, 1); assert.equal(p.east.values.has(oldKey), false);
  assert.equal(await stale.compareExchange(['a'], old, [['stale']]), false); await publish(writer, 'old');
  const newKey = p.catalog.values.get('a').entries[2]; assert.notEqual(newKey, oldKey); assert.ok(newKey.startsWith('blob2-0000000000000001-')); assert.deepEqual(old[0].entries, ['old']);
});
