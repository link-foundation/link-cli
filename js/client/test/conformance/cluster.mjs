import assert from 'node:assert/strict';
import { ClusterEngine } from '../../src/cluster.js';
import { PartitionedJournalStore } from '../../src/partitioned.js';

class Store {
  supportsDurableFlush = false; revision = 0; values = new Map(); loseReply = false;
  async read(keys) { return keys.map((key) => structuredClone(this.values.get(key) ?? { generation: '0', entries: [] })); }
  async compareExchange(keys, expected, updates) {
    if (keys.some((key, index) => (this.values.get(key)?.generation ?? '0') !== expected[index].generation)) return false;
    const generation = String(++this.revision);
    keys.forEach((key, index) => { if (updates[index] !== null) this.values.set(key, { generation, entries: [...updates[index]] }); });
    if (this.loseReply) throw Object.assign(Error('indeterminate coordinated commit: response lost'), { outcome: 'unknown' });
    return true;
  }
}
export async function clusterConformance(binding) {
  const store = new Store(); const first = new ClusterEngine(binding, store); const second = new ClusterEngine(binding, store);
  assert.throws(() => new ClusterEngine(binding, store, { concurrency: 'exclusive' }), /optimistic/);
  assert.throws(() => new ClusterEngine(binding, store, { concurrency: 'pessimistic' }), /optimistic/);
  assert.throws(() => new ClusterEngine(binding, store, { isolation: 'snapshot' }), /serializable/);
  assert.throws(() => new ClusterEngine(binding, store, { requireDurableFlush: true }), /durable/);
  const seed = await first.begin(['a', 'b']);
  await seed.shard('a').create(0, 0); await seed.shard('b').create(0, 0); await seed.commit();
  await assert.rejects(seed.commit(), /closed/); assert.throws(() => seed.shard('a'), /closed/);
  const stale = await first.begin(['a', 'b']); const dependency = await second.begin(['b']);
  assert.throws(() => stale.shard('other'), /not selected/);
  const retainedClient = stale.shard('a');
  await dependency.shard('b').setName(1, 'fresh'); await dependency.commit();
  await retainedClient.delete(1); await assert.rejects(stale.commit(), { outcome: 'failed' });
  await assert.rejects(retainedClient.count(), /closed/);
  const checked = await first.begin(['a', 'b']);
  assert.equal(await checked.shard('a').count(), 1n); assert.equal(await checked.shard('b').getName(1), 'fresh'); await checked.close();
  const a = await first.begin(['a']); const b = await second.begin(['b']);
  await a.shard('a').setName(1, 'parallel'); await b.shard('b').setName(1, 'parallel'); await a.commit(); await b.commit();
  const privateTx = await first.begin(['a', 'b']); await privateTx.shard('a').delete(1); await privateTx.rollback(); await privateTx.close();
  const unknown = await first.begin(['a', 'b']);
  await unknown.shard('a').setName(1, 'committed'); await unknown.shard('b').setName(1, 'committed');
  store.loseReply = true; await assert.rejects(unknown.commit(), { outcome: 'unknown' });
  const recovered = await second.begin(['a', 'b']);
  assert.equal(await recovered.shard('a').getName(1), 'committed'); assert.equal(await recovered.shard('b').getName(1), 'committed'); await recovered.close();
  const malformed = new ClusterEngine(binding, { read: async () => [], compareExchange() {} });
  await assert.rejects(malformed.begin(['a']), /incomplete/);
  store.values.set('bad', { generation: '1', entries: ['corrupt'] });
  await assert.rejects(first.begin(['a', 'bad']), /checksum/);
  console.log('ok wasm cluster: atomic groups, read-only dependency conflicts, independent shards, rollback, uncertain commits');
  const catalog = new Store(); const east = new Store(); const west = new Store();
  const partitioned = new PartitionedJournalStore(catalog, { east, west }, (key) => key === 'a' ? 'east' : 'west');
  const routed = new ClusterEngine(binding, partitioned);
  const group = await routed.begin(['a', 'b']);
  await group.shard('a').create(0, 0); await group.shard('b').create(0, 0); await group.commit();
  const read = await routed.begin(['a', 'b']);
  assert.equal(await read.shard('a').count(), 1n); assert.equal(await read.shard('b').count(), 1n); await read.close();
  assert.equal(east.values.size, 1); assert.equal(west.values.size, 1);
  assert.ok([...catalog.values.values()].every((value) => value.entries.length === 4 && value.entries[0] === 'blob1'));
  console.log('ok wasm partitioned: independent immutable journals and atomic catalog pointers');
}
