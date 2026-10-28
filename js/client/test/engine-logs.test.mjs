import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineClient, MemoryCommitLog, ReplicatedCommitLog } from '../src/index.js';

class FaultLog extends MemoryCommitLog {
  failRead = false; failAppend = false; failFlush = false;
  read() { if (this.failRead) throw Error('read partition'); return super.read(); }
  append(record) { if (this.failAppend) throw Error('append partition'); super.append(record); }
  flush() { if (this.failFlush) throw Error('flush partition'); }
}

test('async catch-up is idempotent and rejects divergent prefixes', () => {
  const primary = new MemoryCommitLog(['a', 'b']);
  const follower = new MemoryCommitLog(['a']);
  const trace = [];
  const log = new ReplicatedCommitLog(primary, [follower], { trace: (message) => trace.push(message) });
  log.append('c'); log.flush();
  assert.equal(follower.read(), '["a"]');
  assert.equal(log.synchronize(), 2);
  assert.equal(log.synchronize(), 2);
  assert.equal(follower.read(), primary.read());
  follower.append('divergence');
  assert.equal(log.synchronize(), 1);
  assert.match(trace[0], /diverges/);
  assert.equal(log.supportsDurableFlush, false);
});

test('synchronous acknowledgements survive fault recovery without duplicate records', () => {
  for (const fault of ['failRead', 'failAppend', 'failFlush']) {
    const follower = new FaultLog(); follower[fault] = true;
    const primary = new MemoryCommitLog();
    const log = new ReplicatedCommitLog(primary, [follower], { acknowledgements: 2 });
    log.append('a');
    assert.throws(() => log.flush(), /indeterminate/);
    follower[fault] = false;
    log.flush();
    assert.equal(follower.read(), '["a"]');
  }
});

test('durability capabilities and membership validation are explicit', () => {
  const memory = new MemoryCommitLog();
  const durable = { read: () => '[]', append() {}, flush() {}, supportsDurableFlush: true };
  for (const acknowledgements of [0, 3, 1.5, NaN])
    assert.throws(() => new ReplicatedCommitLog(memory, [new MemoryCommitLog()], { acknowledgements }), RangeError);
  assert.throws(() => new ReplicatedCommitLog(memory, [memory]), /distinct/);
  assert.throws(() => new ReplicatedCommitLog(durable, [memory, memory]), /distinct/);
  assert.equal(new ReplicatedCommitLog(durable, []).supportsDurableFlush, true);
  assert.equal(new ReplicatedCommitLog(durable, [memory], { acknowledgements: 1 }).supportsDurableFlush, true);
  assert.equal(new ReplicatedCommitLog(durable, [memory], { acknowledgements: 2 }).supportsDurableFlush, false);
  assert.equal(new ReplicatedCommitLog(durable, [{ ...durable }], { acknowledgements: 2 }).supportsDurableFlush, true);
});

test('malformed records and asynchronous adapters cannot count as acknowledgements', () => {
  for (const invalid of ['{}', '[1]', 'bad json']) {
    const log = new ReplicatedCommitLog({ read: () => invalid, flush() {} }, []);
    assert.throws(() => log.synchronize());
  }
  const primary = new MemoryCommitLog(['a']);
  for (const adapter of [
    { read: () => '[]', append: () => Promise.resolve(), flush() {} },
    { read: () => '[]', append() {}, flush: () => 1 },
    { read: () => '["different"]', append() {}, flush() {} },
  ]) {
    const log = new ReplicatedCommitLog(primary, [adapter], { acknowledgements: 2 });
    assert.throws(() => log.flush(), /indeterminate/);
  }
  assert.throws(() => new ReplicatedCommitLog({ append: () => 1 }, []).append('a'), /synchronously/);
});

test('commit I/O errors are marked unknown while conflicts are failed', async () => {
  for (const [message, outcome] of [['indeterminate commit: log unavailable', 'unknown'], ['conflict: changed', 'failed']]) {
    const client = new EngineClient({ request: () => '', commit() { throw message; }, rollback() {}, free() {} });
    await assert.rejects(client.commit(), (error) => error.outcome === outcome);
    await client.close();
  }
});

test('diagnostic callback failures cannot change replication results', () => {
  const log = new ReplicatedCommitLog(new MemoryCommitLog(), [{ read() { throw Error('partition'); } }], {
    trace() { throw Error('diagnostic failure'); },
  });
  assert.equal(log.synchronize(), 1);
});

test('replicated reads return the authoritative primary journal before catch-up', () => {
  const primary = new MemoryCommitLog(['committed']);
  const log = new ReplicatedCommitLog(primary, [new MemoryCommitLog()]);
  assert.equal(log.read(), '["committed"]');
});

test('synchronous replication shares one primary flush and explicit catch-up still flushes', () => {
  class CountedLog extends MemoryCommitLog { flushes = 0; flush() { this.flushes++; } }
  const primary = new CountedLog(), follower = new CountedLog();
  const log = new ReplicatedCommitLog(primary, [follower], { acknowledgements: 2 });
  log.append('first'); log.append('second'); log.flush();
  assert.equal(primary.flushes, 1); assert.equal(follower.flushes, 1);
  assert.equal(follower.read(), primary.read());
  assert.equal(log.synchronize(), 2);
  assert.equal(primary.flushes, 2); assert.equal(follower.flushes, 2);
  assert.equal(follower.read(), '["first","second"]');
});
