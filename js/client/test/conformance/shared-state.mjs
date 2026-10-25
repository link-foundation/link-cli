import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { WasmEngine, MemoryCommitLog, DeltaCommitLog } from '../../src/index.js';

/** Public JS/TS options exercise the actual compiled native core in each runtime. */
export async function sharedStateConformance(binding) {
  for (const revisionEncoding of ['snapshot', 'changes']) {
    for (const stateStructure of ['flat', 'shared']) {
      for (const delta of [false, true]) {
        const physical = new MemoryCommitLog();
        const log = () => delta ? new DeltaCommitLog(physical) : physical;
        const engine = new WasmEngine(binding, { stateStructure, revisionEncoding, isolation: 'snapshot', history: 'all' }, log());
        try {
          const seed = engine.begin();
          const one = await seed.create(0n, 0n); const two = await seed.create(one.index, 0n);
          await seed.commit(); await seed.close();
          const pinned = engine.snapshot();
          const a = engine.begin(); const b = engine.begin();
          await a.setName(one.index, 'one 雪'); await b.setName(two.index, 'two');
          assert.deepEqual(await engine.commitGroup([a, b]), [
            { status: 'committed', revision: 2n }, { status: 'committed', revision: 3n },
          ]);
          await a.close(); await b.close();
          assert.equal(pinned.rows[0].name, null);
          const other = stateStructure === 'shared' ? 'flat' : 'shared';
          const otherEncoding = revisionEncoding === 'changes' ? 'snapshot' : 'changes';
          const child = engine.fork({ stateStructure: other, revisionEncoding: otherEncoding }); const base = child.mergeBaseRecord();
          assert.match(base, /^engine1\|/);
          const edit = child.engine.begin(); await edit.removeName(two.index); await edit.commit(); await edit.close();
          const records = child.engine.journal(); child.close();
          const resumed = engine.resumeBranch(records, base, { stateStructure: other, revisionEncoding: otherEncoding });
          try { assert.equal(resumed.merge(), 4n); assert.equal(resumed.merge(), 4n); }
          finally { resumed.close(); }
          assert.equal(engine.snapshot().rows[1].name, null);
          const reopened = new WasmEngine(binding, { stateStructure: other, revisionEncoding: otherEncoding, history: 'all' }, log());
          try {
            assert.deepEqual(reopened.snapshot(), engine.snapshot());
            assert.equal(reopened.snapshot(1n).rows[0].name, null);
            const following = reopened.begin(); await following.setName(one.index, 'following'); await following.commit(); await following.close();
            assert.equal(pinned.rows[0].name, null);
          } finally { reopened.close(); }
        } finally { engine.close(); }
      }
      const vectors = JSON.parse(readFileSync(new URL('../../../../docs/protocol/engine-revision-vectors.json', import.meta.url), 'utf8'));
      const wire = new WasmEngine(binding, { stateStructure, history: 'all' }, vectors.valid);
      try {
        assert.equal(wire.snapshot(1n).rows[0].name, '雪;|😀');
        assert.deepEqual(wire.journal(), vectors.valid);
      } finally { wire.close(); }
    }
  }
  for (const stateStructure of ['mutable', 42, [], {}])
    assert.throws(() => new WasmEngine(binding, { stateStructure }));
  console.log('ok wasm engine: independent flat/shared state and snapshot/changes records, groups, mixed branches and plain/delta reopen');
}
