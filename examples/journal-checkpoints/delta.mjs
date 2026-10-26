// Copy a retained checkpoint into independently selected physical delta logs.
// This finite example uses volatile logs; it does not measure disk durability.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { DeltaCommitLog, MemoryCommitLog, ReplicatedCommitLog, WasmEngine } from '../../js/client/src/index.js';

const directory = resolve(process.env.CLINK_WASM ?? 'js/pkg');
const binding = await import(pathToFileURL(join(directory, 'clink_wasm.js')).href);
binding.initSync({ module: readFileSync(join(directory, 'clink_wasm_bg.wasm')) });
const results = [];
for (const stateStructure of ['flat', 'shared', 'sorted-runs']) {
  for (const revisionEncoding of ['snapshot', 'changes']) {
    for (const history of [null, 2, 'all']) {
      const options = { stateStructure, revisionEncoding, history };
      using original = new WasmEngine(binding, options);
      for (let revision = 1; revision <= 6; revision++) {
        await using tx = original.begin();
        if (revision === 1) {
          for (let id = 1n; id <= 40n; id++) assert.equal((await tx.create(0, 0)).index, id);
          for (let id = 33n; id <= 40n; id++) await tx.delete(id);
        }
        await tx.setName(16, `雪;|😀 ${revision}`); await tx.commit();
      }
      const exported = original.exportCheckpoint();
      for (const compressOutside of [false, true]) {
        const primary = new MemoryCommitLog(); const follower = new MemoryCommitLog();
        const replica = new ReplicatedCommitLog(
          compressOutside ? primary : new DeltaCommitLog(primary, { checkpointInterval: 1 }),
          [compressOutside ? follower : new DeltaCommitLog(follower)], { acknowledgements: 2 });
        const log = compressOutside ? new DeltaCommitLog(replica) : replica;
        for (const record of exported) log.append(record);
        log.flush();
        using copy = new WasmEngine(binding, options, log);
        assert.deepEqual(copy.snapshot(), original.snapshot());
        for (let revision = 0; revision <= 6; revision++) {
          if (history === 'all' || revision === 6 || (history === 2 && revision === 5))
            assert.deepEqual(copy.snapshot(revision), original.snapshot(revision));
          else assert.throws(() => copy.snapshot(revision), /not retained/);
        }
        const pinned = copy.snapshot();
        await using tx = copy.begin();
        assert.equal((await tx.create(1, 0)).index, 41n);
        await tx.setName(16, 'after checkpoint'); await tx.commit();
        assert.equal(pinned.revision, 6n); assert.equal(pinned.rows.length, 32);
        assert.equal(pinned.rows.find((row) => row.index === 16n).name, '雪;|😀 6');
        assert.equal(replica.synchronize(), 2);
        const logical = copy.journal();
        for (const raw of [primary, follower]) {
          assert.equal(JSON.parse(raw.read())[0], exported[0]);
          using reopened = new WasmEngine(binding, options, new DeltaCommitLog(raw));
          assert.deepEqual(reopened.snapshot(), copy.snapshot());
          assert.deepEqual(reopened.journal(), logical);
        }
        if (compressOutside) assert.equal(primary.read(), follower.read());
        else if (history !== null || revisionEncoding === 'snapshot') assert.notEqual(primary.read(), follower.read());
        const last = JSON.parse(follower.read()).at(-1);
        assert.match(last, revisionEncoding === 'snapshot' ? /^delta1\|/ : /^engine2\|/);
        results.push({ stateStructure, revisionEncoding, history, compressOutside,
          records: logical.length, primaryBytes: new TextEncoder().encode(primary.read()).length,
          followerBytes: new TextEncoder().encode(follower.read()).length });
      }
    }
  }
}
console.log(JSON.stringify({ cases: results.length, results }, null, 2));
