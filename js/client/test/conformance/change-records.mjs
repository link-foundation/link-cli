import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { WasmEngine, MemoryCommitLog, DeltaCommitLog } from '../../src/index.js';

/** Independent wire fixtures cross the actual JS-to-WASM boundary. */
export async function changeRecordConformance(binding) {
  const vectors = JSON.parse(readFileSync(new URL('../../../../docs/protocol/engine-change-vectors.json', import.meta.url), 'utf8'));
  for (const stateStructure of ['flat', 'shared']) {
    for (const revisionEncoding of ['snapshot', 'changes']) {
      for (const records of [vectors.valid, vectors.mixed]) {
        const physical = new MemoryCommitLog(records);
        const options = { stateStructure, revisionEncoding, history: 'all' };
        const engine = new WasmEngine(binding, options, physical);
        try {
          assert.equal(engine.snapshot().revision, 4n);
          assert.equal(engine.snapshot(1n).rows[0].name, '雪;|😀\0\n');
          assert.equal(engine.snapshot(1n).rows[1].name, '');
          assert.equal(engine.snapshot(2n).rows.length, 1);
          assert.equal(engine.snapshot().rows[1].index, 4294967295n);
          const exhausted = engine.begin();
          try { await assert.rejects(exhausted.create(0n, 0n), /address/i); }
          finally { await exhausted.close(); }
          const tx = engine.begin();
          try {
            await tx.setName(1n, 'following 雪');
            assert.equal(await tx.commit(), 5n);
          } finally { await tx.close(); }
          assert.match(JSON.parse(physical.read()).at(-1), revisionEncoding === 'changes' ? /^engine2\|/ : /^engine1\|/);
        } finally { engine.close(); }
        const reopened = new WasmEngine(binding, {}, physical);
        try { assert.equal(reopened.snapshot().rows[0].name, 'following 雪'); }
        finally { reopened.close(); }
      }
      for (const invalid of vectors.invalidU32) {
        const physical = new MemoryCommitLog(invalid.records);
        const before = physical.read();
        assert.throws(() => new WasmEngine(binding, { stateStructure, revisionEncoding }, physical), undefined, invalid.case);
        assert.equal(physical.read(), before);
      }
      const physical = new MemoryCommitLog(vectors.physicalMixed.physical);
      const engine = new WasmEngine(binding, { stateStructure, revisionEncoding, history: 'all' }, new DeltaCommitLog(physical));
      try {
        assert.equal(engine.snapshot().revision, 5n);
        assert.equal(engine.snapshot().rows.length, 31);
        assert.deepEqual(engine.journal(), vectors.physicalMixed.logical);
      } finally { engine.close(); }
    }
  }
  for (const revisionEncoding of ['delta', 42, [], {}])
    assert.throws(() => new WasmEngine(binding, { revisionEncoding }));
  console.log('ok wasm engine: canonical changes, mixed wire replay, allocator, names, rejected corruption and physical delta composition');
}
