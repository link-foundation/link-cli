// Finite actual-WASM scenario: independent profile edits under serializable OCC.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import * as binding from '../../pkg/clink_wasm.js';
import { WasmEngine } from '../src/index.js';
binding.initSync({ module: readFileSync(new URL('../../pkg/clink_wasm_bg.wasm', import.meta.url)) });
const engine = new WasmEngine(binding, { serializableValidation: 'read-set' });
try {
  const seed = engine.begin();
  const one = await seed.create(0n, 0n);
  const two = await seed.create(one.index, one.index);
  await seed.commit(); await seed.close();
  const first = engine.begin(); const second = engine.begin();
  try {
    assert.equal(await first.getName(one.index), null);
    assert.equal(await second.getName(two.index), null);
    await first.setName(one.index, 'profile one'); await second.setName(two.index, 'profile two');
    assert.equal(await first.commit(), 2n);
    assert.equal(await second.commit(), 3n);
  } finally { await first.close(); await second.close(); }
  assert.equal(engine.snapshot().rows.length, 2);
  assert.deepEqual(engine.snapshot().rows.map(row => row.name), ['profile one', 'profile two']);
  console.log('Independent point transactions both commit under serializable read-set validation');
} finally { engine.close(); }
