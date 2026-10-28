// Finite logical serialization control; run with CLINK_WASM pointing to a web package.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { WasmEngine, MemoryCommitLog } from '../../js/client/src/index.js';

assert.ok(process.env.CLINK_WASM, 'Set CLINK_WASM to the generated web package');
const binding = await import(pathToFileURL(join(process.env.CLINK_WASM, 'clink_wasm.js')).href);
binding.initSync({ module: readFileSync(join(process.env.CLINK_WASM, 'clink_wasm_bg.wasm')) });
const changes = process.argv.includes('--changes');
const shared = process.argv.includes('--shared');
const options = {};
if (changes) options.revisionEncoding = 'changes';
if (shared) options.stateStructure = 'shared';
const log = new MemoryCommitLog();
const engine = new WasmEngine(binding, options, log);
const ROWS = 256; const COMMITS = 16;
let pinned;
try {
  const seed = engine.begin();
  try {
    for (let id = 1; id <= ROWS; id++) {
      const link = await seed.create(0n, 0n);
      assert.equal(link.index, BigInt(id));
      await seed.setName(link.index, `untouched row ${id} ${'雪'.repeat(32)}`);
    }
    await seed.commit();
  } finally { await seed.close(); }
  pinned = engine.snapshot();
  for (let step = 1; step <= COMMITS; step++) {
    const tx = engine.begin();
    try { await tx.update(1n, BigInt(step), 0n); await tx.commit(); }
    finally { await tx.close(); }
  }
  assert.equal(engine.snapshot().rows[0].source, BigInt(COMMITS));
  assert.equal(pinned.rows[0].source, 0n);
} finally { engine.close(); }
const records = JSON.parse(log.read());
assert.equal(records.length, COMMITS + 1);
let bytes = 0; let encodedRows = 0;
for (const record of records.slice(1)) {
  const fields = record.split('|');
  assert.equal(fields[0], changes ? 'engine2' : 'engine1');
  const payload = fields[changes ? 4 : 3];
  const count = payload.split(';').length;
  assert.equal(count, changes ? 1 : ROWS);
  bytes += Buffer.byteLength(record, 'utf8'); encodedRows += count;
}
const reopened = new WasmEngine(binding, options, log);
try {
  const actual = reopened.snapshot(); assert.equal(actual.rows.length, ROWS);
  assert.equal(actual.rows[0].source, BigInt(COMMITS));
  assert.deepEqual(actual.rows.slice(1), pinned.rows.slice(1));
} finally { reopened.close(); }
console.log(JSON.stringify({ rows: ROWS, commits: COMMITS, state: shared ? 'shared' : 'flat', encoding: changes ? 'changes' : 'snapshot', encoded_update_rows: encodedRows, encoded_update_bytes: bytes }));
