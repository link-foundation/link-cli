import assert from 'node:assert/strict';
import { WasmEngine, MemoryCommitLog, ReplicatedCommitLog, DeltaCommitLog } from '../../src/index.js';

// Execute the actual wasm-bindgen ownership glue in Node, Bun and Deno.
export async function groupCommitConformance(binding) {
  class CountedLog extends MemoryCommitLog { flushes = 0; flush() { this.flushes++; } }
  const primary = new CountedLog(), follower = new CountedLog();
  const log = new DeltaCommitLog(new ReplicatedCommitLog(primary, [follower], { acknowledgements: 2 }));
  const engine = new WasmEngine(binding, { isolation: 'snapshot', history: 'all', constraints: ['references'] }, log);
  const first = engine.begin(), bad = engine.begin(), last = engine.begin();
  try {
    // No await: commitGroup must drain every member's existing request queue.
    const one = first.create(0n, 0n);
    const invalid = bad.create(999n, 0n);
    const two = last.create(1n, 0n);
    const result = engine.commitGroup([first, bad, last]);
    const closed = first.close();
    assert.equal((await one).index, 1n); await invalid; assert.equal((await two).index, 3n);
    assert.deepEqual((await result).map(outcome => outcome.status), ['committed', 'aborted', 'committed']);
    await closed;
    assert.equal(primary.flushes, 1); assert.equal(follower.flushes, 1);
    assert.equal(primary.read(), follower.read());
    assert.equal(engine.snapshot(1n).rows.length, 1);
    assert.equal(engine.snapshot(2n).rows.length, 2);
    const reopened = new WasmEngine(binding, {}, new DeltaCommitLog(follower));
    try { assert.equal(reopened.snapshot().rows.length, 2); }
    finally { reopened.close(); }
    await assert.rejects(first.commit(), /commit group/);
    const other = new WasmEngine(binding);
    const foreign = other.begin();
    const local = engine.begin();
    try {
      await assert.rejects(engine.commitGroup([local, foreign]), TypeError);
      await local.create(0n, 0n); assert.equal(await local.commit(), 3n);
    } finally { await foreign.close(); await local.close(); other.close(); }
  } finally { await first.close(); await bad.close(); await last.close(); engine.close(); }

  for (const phase of ['append', 'flush']) {
    for (const after of [false, true]) {
      const records = []; let appends = 0;
      const adapter = {
        supportsDurableFlush: false, read: () => JSON.stringify(records),
        append(record) {
          const fail = phase === 'append' && ++appends === 2;
          if (fail && !after) throw Error('before append');
          records.push(record);
          if (fail) throw Error('after append');
        },
        flush() { if (phase === 'flush') throw Error('flush failed'); },
      };
      const uncertain = new WasmEngine(binding, { isolation: 'snapshot' }, adapter);
      const a = uncertain.begin(), b = uncertain.begin();
      try {
        await a.create(0n, 0n); await b.create(1n, 0n);
        const outcomes = await uncertain.commitGroup([a, b]);
        assert.ok(outcomes.every(outcome => outcome.status === 'indeterminate' && /reopen/.test(outcome.error)));
        assert.throws(() => uncertain.snapshot(), /indeterminate/);
        const recovered = new WasmEngine(binding, {}, records);
        try { assert.equal(recovered.snapshot().rows.length, phase === 'append' && !after ? 1 : 2); }
        finally { recovered.close(); }
      } finally { await a.close(); await b.close(); uncertain.close(); }
    }
  }
  console.log('ok wasm group commit: separate revisions, one flush per replica, ownership, queue ordering, known aborts and uncertain prefixes');
}
