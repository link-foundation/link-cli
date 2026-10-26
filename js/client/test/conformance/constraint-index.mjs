import assert from 'node:assert/strict';
import { WasmEngine, MemoryCommitLog, DeltaCommitLog } from '../../src/index.js';

/** The option crosses the public JS queue and actual compiled WASM boundary. */
export async function constraintIndexConformance(binding) {
  for (const stateStructure of ['flat', 'shared', 'sorted-runs']) {
    for (const revisionEncoding of ['snapshot', 'changes']) {
      for (const constraintValidation of ['scan', 'indexed']) {
        const physical = new MemoryCommitLog();
        const log = () => new DeltaCommitLog(physical);
        const options = { stateStructure, revisionEncoding, constraintValidation, isolation: 'snapshot',
          history: 'all', constraints: ['references', 'unique-pairs', 'unique-names'] };
        const engine = new WasmEngine(binding, options, log());
        try {
          const seed = engine.begin();
          for (const [id, name] of [[1n, 'a'], [2n, '雪\0'], [3n, '']]) {
            assert.equal((await seed.create(id, id)).index, id); await seed.setName(id, name);
          }
          await seed.commit(); await seed.close();
          const pinned = engine.snapshot();
          const swap = engine.begin();
          await swap.update(1n, 2n, 2n); await swap.setName(1n, '雪\0');
          await swap.update(2n, 1n, 1n); await swap.setName(2n, 'a');
          await swap.commit(); await swap.close();
          const bad = engine.begin(); await bad.delete(1n);
          await assert.rejects(bad.commit(), /reference/i); await bad.close();
          assert.equal(engine.snapshot().revision, 2n);
          const a = engine.begin(); const b = engine.begin();
          await a.setName(1n, 'group'); await b.setName(2n, 'group');
          const outcomes = await engine.commitGroup([a, b]);
          assert.equal(outcomes[0].status, 'committed'); assert.equal(outcomes[1].status, 'aborted');
          await a.close(); await b.close();
          assert.equal(pinned.rows[0].name, 'a');
          const branch = engine.fork({ constraintValidation: 'indexed', stateStructure, revisionEncoding });
          try {
            const edit = branch.engine.begin(); await edit.setName(2n, 'group'); await edit.commit(); await edit.close();
            assert.throws(() => branch.merge(), /duplicate name/i);
            const fix = branch.engine.begin(); await fix.setName(2n, 'child'); await fix.commit(); await fix.close();
            assert.equal(branch.merge(), 4n); assert.equal(branch.merge(), 4n);
          } finally { branch.close(); }
          const reopened = new WasmEngine(binding, { ...options, constraintValidation: constraintValidation === 'scan' ? 'indexed' : 'scan' }, log());
          try {
            assert.deepEqual(reopened.snapshot(), engine.snapshot());
            const transfer = reopened.begin(); await transfer.removeName(1n); await transfer.setName(3n, 'group'); await transfer.commit(); await transfer.close();
          } finally { reopened.close(); }
        } finally { engine.close(); }
      }
    }
  }
  for (const constraintValidation of ['disabled', 42, [], {}]) assert.throws(() => new WasmEngine(binding, { constraintValidation }));
  console.log('ok wasm engine: scan/indexed constraints, atomic swaps, incoming references, rejected groups/merges and mixed replay');
}
