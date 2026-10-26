// CLINK_WASM=<web package> node|bun|deno run --allow-all ... <database> [--delta] [--flat|--sorted-runs] [--snapshot] [--references] [--indexed-constraints] [--checkpoint-fixture=<json>]
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { WasmEngine, DeltaCommitLog } from '../src/index.js';
import { FilePagedCommitLog } from '../src/engine-file-paged-log.js';
const path = process.argv.slice(2).find(arg => !arg.startsWith('--'));
if (!path || !process.env.CLINK_WASM) throw new Error('provide a dedicated database path and CLINK_WASM web package');
const binding = await import(pathToFileURL(join(process.env.CLINK_WASM, 'clink_wasm.js')).href);
binding.initSync({ module: readFileSync(join(process.env.CLINK_WASM, 'clink_wasm_bg.wasm')) });
const compact = process.argv.includes('--compact');
const Provider = compact ? (await import('../src/engine-file-checkpoint-pages.js')).FileCheckpointPagedCommitLog : process.argv.includes('--mirrored')
  ? (await import('../src/engine-file-mirrored-pages.js')).FileMirroredPagedCommitLog : FilePagedCommitLog;
using physical = new Provider(path, { mirroredBanks: process.argv.includes('--mirrored') });
const fixturePrefix = '--checkpoint-fixture=';
const fixture = process.argv.find(arg => arg.startsWith(fixturePrefix));
const checkpoint = fixture === undefined ? null : JSON.parse(readFileSync(fixture.slice(fixturePrefix.length), 'utf8')).nativePageSeed;
const log = process.argv.includes('--delta') ? new DeltaCommitLog(physical) : physical;
if (fixture !== undefined) {
  assert.equal(compact, false, 'compaction handoffs use their own two-revision retention control');
  assert.equal(typeof checkpoint, 'string');
  if (JSON.parse(log.read()).length === 0) { log.append(checkpoint); log.flush(); }
  assert.equal(JSON.parse(log.read())[0], checkpoint);
}
using engine = new WasmEngine(binding, {
  isolation: 'snapshot', history: compact ? 2 : 'all', requireDurableFlush: true,
  stateStructure: process.argv.includes('--flat') ? 'flat' : process.argv.includes('--sorted-runs') ? 'sorted-runs' : 'shared',
  revisionEncoding: process.argv.includes('--snapshot') ? 'snapshot' : 'changes',
  constraintValidation: process.argv.includes('--indexed-constraints') ? 'indexed' : 'scan',
  constraints: process.argv.includes('--references') || process.argv.includes('--indexed-constraints') ? ['references'] : [],
}, log);
const before = engine.snapshot();
const branch = compact ? engine.fork({ history: 'all', requireDurableFlush: false }) : null;
if (fixture !== undefined) {
  assert.ok(before.revision >= 40n); assert.equal(before.revision - 8n, BigInt(before.rows.length));
  assert.equal(engine.snapshot(40).rows.length, 32); assert.equal(engine.exportCheckpoint()[0], checkpoint);
  assert.throws(() => engine.snapshot(39), /not retained/);
  for (let id = 33n; id <= 40n; id++) assert.ok(!before.rows.some(row => row.index === id));
} else assert.equal(before.revision, BigInt(before.rows.length));
for (const row of before.rows) {
  assert.equal(row.source, row.index % 2n === 1n ? 0n : row.index - 1n); assert.equal(row.target, 0n);
  assert.equal(row.name, row.index % 2n === 1n ? '\ufefffirst 雪\0😀' : 'second');
}
await using a = engine.begin(); const first = await a.create(0n, 0n); await a.setName(first.index, '\ufefffirst 雪\0😀');
await using b = engine.begin(); const second = await b.create(first.index, 0n); await b.setName(second.index, 'second');
assert.equal(first.index, before.revision + 1n); assert.equal(second.index, before.revision + 2n);
assert.deepEqual(await engine.commitGroup([a, b]), [{ status: 'committed', revision: before.revision + 1n }, { status: 'committed', revision: before.revision + 2n }]);
assert.equal(engine.snapshot().rows.length, before.rows.length + 2);
if (!compact) assert.deepEqual(engine.snapshot(before.revision), before);
const after = engine.snapshot();
if (compact) {
  assert.throws(() => engine.snapshot(before.revision), /not retained/);
  await using pending = engine.begin(); assert.equal((await pending.create(0, 0)).index, after.revision + 1n);
  assert.equal(engine.compactJournal(), true); assert.equal(engine.compactJournal(), false);
  const descriptor = branch.mergeBaseRecord(), journal = branch.engine.journal(); branch.close();
  using resumed = engine.resumeBranch(journal, descriptor, { requireDurableFlush: false });
  assert.deepEqual(resumed.engine.snapshot().rows, before.rows); assert.equal(resumed.mergeBaseRecord(), descriptor); await pending.rollback();
  console.log(`Native compacting pages group: ${before.revision} -> ${after.revision} revisions; ${before.rows.length} -> ${after.rows.length} rows; private reservation ${after.revision + 1n}, pinned rows and branch base verified`);
  console.log(`Native checkpoint bank epoch: ${physical.epoch}`);
} else console.log(fixture !== undefined
  ? `Native checkpoint pages group: ${before.revision} -> ${after.revision} revisions; ${before.rows.length} -> ${after.rows.length} rows; pinned predecessor and exact prior names verified`
  : `Native pages group: ${before.revision} -> ${after.revision} revisions/rows; pinned predecessor and exact prior names verified`);
