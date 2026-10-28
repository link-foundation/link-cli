// Native Rust/C# and Node/Bun/Deno may take turns on the same owned prefix.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { ClusterEngine, EtcdJournalStore, ReclaimingPartitionedJournalStore, WasmEngine } from '../src/index.js';
const [endpoints, prefix] = process.argv.slice(2);
if (!endpoints || !prefix) throw new TypeError('usage: endpoints-comma-separated dedicated-prefix/');
const directory = resolve(process.env.CLINK_WASM ?? 'js/pkg');
const binding = await import(pathToFileURL(resolve(directory, 'clink_wasm.js')).href);
binding.initSync({ module: await readFile(resolve(directory, 'clink_wasm_bg.wasm')) });
const provider = (name) => new EtcdJournalStore(endpoints.split(','), { prefix: `${prefix}${name}/` });
const catalog = provider('catalog');
const store = await ReclaimingPartitionedJournalStore.open(catalog, { east: provider('east'), west: provider('west') }, (key) => key === 'a' ? 'east' : 'west', { pageKeys: 1, cacheBytes: 65536, chunks: process.env.RECLAMATION_CHUNKS === '1' ? { maxRecords: 2 } : null });
async function verify() {
  const revisions = [];
  for (const journal of await store.read(['a', 'b'])) {
    const local = new WasmEngine(binding, { history: 'all' }, journal.entries);
    try {
      const current = local.snapshot(); const revision = current.revision;
      if (revision > 0n) {
        assert.equal(current.rows.length, 1); assert.equal(current.rows[0].name, `revision-${revision} 雪`);
        for (let old = 1n; old < revision; old++) assert.equal(local.snapshot(old).rows[0].name, `revision-${old} 雪`);
        const probe = local.begin();
        try { assert.equal((await probe.create(0, 0)).index, revision + 2n); await probe.rollback(); }
        finally { await probe.close(); }
      }
      revisions.push(revision);
    } finally { local.close(); }
  }
  assert.equal(revisions[0], revisions[1]); return revisions[0];
}
const revision = await verify(); assert.ok(revision < 32n);
const retired = (await catalog.read(['a', 'b'])).reduce((sum, root) => {
  if (root.generation === '0') return sum;
  if (root.entries[0] === 'chunks2') return sum + Number(root.entries[3]);
  assert.equal(root.entries[0], 'blob2'); return sum + 1;
}, 0);
const engine = new ClusterEngine(binding, store, { history: 'all', requireDurableFlush: true, journalCheckpointInterval: 1 });
const stale = await engine.begin(['a', 'b']); const write = await engine.begin(['a', 'b']);
let conflicted = false;
try {
  for (const key of ['a', 'b']) {
    const shard = write.shard(key);
    if (revision === 0n) assert.equal((await shard.create(0, 0)).index, 1n);
    const gap = (await shard.create(0, 0)).index; assert.equal(gap, revision + 2n); await shard.delete(gap);
    await shard.setName(1, `revision-${revision + 1n} 雪`);
  }
  await write.commit();
  const collected = await store.collectGarbage(); assert.ok(collected);
  assert.equal(collected.deleted, retired);
  if (revision > 0n) assert.equal(await stale.shard('a').getName(1), `revision-${revision} 雪`);
  await assert.rejects(stale.commit(), /conflict/);
  conflicted = true;
  assert.equal(await verify(), revision + 1n);
  console.log(`PASS WASM reclamation: revision ${revision + 1n}, atomic checkpoints, all retained history, deleted allocator gap, owned stale conflict, ${collected.deleted} retired objects`);
} finally {
  await write.close();
  // Closing joins the same terminal promise and preserves its conflict result.
  if (conflicted) await assert.rejects(stale.close(), /conflict/);
  else await stale.close();
}
