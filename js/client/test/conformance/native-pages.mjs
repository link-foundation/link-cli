import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { WasmEngine, ReplicatedCommitLog, DeltaCommitLog } from '../../src/index.js';
import { FilePagedCommitLog } from '../../src/engine-file-paged-log.js';

import { FileMirroredPagedCommitLog } from '../../src/engine-file-mirrored-pages.js';

/** Real native files and actual wasm-bindgen calls, without a SQL provider. */
export async function nativePageConformance(binding) {
  const directory = mkdtempSync(join(tmpdir(), 'clink-wasm-pages-'));
  try {
    let configuration = 0;
    for (const Provider of [FilePagedCommitLog, FileMirroredPagedCommitLog]) {
    class CountedLog extends Provider {
      flushes = 0;
      flush() { this.flushes++; super.flush(); }
    }
    for (const stateStructure of ['flat', 'shared'])
      for (const revisionEncoding of ['snapshot', 'changes'])
        for (const constraintValidation of ['scan', 'indexed'])
          for (const delta of [false, true]) {
            const path = join(directory, String(configuration++));
            const expected = [];
            for (let round = 0; round < 2; round++) {
              if (round === 1 && Provider === FileMirroredPagedCommitLog) {
                // Damage a committed primary copy only after both handles close.
                for (const file of [path, path + '.replica']) {
                  const bytes = readFileSync(file); bytes[56] ^= 1; writeFileSync(file, bytes);
                  writeFileSync(file + '.meta0', new Uint8Array(17));
                }
              }
              using primary = new CountedLog(path);
              using replica = new CountedLog(path + '.replica');
              const copies = new ReplicatedCommitLog(primary, [replica], { acknowledgements: 2 });
              const log = delta ? new DeltaCommitLog(copies) : copies;
              using engine = new WasmEngine(binding, {
                stateStructure, revisionEncoding, constraintValidation,
                constraints: ['references'], isolation: 'snapshot', history: 'all', requireDurableFlush: true,
              }, log);
              assert.equal(engine.snapshot().revision, BigInt(round * 3));
              assert.deepEqual(engine.snapshot().rows, expected);
              const pinned = engine.snapshot();
              await using first = engine.begin();
              const a = await first.create(0n, 0n);
              await first.setName(a.index, '\ufeff雪\0😀' + round);
              await using second = engine.begin();
              const b = await second.create(a.index, 0n);
              await second.setName(b.index, 'second ' + round);
              const outcomes = await engine.commitGroup([first, second]);
              assert.deepEqual(outcomes, [
                { status: 'committed', revision: pinned.revision + 1n },
                { status: 'committed', revision: pinned.revision + 2n },
              ]);
              assert.equal(primary.flushes, 1); assert.equal(replica.flushes, 1);
              assert.deepEqual(engine.snapshot(pinned.revision), pinned);
              await using invalid = engine.begin();
              await invalid.create(999999n, 0n);
              await assert.rejects(invalid.commit(), /dangling|reference|constraint/i);
              assert.equal(primary.flushes, 1);
              await using discarded = engine.begin();
              await discarded.create(0n, 0n); await discarded.rollback();
              await using following = engine.begin();
              const c = await following.create(b.index, 0n);
              await following.setName(c.index, 'following');
              assert.equal(await following.commit(), pinned.revision + 3n);
              assert.equal(primary.flushes, 2); assert.equal(replica.flushes, 2);
              assert.equal(primary.read(), replica.read());
              expected.push(
                { ...a, name: '\ufeff雪\0😀' + round },
                { ...b, name: 'second ' + round },
                { ...c, name: 'following' },
              );
              assert.deepEqual(engine.snapshot().rows, expected);
            }
          }
    }
    console.log('ok wasm native pages: 32 independent policies, plain/mirrored real-file reopen and repair, exact names, shared flush, replica acknowledgement, rejected constraints and following writes');
  } finally { rmSync(directory, { recursive: true, force: true }); }
}
