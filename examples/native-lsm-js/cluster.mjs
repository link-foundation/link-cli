// Actual WASM engine over native files; bounded direct and partitioned consumers.
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { FileLsmJournalStore } from '../../js/client/src/engine-file-lsm-store.js';
import { ClusterEngine, PartitionedJournalStore, ReclaimingPartitionedJournalStore } from '../../js/client/src/index.js';

const packagePath = resolve(process.env.CLINK_WASM ?? 'js/pkg');
const binding = await import(pathToFileURL(join(packagePath, 'clink_wasm.js')).href);
binding.initSync({ module: readFileSync(join(packagePath, 'clink_wasm_bg.wasm')) });
const options = { maxKeys: 256, maxBytes: 1048576, bufferKeys: 64, bufferBytes: 262144, maxPages: 128 };
for (const mode of ['direct', 'partitioned', 'chunks', 'reclaiming']) {
  const directory = mkdtempSync(join(tmpdir(), 'clink-lsm-consumer-'));
  let stores = [];
  async function open() {
    const names = mode === 'direct' ? ['direct'] : ['catalog', 'east', 'west'];
    stores = names.map(name => {
      const path = join(directory, name); if (!readdirSync(directory).includes(name)) mkdirSync(path);
      return new FileLsmJournalStore(path, options);
    });
    if (mode === 'direct') return stores[0];
    const args = [stores[0], { east: stores[1], west: stores[2] }, key => key === 'a' ? 'east' : 'west'];
    return mode === 'chunks' ? new PartitionedJournalStore(...args, { chunks: { maxRecords: 2 } }) : mode === 'partitioned' ? new PartitionedJournalStore(...args) : await ReclaimingPartitionedJournalStore.open(...args);
  }
  function close() { for (const store of stores) { store.compact(); store.close(); } stores = []; }
  try {
    let journals = await open();
    let engine = new ClusterEngine(binding, journals, { requireDurableFlush: process.platform !== 'win32', journalCheckpointInterval: 1 });
    for (let revision = 0; revision < 3; revision++) {
      const transaction = await engine.begin(['a', 'b']);
      try {
        for (const key of ['a', 'b']) {
          const shard = transaction.shard(key); if (await shard.count() === 0n) await shard.create(0, 0);
          await shard.setName(1, `revision-${revision} 雪😀`);
        }
        await transaction.commit();
      } finally { await transaction.close(); }
    }
    const stale = await engine.begin(['a', 'b']), winner = await engine.begin(['a']);
    await winner.shard('a').setName(1, 'winner'); await winner.commit(); await winner.close();
    await stale.shard('b').setName(1, 'lost'); await assert.rejects(stale.commit(), { outcome: 'failed' });
    await assert.rejects(stale.close(), { outcome: 'failed' });
    if (mode === 'reclaiming') { const result = await journals.collectGarbage(); assert.ok(result.deleted > 0); }
    close(); journals = await open();
    engine = new ClusterEngine(binding, journals, { requireDurableFlush: process.platform !== 'win32' });
    const reader = await engine.begin(['a', 'b']);
    try {
      assert.equal(await reader.shard('a').getName(1), 'winner');
      assert.equal(await reader.shard('b').getName(1), 'revision-2 雪😀');
      for (const key of ['a', 'b']) assert.equal(await reader.shard(key).count(), 1n);
    } finally { await reader.close(); }
    close();
    console.log(`${mode}: actual WASM links, checkpoint journals, stale group rejection and native file reopen pass`);
  } finally { for (const store of stores) store.close(); rmSync(directory, { recursive: true, force: true }); }
}
