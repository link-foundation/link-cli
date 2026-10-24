import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineClient, WasmEngine, createWasmClient, UnsupportedError, LinksError } from '../src/index.js';

test('transaction controls run after queued requests and failures do not break the queue', async () => {
  const events = [];
  const session = {
    request(text) { events.push(text); return '(count: 0)'; },
    commit() { events.push('commit'); throw new Error('conflict'); },
    rollback() { events.push('rollback'); },
    free() { events.push('free'); },
  };
  const client = new EngineClient(session);
  client.require('atomicTransactions', 'rollback');
  assert.throws(() => client.require('durableFlush'), UnsupportedError);
  const request = client.count();
  const commit = client.commit();
  await request;
  await assert.rejects(commit, LinksError);
  await client.rollback();
  await client.close();
  assert.deepEqual(events, ['(count: ())', 'commit', 'rollback', 'free']);
  await assert.rejects(client.commit(), /closed/);
});

test('ordinary wasm stores cannot be sent transaction controls', async () => {
  assert.throws(() => new EngineClient({ request: () => '' }), TypeError);
  const client = createWasmClient({ request: () => '', free() {} });
  await assert.rejects(client.transport.control('commit'), UnsupportedError);
  await client.close();
});

test('engine wrapper preserves decimal addresses and passes explicit policies', () => {
  let freed = 0;
  class Native {
    constructor(options, journal) {
      assert.equal(options, '{"isolation":"snapshot"}');
      assert.equal(journal, '["record"]');
    }
    journal() { return '["record"]'; }
    snapshot(revision) {
      assert.equal(revision, 10n);
      return '{"revision":"10","rows":[{"index":"4294967295","source":"0","target":"1","name":null}]}';
    }
    begin() { return { request: () => '', commit: () => 1n, rollback() {}, free() {} }; }
    free() { freed++; }
  }
  const engine = new WasmEngine({ StagedEngine: Native }, { isolation: 'snapshot' }, ['record']);
  assert.equal(engine.snapshot('10').rows[0].index, 4294967295n);
  assert.deepEqual(engine.journal(), ['record']);
  assert.ok(engine.begin() instanceof EngineClient);
  engine.close(); engine.close();
  assert.equal(freed, 1);
  assert.throws(() => engine.begin(), /closed/);
  assert.throws(() => new WasmEngine({}), /StagedEngine/);
});

test('custom logs pass through to the binding with explicit capabilities', async () => {
  const log = { supportsDurableFlush: true };
  class Native {
    static withLog(options, adapter) {
      assert.equal(options, '{"requireDurableFlush":true}'); assert.equal(adapter, log);
      return new Native();
    }
    begin() { return { request: () => '', commit: () => 1n, rollback() {}, free() {} }; }
    free() {}
  }
  const engine = new WasmEngine({ StagedEngine: Native }, { requireDurableFlush: true }, log);
  const session = engine.begin(); session.require('durableFlush', 'persistent', 'crashConsistency');
  await session.rollback(); await session.close(); engine[Symbol.dispose]();
  assert.throws(() => engine.snapshot(), /closed/);
  assert.throws(() => engine.journal(), /closed/);
});

test('a failing capability getter cannot leak an allocated native engine', () => {
  let allocated = 0;
  class Native {
    static withLog() { allocated++; return {}; }
    forkWithLog() { allocated++; return {}; }
    resumeWithLog() { allocated++; return {}; }
    free() {}
  }
  const log = { get supportsDurableFlush() { throw new Error('capability unavailable'); } };
  assert.throws(() => new WasmEngine({ StagedEngine: Native }, {}, log), /capability unavailable/);
  const parent = new WasmEngine({ StagedEngine: Native });
  assert.throws(() => parent.fork({}, log), /capability unavailable/);
  assert.throws(() => parent.resumeBranch(log, 'base'), /capability unavailable/);
  parent.close();
  assert.equal(allocated, 0);
});

test('branches pass policies and capabilities, retain ownership and classify uncertain merges', async () => {
  const events = [];
  let failure;
  const log = { supportsDurableFlush: true };
  const session = () => ({ request: () => '', commit: () => 1n, rollback() {}, free() { events.push('session freed'); } });
  const branch = () => ({
    engine() { return { begin: session, free() { events.push('engine freed'); } }; },
    mergeBaseRecord() { return 'base'; },
    merge() { if (failure) throw failure; return 2n; },
    free() { events.push('branch freed'); },
  });
  class Native {
    fork(options, journal) {
      assert.equal(options, '{"history":1}'); assert.equal(journal, '[]'); return branch();
    }
    forkWithLog(options, adapter) {
      assert.equal(options, '{}'); assert.equal(adapter, log); return branch();
    }
    resume(options, journal, base) {
      assert.equal(options, '{}'); assert.equal(journal, '["saved"]'); assert.equal(base, 'base'); return branch();
    }
    resumeWithLog(options, adapter, base) {
      assert.equal(options, '{"requireDurableFlush":true}'); assert.equal(adapter, log); assert.equal(base, 'base'); return branch();
    }
    free() { events.push('parent freed'); }
  }
  const parent = new WasmEngine({ StagedEngine: Native });
  const fork = parent.fork({ history: 1 });
  assert.equal(fork.mergeBaseRecord(), 'base'); assert.equal(fork.merge(), 2n);
  failure = new Error('conflict: overlapping edits');
  assert.throws(() => fork.merge(), (error) => error instanceof LinksError && error.outcome === 'failed' && error.cause === failure);
  failure = new Error('indeterminate commit: flush failed');
  assert.throws(() => fork.merge(), (error) => error instanceof LinksError && error.outcome === 'unknown');
  failure = undefined;
  const privateSession = fork.engine.begin();
  assert.throws(() => privateSession.require('persistent'), UnsupportedError);
  fork.close(); fork.close();
  await privateSession.rollback(); await privateSession.close();
  assert.throws(() => fork.merge(), /closed/); assert.throws(() => fork.mergeBaseRecord(), /closed/);
  const durableFork = parent.fork({}, log);
  const durableSession = durableFork.engine.begin(); durableSession.require('persistent', 'durableFlush');
  await durableSession.close(); durableFork[Symbol.dispose]();
  parent.resumeBranch(['saved'], 'base').close();
  parent.resumeBranch(log, 'base', { requireDurableFlush: true }).close();
  parent.close();
  assert.throws(() => parent.fork(), /closed/); assert.throws(() => parent.resumeBranch([], 'base'), /closed/);
  assert.equal(events.filter((event) => event === 'engine freed').length, 4);
  assert.equal(events.filter((event) => event === 'branch freed').length, 4);
  assert.equal(events.filter((event) => event === 'session freed').length, 2);
  assert.equal(events.at(-1), 'parent freed');
});
