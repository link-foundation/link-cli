// Real compiled binding; a missing strategy is a feature baseline, not a bug.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { WasmEngine } from '../../js/client/src/index.js';

assert.ok(process.env.CLINK_WASM, 'provide the generated actual WASM web package');
const binding = await import(pathToFileURL(join(process.env.CLINK_WASM, 'clink_wasm.js')).href);
binding.initSync({ module: readFileSync(join(process.env.CLINK_WASM, 'clink_wasm_bg.wasm')) });
for (const revisionEncoding of ['snapshot', 'changes']) {
  using engine = new WasmEngine(binding, { stateStructure: 'sorted-runs', revisionEncoding, history: 'all' });
  await using seed = engine.begin();
  const first = await seed.create(0, 0);
  await seed.setName(first.index, 'original 雪\0');
  await seed.commit();
  const pinned = engine.snapshot();
  for (let n = 1n; n <= 16n; n++) {
    await using writer = engine.begin();
    await writer.update(first.index, n, 0);
    await writer.setName(first.index, `updated ${n}`);
    await writer.commit();
    assert.equal(engine.snapshot().rows[0].source, n);
    assert.deepEqual(engine.snapshot(1n), pinned);
  }
  await using discarded = engine.begin();
  await discarded.delete(first.index);
  await discarded.rollback();
  assert.equal(engine.snapshot().rows.length, 1);
  for (const stateStructure of ['flat', 'shared', 'sorted-runs']) {
    using reopened = new WasmEngine(binding, { stateStructure }, engine.journal());
    assert.deepEqual(reopened.snapshot(), engine.snapshot());
  }
}
console.log('Actual-WASM sorted-run selection, edits, pins, rollback and all-structure reopening pass.');
