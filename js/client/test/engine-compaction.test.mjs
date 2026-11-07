import test from 'node:test';
import assert from 'node:assert/strict';
import { WasmEngine } from '../src/index.js';
test('compactJournal preserves synchronous native boolean and rejects closed wrapper', () => {
  class Native {
    result = true;
    compactJournal() {
      const result = this.result;
      this.result = false;
      return result;
    }
    free() {}
  }
  const engine = new WasmEngine({ StagedEngine: Native });
  assert.equal(engine.compactJournal(), true);
  assert.equal(engine.compactJournal(), false);
  engine.close();
  assert.throws(() => engine.compactJournal(), /closed/);
});

const MAX = 0xffffffffffffffffn;
/** A native engine that records thresholds and fails when told to. */
class ThresholdNative {
  static calls = [];
  static failure;
  compactJournalAfter(commits) {
    ThresholdNative.calls.push(commits);
    if (commits !== MAX && ThresholdNative.failure) throw ThresholdNative.failure;
    return commits === 1n;
  }
  free() {}
}
function withTimers(run) {
  const { setInterval: start, clearInterval: clear } = globalThis;
  const timers = new Map();
  let next = 1;
  globalThis.setInterval = (pass, period) => {
    timers.set(next, { pass, period });
    return next++;
  };
  globalThis.clearInterval = (timer) => timers.delete(timer);
  ThresholdNative.calls = [];
  ThresholdNative.failure = undefined;
  try {
    run(timers);
  } finally {
    globalThis.setInterval = start;
    globalThis.clearInterval = clear;
  }
}

test('compactJournalAfter passes a checked threshold and rejects a closed wrapper', () => {
  ThresholdNative.calls = [];
  ThresholdNative.failure = undefined;
  const engine = new WasmEngine({ StagedEngine: ThresholdNative });
  assert.equal(engine.compactJournalAfter(1), true);
  assert.equal(engine.compactJournalAfter(2n), false);
  assert.deepEqual(ThresholdNative.calls, [1n, 2n]);
  assert.throws(() => engine.compactJournalAfter(-1), /commits/);
  engine.close();
  assert.throws(() => engine.compactJournalAfter(1), /closed/);
});

test('compactEvery refuses a wrong schedule before starting a timer', () => {
  withTimers((timers) => {
    const engine = new WasmEngine({ StagedEngine: ThresholdNative });
    const refused = (schedule, pattern) =>
      assert.throws(() => engine.compactEvery(schedule), pattern);
    refused({ commits: 0, periodMs: 1 }, /at least one commit/);
    refused({ commits: -1, periodMs: 1 }, /commits/);
    refused({ commits: 1, periodMs: 0 }, /periodMs/);
    refused({ commits: 1, periodMs: Number.NaN }, /periodMs/);
    class Unreplaceable extends ThresholdNative {
      compactJournalAfter() {
        throw new Error('selected log does not support journal replacement');
      }
    }
    const fixed = new WasmEngine({ StagedEngine: Unreplaceable });
    assert.throws(() => fixed.compactEvery({ commits: 1, periodMs: 1 }), /does not support/);
    assert.equal(timers.size, 0);
    engine.close();
    assert.throws(() => engine.compactEvery({ commits: 1, periodMs: 1 }), /closed/);
  });
});

test('compactEvery passes on each tick until stopped', () => {
  withTimers((timers) => {
    const engine = new WasmEngine({ StagedEngine: ThresholdNative });
    const worker = engine.compactEvery({ commits: 2, periodMs: 5 });
    const [{ pass, period }] = timers.values();
    assert.equal(period, 5);
    pass();
    pass();
    assert.deepEqual(ThresholdNative.calls, [MAX, 2n, 2n]);
    worker.stop();
    worker.stop();
    assert.equal(timers.size, 0);
  });
});

test('compactEvery ends at the first failure and stop throws it', () => {
  withTimers((timers) => {
    const engine = new WasmEngine({ StagedEngine: ThresholdNative });
    const worker = engine.compactEvery({ commits: 1, periodMs: 1 });
    const [{ pass }] = timers.values();
    ThresholdNative.failure = new Error('injected failure');
    pass();
    assert.equal(timers.size, 0);
    assert.throws(() => worker.stop(), /injected failure/);
  });
});

test('compactEvery ends quietly with the engine', () => {
  withTimers((timers) => {
    const engine = new WasmEngine({ StagedEngine: ThresholdNative });
    const worker = engine.compactEvery({ commits: 1, periodMs: 1 });
    const [{ pass }] = timers.values();
    engine.close();
    pass();
    assert.equal(timers.size, 0);
    assert.deepEqual(ThresholdNative.calls, [MAX]);
    worker.stop();
  });
});

test('compactEvery does not keep a process alive on a real timer', () => {
  ThresholdNative.failure = undefined;
  const engine = new WasmEngine({ StagedEngine: ThresholdNative });
  engine.compactEvery({ commits: 1, periodMs: 3600000 }).stop();
  engine.close();
});
