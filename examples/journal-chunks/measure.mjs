// Finite actual-engine payload accounting, not a physical I/O/latency benchmark.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { ClusterEngine, PartitionedJournalStore, WasmEngine } from '../../js/client/src/index.js';

const directory = resolve(process.env.CLINK_WASM ?? 'js/pkg');
const binding = await import(pathToFileURL(join(directory, 'clink_wasm.js')).href);
binding.initSync({ module: readFileSync(join(directory, 'clink_wasm_bg.wasm')) });
const encoder = new TextEncoder();
const bytes = records => records.reduce((sum, record) => sum + encoder.encode(record).length, 0);
class Store {
  values = new Map(); generation = 0n;
  reads = 0; writes = 0; readBytes = 0; writtenBytes = 0;
  async read(keys) {
    this.reads++;
    return keys.map(key => {
      const value = this.values.get(key) ?? { generation: '0', entries: [] };
      this.readBytes += bytes(value.entries);
      return structuredClone(value);
    });
  }
  async compareExchange(keys, expected, updates) {
    this.writes++;
    if (keys.some((key, i) => (this.values.get(key)?.generation ?? '0') !== expected[i].generation)) return false;
    const generation = String(++this.generation);
    for (let i = 0; i < keys.length; i++) if (updates[i] !== null) {
      const entries = [...updates[i]];
      this.writtenBytes += bytes(entries);
      this.values.set(keys[i], { generation, entries });
    }
    return true;
  }
}

const revisions = 16;
const results = [];
for (const [layout, records, cacheBytes] of [['whole', null, 0], ['chunks-2-records', 2, 0], ['chunks-2-records-cache', 2, 262144], ['chunks-4-records-cache', 4, 262144]]) {
  const catalog = new Store(); const east = new Store(); const west = new Store();
  const journals = new PartitionedJournalStore(catalog, { east, west }, key => key === 'a' ? 'east' : 'west',
    { cacheBytes, chunks: records === null ? null : { maxRecords: records, maxBytes: 65536, maxChunks: 64 } });
  const engine = new ClusterEngine(binding, journals, { stateStructure: 'shared', revisionEncoding: 'changes', history: 'all' });
  for (let revision = 1; revision <= revisions; revision++) {
    await using tx = await engine.begin(['a', 'b']);
    for (const key of ['a', 'b']) {
      if (revision === 1) await tx.shard(key).create(0, 0);
      await tx.shard(key).setName(1, `${key} 雪😀 ${revision}`);
    }
    await tx.commit();
  }
  // Capture costs before the following separate read/replay verification.
  results.push({ layout, maxRecords: records, cacheBytes,
    dataWrittenBytes: east.writtenBytes + west.writtenBytes,
    dataReadBytes: east.readBytes + west.readBytes,
    dataReadRequests: east.reads + west.reads, dataWriteRequests: east.writes + west.writes,
    catalogWrittenBytes: catalog.writtenBytes, catalogReadBytes: catalog.readBytes,
    catalogReadRequests: catalog.reads, catalogWriteRequests: catalog.writes,
    retainedObjects: east.values.size + west.values.size });
  for (const [index, key] of ['a', 'b'].entries()) {
    const snapshot = (await journals.read(['a', 'b']))[index];
    using reopened = new WasmEngine(binding, { stateStructure: 'shared', revisionEncoding: 'changes', history: 'all' }, snapshot.entries);
    assert.equal(snapshot.entries.length, revisions);
    assert.equal(reopened.snapshot().revision, BigInt(revisions));
    assert.equal(reopened.snapshot().rows[0].name, `${key} 雪😀 ${revisions}`);
    for (let revision = 1; revision <= revisions; revision++)
      assert.equal(reopened.snapshot(revision).rows[0].name, `${key} 雪😀 ${revision}`);
    await using following = reopened.begin();
    assert.equal((await following.create(1, 0)).index, 2n); await following.commit();
  }
}
assert.ok(results.slice(1).every(result => result.dataWrittenBytes < results[0].dataWrittenBytes));
assert.equal(results[1].dataWrittenBytes, results[2].dataWrittenBytes);
assert.ok(results[2].dataReadBytes < results[1].dataReadBytes);
assert.ok(results.every(result => result.catalogWriteRequests === revisions));
console.log(JSON.stringify({ revisions, shards: 2, units: 'UTF-8 record payload bytes; keys/framing/physical I/O excluded', results }, null, 2));
