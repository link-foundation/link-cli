// Finite write-amplification probe: 1,000 rows, ten one-row name updates.
// Build rust/wasm with wasm-pack before running this script.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import * as binding from '../../js/pkg/clink_wasm.js';
import { WasmEngine, MemoryCommitLog, DeltaCommitLog } from '../../js/client/src/index.js';

binding.initSync({ module: readFileSync(new URL('../../js/pkg/clink_wasm_bg.wasm', import.meta.url)) });
async function measure(compressed) {
 const physical = new MemoryCommitLog();
 const engine = new WasmEngine(binding, { history: 'all' }, compressed ? new DeltaCommitLog(physical) : physical);
 try {
  const seed = engine.begin();
  try {
    for (let i = 0; i < 1_000; i++) await seed.create(0n, 0n);
    await seed.commit();
  } finally { await seed.close(); }
  for (let i = 0; i < 10; i++) {
    const tx = engine.begin();
    try { await tx.setName(500n, `edit-${i}`); await tx.commit(); }
    finally { await tx.close(); }
  }
  const records = JSON.parse(physical.read());
  assert.equal(records.length, 11);
  assert.equal(engine.snapshot().rows.length, 1_000);
  const result = { strategy: compressed ? 'delta1' : 'engine1', rows: 1_000, oneRowUpdates: 10,
    totalRecordBytes: records.reduce((sum, text) => sum + Buffer.byteLength(text), 0),
    initialRecordBytes: Buffer.byteLength(records[0]),
    lastUpdateBytes: Buffer.byteLength(records.at(-1)),
  };
  const reopened = new WasmEngine(binding, { history: 'all' }, compressed ? new DeltaCommitLog(physical) : physical);
  try {
    assert.equal(reopened.snapshot(1n).rows.find(row => row.index === 500n).name, null);
    assert.equal(reopened.snapshot().rows.find(row => row.index === 500n).name, 'edit-9');
    assert.deepEqual(reopened.journal(), engine.journal());
  } finally { reopened.close(); }
  return result;
 } finally { engine.close(); }
}
const baseline = await measure(false); const delta = await measure(true);
assert.ok(delta.lastUpdateBytes < baseline.lastUpdateBytes / 10);
assert.ok(delta.totalRecordBytes < baseline.totalRecordBytes / 5);
console.log(JSON.stringify({ baseline, delta,
  totalByteReductionPercent: 100 * (1 - delta.totalRecordBytes / baseline.totalRecordBytes) }));
