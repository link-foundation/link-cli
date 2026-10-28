// Finite actual-WASM payload accounting with explicit collection frequencies.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { ClusterEngine, ReclaimingPartitionedJournalStore, WasmEngine } from '../../js/client/src/index.js';
import { MemoryMaintenanceStore } from '../../experiments/issue-138/reclamation-memory.mjs';
const directory = resolve(process.env.CLINK_WASM ?? 'js/pkg');
const binding = await import(pathToFileURL(resolve(directory, 'clink_wasm.js')).href);
binding.initSync({ module: readFileSync(resolve(directory, 'clink_wasm_bg.wasm')) });
const encoder = new TextEncoder();
const bytes = records => records.reduce((sum, record) => sum + encoder.encode(record).length, 0);
class Measured extends MemoryMaintenanceStore {
  readBytes = 0; writtenBytes = 0; scanRecordBytes = 0;
  async read(keys) {
    const result = await super.read(keys);
    for (const item of result) this.readBytes += bytes(item.entries);
    return result;
  }
  async compareExchange(keys, expected, updates) {
    const result = await super.compareExchange(keys, expected, updates);
    if (result) for (const records of updates) if (records !== null) this.writtenBytes += bytes(records);
    return result;
  }
  async scanNamespace(options) {
    const result = await super.scanNamespace(options);
    for (const item of result) this.scanRecordBytes += bytes(item.version.entries);
    return result;
  }
  count(operation) { return this.calls.filter(call => call.operation === operation).length; }
}
const revisions = 16; const results = [];
for (const [layout, maxRecords, collectEvery] of [['whole', null, 0], ['chunks', 2, 0], ['chunks-fence-4', 2, 4], ['chunks-fence-1', 2, 1]]) {
  const catalog = new Measured('catalog'); const east = new Measured('east'); const west = new Measured('west');
  const journals = await ReclaimingPartitionedJournalStore.open(catalog, { east, west }, key => key === 'a' ? 'east' : 'west',
    { pageKeys: 1, cacheBytes: 262144, chunks: maxRecords === null ? null : { maxRecords, maxBytes: 65536, maxChunks: 64 } });
  const engine = new ClusterEngine(binding, journals, { stateStructure: 'shared', revisionEncoding: 'changes', history: 'all' });
  let deleted = 0;
  for (let revision = 1; revision <= revisions; revision++) {
    await using tx = await engine.begin(['a', 'b']);
    for (const key of ['a', 'b']) {
      if (revision === 1) await tx.shard(key).create(0, 0);
      await tx.shard(key).setName(1, `${key} 雪😀 ${revision}`);
    }
    await tx.commit();
    if (collectEvery && revision % collectEvery === 0) {
      const result = await journals.collectGarbage(); assert.ok(result); deleted += result.deleted;
    }
  }
  // Read/replay correctness checks below are excluded from measured counters.
  results.push({ layout, maxRecords, collectEvery,
    dataWrittenBytes: east.writtenBytes + west.writtenBytes, dataReadBytes: east.readBytes + west.readBytes,
    dataReadRequests: east.count('read') + west.count('read'), dataWriteRequests: east.count('swap') + west.count('swap'),
    catalogWrittenBytes: catalog.writtenBytes, catalogReadBytes: catalog.readBytes,
    catalogReadRequests: catalog.count('read'), catalogWriteRequests: catalog.count('swap'),
    namespaceScanRequests: catalog.count('scan') + east.count('scan') + west.count('scan'),
    catalogScanRecordBytes: catalog.scanRecordBytes, dataScanRecordBytes: east.scanRecordBytes + west.scanRecordBytes,
    deleteRequests: east.count('delete') + west.count('delete'), deleted, retainedObjects: east.values.size + west.values.size });
  const owned = await journals.read(['a', 'b']);
  for (const [index, key] of ['a', 'b'].entries()) {
    using reopened = new WasmEngine(binding, { stateStructure: 'shared', revisionEncoding: 'changes', history: 'all' }, owned[index].entries);
    assert.equal(owned[index].entries.length, revisions); assert.equal(reopened.snapshot().revision, BigInt(revisions));
    for (let revision = 1; revision <= revisions; revision++)
      assert.equal(reopened.snapshot(revision).rows[0].name, `${key} 雪😀 ${revision}`);
    await using following = reopened.begin(); assert.equal((await following.create(1, 0)).index, 2n); await following.commit();
  }
  const final = await journals.collectGarbage(); assert.ok(final);
  assert.deepEqual((await journals.read(['a', 'b'])).map(value => value.entries), owned.map(value => value.entries));
  if (maxRecords !== null) assert.equal(east.values.size + west.values.size, revisions);
}
assert.ok(results[1].dataWrittenBytes < results[2].dataWrittenBytes);
assert.ok(results[2].dataWrittenBytes < results[3].dataWrittenBytes);
assert.equal(results[0].dataWrittenBytes, results[3].dataWrittenBytes);
assert.ok(results[2].retainedObjects <= results[1].retainedObjects);
assert.ok(results[2].deleted > 0 && results[3].deleted > 0);
assert.equal(results[0].namespaceScanRequests, results[1].namespaceScanRequests);
console.log(JSON.stringify({ revisions, shards: 2, cacheBytes: 262144,
  units: 'UTF-8 record payload bytes; keys/framing/physical I/O excluded', results }, null, 2));
