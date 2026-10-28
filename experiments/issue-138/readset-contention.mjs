// Finite acceptance comparison, not a latency/throughput benchmark.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import * as binding from '../../js/pkg/clink_wasm.js';
import { WasmEngine } from '../../js/client/src/index.js';
binding.initSync({ module: readFileSync(new URL('../../js/pkg/clink_wasm_bg.wasm', import.meta.url)) });
for (const serializableValidation of ['revision', 'read-set']) {
  const engine = new WasmEngine(binding, { serializableValidation });
  const sessions = [];
  try {
    const seed = engine.begin(); const ids = [];
    try {
      for (let i = 0; i < 32; i++) ids.push((await seed.create(0n, 0n)).index);
      await seed.commit();
    } finally { await seed.close(); }
    for (const id of ids) {
      const session = engine.begin(); sessions.push(session);
      assert.equal(await session.getName(id), null);
      await session.setName(id, `profile-${id}`);
    }
    let committed = 0; let aborted = 0;
    for (const session of sessions) {
      try { await session.commit(); committed++; }
      catch (error) { assert.match(String(error), /conflict/i); aborted++; }
    }
    assert.equal(committed, serializableValidation === 'read-set' ? 32 : 1);
    assert.equal(engine.snapshot().rows.filter(row => row.name !== null).length, committed);
    console.log(JSON.stringify({ serializableValidation, sessions: 32, committed, aborted }));
  } finally { for (const session of sessions) await session.close(); engine.close(); }
}
