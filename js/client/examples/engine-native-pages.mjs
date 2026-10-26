// CLINK_WASM=<web package> node|bun|deno run --allow-all ... <database> [--delta] [--flat] [--snapshot] [--references] [--indexed-constraints]
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
const Provider = process.argv.includes('--mirrored')
  ? (await import('../src/engine-file-mirrored-pages.js')).FileMirroredPagedCommitLog : FilePagedCommitLog;
using physical = new Provider(path);
using engine = new WasmEngine(binding, {
  isolation: 'snapshot', history: 'all', requireDurableFlush: true,
  stateStructure: process.argv.includes('--flat') ? 'flat' : 'shared',
  revisionEncoding: process.argv.includes('--snapshot') ? 'snapshot' : 'changes',
  constraintValidation: process.argv.includes('--indexed-constraints') ? 'indexed' : 'scan',
  constraints: process.argv.includes('--references') || process.argv.includes('--indexed-constraints') ? ['references'] : [],
}, process.argv.includes('--delta') ? new DeltaCommitLog(physical) : physical);
const before = engine.snapshot(); assert.equal(before.revision, BigInt(before.rows.length));
for (const row of before.rows) {
  assert.equal(row.source, row.index % 2n === 1n ? 0n : row.index - 1n); assert.equal(row.target, 0n);
  assert.equal(row.name, row.index % 2n === 1n ? '\ufefffirst 雪\0😀' : 'second');
}
await using a = engine.begin(); const first = await a.create(0n, 0n); await a.setName(first.index, '\ufefffirst 雪\0😀');
await using b = engine.begin(); const second = await b.create(first.index, 0n); await b.setName(second.index, 'second');
assert.equal(first.index, before.revision + 1n); assert.equal(second.index, before.revision + 2n);
assert.deepEqual(await engine.commitGroup([a, b]), [{ status: 'committed', revision: before.revision + 1n }, { status: 'committed', revision: before.revision + 2n }]);
assert.equal(engine.snapshot().rows.length, before.rows.length + 2); assert.deepEqual(engine.snapshot(before.revision), before);
console.log(`Native pages group: ${before.revision} -> ${engine.snapshot().revision} revisions/rows; pinned predecessor and exact prior names verified`);
