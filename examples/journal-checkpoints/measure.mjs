// Finite logical payload accounting, not an I/O or latency benchmark.
// CLINK_WASM=/absolute/web-package node examples/journal-checkpoints/measure.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { ClusterEngine, WasmEngine } from '../../js/client/src/index.js';

const directory = resolve(process.env.CLINK_WASM ?? 'js/pkg');
const binding = await import(pathToFileURL(join(directory, 'clink_wasm.js')).href);
binding.initSync({ module: readFileSync(join(directory, 'clink_wasm_bg.wasm')) });
const bytes = (entries) => new TextEncoder().encode(JSON.stringify(entries)).length;
class Store {
  generation = 0; entries = []; readBytes = 0; publishedBytes = 0;
  async read() {
    this.readBytes += bytes(this.entries);
    return [{ generation: String(this.generation), entries: [...this.entries] }];
  }
  async compareExchange(keys, expected, updates) {
    assert.deepEqual(keys, ['a']);
    if (expected[0].generation !== String(this.generation)) return false;
    if (updates[0] !== null) {
      this.generation++; this.entries = [...updates[0]]; this.publishedBytes += bytes(this.entries);
    }
    return true;
  }
}
const results = [];
for (const stateStructure of ['flat', 'shared', 'sorted-runs']) {
  for (const revisionEncoding of ['snapshot', 'changes']) {
    for (const history of [null, 2, 'all']) {
      for (const journalCheckpointInterval of [undefined, 4]) {
        const store = new Store(); const options = { stateStructure, revisionEncoding, history };
        const engine = new ClusterEngine(binding, store, { ...options, journalCheckpointInterval });
        for (let revision = 1; revision <= 32; revision++) {
          await using tx = await engine.begin(['a']); const shard = tx.shard('a');
          if (revision === 1) await shard.create(0, 0);
          if (revision === 3) {
            for (let id = 2n; id <= 7n; id++) assert.equal((await shard.create(0, 0)).index, id);
            for (let id = 2n; id <= 7n; id++) await shard.delete(id);
          }
          await shard.setName(1, `雪 ${revision}`); await tx.commit();
        }
        using reopened = new WasmEngine(binding, options, store.entries);
        assert.equal(reopened.snapshot().revision, 32n);
        assert.equal(reopened.snapshot().rows[0].name, '雪 32');
        assert.equal(reopened.snapshot().rows.length, 1);
        for (let revision = 0; revision <= 32; revision++) {
          if (history === 'all' || revision === 32 || (history === 2 && revision === 31))
            assert.equal(reopened.snapshot(revision).revision, BigInt(revision));
          else assert.throws(() => reopened.snapshot(revision), /not retained/);
        }
        const compacted = reopened.exportCheckpoint();
        using copied = new WasmEngine(binding, options, compacted);
        assert.deepEqual(copied.snapshot(), reopened.snapshot());
        await using write = copied.begin(); assert.equal((await write.create(1, 0)).index, 8n); await write.commit();
        assert.equal(store.entries.length, journalCheckpointInterval === undefined ? 32 : history === 'all' ? 33 : history ?? 1);
        results.push({ stateStructure, revisionEncoding, history, interval: journalCheckpointInterval ?? null,
          records: store.entries.length, bytes: bytes(store.entries), readBytes: store.readBytes, publishedBytes: store.publishedBytes });
      }
    }
  }
}
console.log(JSON.stringify({ revisions: 32, results }, null, 2));
