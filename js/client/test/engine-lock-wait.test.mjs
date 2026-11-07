import assert from 'node:assert/strict';
import { test } from 'node:test';
import { AbortError, DeadlineExceededError, LinksError, UnsupportedError } from '../src/errors.js';
import { EngineClient, WasmEngine } from '../src/index.js';

/** A small model of the Rust session: one write per request, `set <address>`. */
function fakeBinding() {
  const owners = new Map();
  let tokens = 0n;
  class Session {
    owner = (tokens += 1n);
    writes = [];
    blocker;
    savepoints = new Map();
    closed = false;
    writeOwner() {
      return this.owner;
    }
    takeWriteBlocker() {
      const blocker = this.blocker;
      this.blocker = undefined;
      return blocker;
    }
    savepoint() {
      if (this.closed) throw new Error('transaction is closed');
      const token = BigInt(this.savepoints.size + 1);
      this.savepoints.set(token, [...this.writes]);
      return token;
    }
    rollbackTo(token) {
      this.writes = [...this.savepoints.get(token)];
    }
    releaseSavepoint(token) {
      this.savepoints.delete(token);
    }
    request(text) {
      binding.hook?.();
      if (this.closed) return "(error: 'transaction is closed')";
      for (const address of text.match(/\d+/g) ?? []) {
        const owner = owners.get(address);
        if (owner !== undefined && owner !== this.owner) {
          this.blocker = owner;
          return "(error: 'busy: another transaction owns this write address')";
        }
        owners.set(address, this.owner);
        this.writes.push(address);
      }
      return `(count: (${this.writes.length}))`;
    }
    end() {
      this.closed = true;
      for (const [address, owner] of owners) if (owner === this.owner) owners.delete(address);
    }
    commit() {
      this.end();
      return 1n;
    }
    rollback() {
      this.end();
    }
    free() {
      this.end();
    }
  }
  const binding = {
    StagedEngine: class {
      constructor(options) {
        binding.coreOptions = JSON.parse(options);
      }
      begin() {
        return new Session();
      }
      group() {
        const members = [];
        return {
          add: (handle) => members.push(handle),
          commit() {
            for (const member of members) member.commit();
            return JSON.stringify(members.map(() => ({ status: 'committed', revision: '1' })));
          },
          free() {},
        };
      }
      fork(options) {
        const engine = new this.constructor(options);
        return { engine: () => engine, free() {} };
      }
      resume(options) {
        return this.fork(options);
      }
      free() {}
    },
  };
  return binding;
}

const pessimistic = { concurrency: 'pessimistic' };
const open = (lockWaitMs, binding = fakeBinding()) =>
  new WasmEngine(binding, { ...pessimistic, lockWaitMs });
const write = (session, address, scope) =>
  session.request([{ id: 'set', values: [String(address)] }], scope);
const tick = (ms = 5) => new Promise((resolve) => setTimeout(resolve, ms));

test('a request waits for the owner to roll back and then runs', async () => {
  const engine = open(60_000);
  const owner = engine.begin();
  const waiter = engine.begin();
  await write(owner, 1);
  let finished = false;
  const waiting = write(waiter, 1).then(() => (finished = true));
  await tick();
  assert.equal(finished, false);
  assert.equal(waiter.transport.stats.requests, 1);
  await owner.rollback();
  await waiting;
  assert.equal(finished, true);
});

for (const end of ['commit', 'close']) {
  test(`${end} wakes a waiter`, async () => {
    const engine = open(60_000);
    const owner = engine.begin();
    const waiter = engine.begin();
    await write(owner, 1);
    const waiting = write(waiter, 1);
    await tick();
    await owner[end]();
    await waiting;
  });
}

test('a commit group wakes waiters', async () => {
  const engine = open(60_000);
  const owner = engine.begin();
  const waiter = engine.begin();
  await write(owner, 1);
  const waiting = write(waiter, 1);
  await tick();
  assert.equal((await engine.commitGroup([owner]))[0].status, 'committed');
  await waiting;
});

test('a wait ends at its limit with the same busy answer and undoes the request', async () => {
  const engine = open(10);
  const owner = engine.begin();
  const waiter = engine.begin();
  await write(owner, 1);
  await assert.rejects(write(waiter, 1), /busy/);
  await assert.rejects(write(waiter, '2 1'), /busy/);
  await owner.rollback();
  await write(waiter, 3);
});

test('a zero limit answers at once', async () => {
  const engine = open(0);
  const owner = engine.begin();
  await write(owner, 1);
  await assert.rejects(write(engine.begin(), 1), /busy/);
});

test('the request that closes a cycle is refused and the other continues', async () => {
  const engine = open(60_000);
  const [a, b, c] = [engine.begin(), engine.begin(), engine.begin()];
  await write(a, 1);
  await write(b, 2);
  await write(c, 3);
  const first = write(a, 2);
  await tick();
  const second = write(b, 3);
  await tick();
  await assert.rejects(
    write(c, 1),
    (error) =>
      error instanceof LinksError && /deadlock/.test(error.message) && error.outcome === 'failed',
  );
  await c.rollback();
  await second;
  await b.rollback();
  await first;
});

test('a wait ends with the scope of its request', async () => {
  const engine = open(60_000);
  const owner = engine.begin();
  const waiter = engine.begin();
  await write(owner, 1);
  const controller = new AbortController();
  const waiting = write(waiter, 1, { signal: controller.signal });
  await tick();
  controller.abort();
  await assert.rejects(
    waiting,
    (error) => error instanceof AbortError && error.outcome === 'not-sent',
  );
  await assert.rejects(write(waiter, 1, { timeout: 5 }), DeadlineExceededError);
  // The owner's end wakes nobody now, and later work on the same session proceeds.
  await owner.rollback();
  await write(waiter, 1);
});

test('a scope that ended while the request ran does not start a wait', async () => {
  const binding = fakeBinding();
  const engine = open(60_000, binding);
  const owner = engine.begin();
  const waiter = engine.begin();
  await write(owner, 1);
  const controller = new AbortController();
  binding.hook = () => controller.abort();
  await assert.rejects(write(waiter, 1, { signal: controller.signal }), AbortError);
});

test('a closed session answers without waiting', async () => {
  const engine = open(60_000);
  const session = engine.begin();
  await session.commit();
  await assert.rejects(write(session, 1), /closed/);
});

test('options are checked and never reach the core', async () => {
  const binding = fakeBinding();
  const engine = open(1, binding);
  assert.throws(() => new WasmEngine(binding, { lockWaitMs: 1 }), /requires pessimistic/);
  for (const lockWaitMs of [-1, Number.NaN, Infinity, '5']) {
    assert.throws(() => new WasmEngine(binding, { ...pessimistic, lockWaitMs }), /non-negative/);
  }
  assert.deepEqual(binding.coreOptions, pessimistic);
  const branch = engine.fork({ ...pessimistic, lockWaitMs: 60_000 });
  assert.deepEqual(binding.coreOptions, pessimistic);
  const owner = branch.engine.begin();
  const waiter = branch.engine.begin();
  await write(owner, 1);
  const waiting = write(waiter, 1);
  await tick();
  await owner.rollback();
  await waiting;
  engine.resumeBranch([], 'record', { ...pessimistic, lockWaitMs: 1 });
});

test('options that are not an object reach the core unchanged so it can refuse them', () => {
  const binding = fakeBinding();
  for (const options of [[], ['optimistic']]) {
    new WasmEngine(binding, options);
    assert.deepEqual(binding.coreOptions, options);
  }
});

test('sessions built before lock waiting refuse it', () => {
  assert.throws(
    () => new EngineClient({ commit() {}, rollback() {}, request() {} }, { lockWaits: {} }),
    UnsupportedError,
  );
});
