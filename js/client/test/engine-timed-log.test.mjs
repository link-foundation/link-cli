import assert from 'node:assert/strict';
import { test } from 'node:test';

import { MemoryCommitLog, revisionAt, TimedCommitLog } from '../src/index.js';

const records = (log) => JSON.parse(log.read());

class Scripted extends MemoryCommitLog {
  failing = false;
  supportsReplacement = true;
  replaced = null;
  read() {
    this.#check();
    return super.read();
  }
  append(record) {
    this.#check();
    super.append(record);
  }
  flush() {
    this.#check();
  }
  replace(list) {
    this.replaced = list;
    return true;
  }
  #check() {
    if (this.failing) throw Error('injected');
  }
}

const filled = (...entries) => {
  const log = new MemoryCommitLog();
  for (const entry of entries) log.append(entry);
  return log;
};

test('timestamps every record without changing it and answers by time', () => {
  let now = 100;
  const [journal, timeline] = [new MemoryCommitLog(), new MemoryCommitLog()];
  const log = new TimedCommitLog(journal, timeline, { clock: () => now });
  log.append('engine1|1|2|p|c');
  now = 200;
  log.append('engine2|2|1|2|p|c');
  log.flush();
  assert.deepEqual(records(journal), ['engine1|1|2|p|c', 'engine2|2|1|2|p|c']);
  assert.equal(log.read(), journal.read());
  assert.deepEqual(records(timeline), ['1|100', '2|200']);
  assert.equal(revisionAt(timeline, 100), 1n);
  assert.equal(revisionAt(timeline, 199), 1n);
  assert.equal(revisionAt(timeline, new Date(200)), 2n);
  assert.throws(() => revisionAt(timeline, 99), /no revision was written/);
  assert.throws(() => revisionAt(new MemoryCommitLog(), 1), /no revision was written/);
});

test('time never runs backwards, even across a reopen', () => {
  let now = 500;
  const [journal, timeline] = [new MemoryCommitLog(), new MemoryCommitLog()];
  const log = new TimedCommitLog(journal, timeline, { clock: () => now });
  log.append('engine1|1|2|p|c');
  now = 400;
  log.append('engine1|2|2|p|c');
  new TimedCommitLog(journal, timeline, { clock: () => now }).append('engine1|3|2|p|c');
  assert.deepEqual(records(timeline), ['1|500', '2|500', '3|500']);
});

test('a revision written again replaces what the lost one claimed', () => {
  const timeline = filled('3|10', '4|20', '5|30', '6|40', '5|70');
  assert.equal(revisionAt(timeline, 20), 4n);
  assert.equal(revisionAt(timeline, 69), 4n);
  assert.equal(revisionAt(timeline, 70), 5n);
});

test('a damaged timeline is refused rather than guessed at', () => {
  for (const entry of ['x|1', '1|x', '1']) {
    const timeline = filled(entry);
    assert.throws(() => revisionAt(timeline, 1), /malformed timeline/);
    assert.throws(() => new TimedCommitLog(new MemoryCommitLog(), timeline), /malformed timeline/);
  }
});

test('a record without a revision reaches neither log', () => {
  for (const record of ['other|1|2', 'engine1', 'engine1|x|2']) {
    const [journal, timeline] = [new MemoryCommitLog(), new MemoryCommitLog()];
    const log = new TimedCommitLog(journal, timeline);
    assert.throws(() => log.append(record), /without a revision/);
    assert.deepEqual(records(journal), []);
    assert.deepEqual(records(timeline), []);
  }
});

test('the system clock is the default', () => {
  const timeline = new MemoryCommitLog();
  new TimedCommitLog(new MemoryCommitLog(), timeline).append('engine1|1|2|p|c');
  assert.ok(Number(records(timeline)[0].split('|')[1]) > 1_600_000_000_000);
});

test('a failing journal or timeline fails the call, and the record lands first', () => {
  const [journal, timeline] = [new Scripted(), new Scripted()];
  const log = new TimedCommitLog(journal, timeline, { clock: () => 1 });
  timeline.failing = true;
  assert.throws(() => log.append('engine1|1|2|p|c'), /injected/);
  assert.equal(records(journal).length, 1);
  assert.throws(() => log.flush(), /injected/);
  timeline.failing = false;
  journal.failing = true;
  assert.throws(() => log.append('engine1|2|2|p|c'), /injected/);
  assert.throws(() => log.flush(), /injected/);
  assert.throws(() => log.read(), /injected/);
  journal.failing = false;
  assert.deepEqual(records(timeline), []);
});

test('durability needs both logs', () => {
  const durable = { supportsDurableFlush: true, read: () => '[]' };
  const volatile = { supportsDurableFlush: false, read: () => '[]' };
  assert.equal(new TimedCommitLog(durable, durable).supportsDurableFlush, true);
  assert.equal(new TimedCommitLog(durable, volatile).supportsDurableFlush, false);
  assert.equal(new TimedCommitLog(volatile, durable).supportsDurableFlush, false);
});

test('replacement goes to a log that can do it and leaves the timeline', () => {
  const [journal, timeline] = [new Scripted(), new MemoryCommitLog()];
  const log = new TimedCommitLog(journal, timeline);
  assert.equal(log.supportsReplacement, true);
  assert.equal(log.replace(['engine3|1|2|p|c']), true);
  assert.deepEqual(journal.replaced, ['engine3|1|2|p|c']);
  assert.deepEqual(records(timeline), []);

  const fixed = new TimedCommitLog(new MemoryCommitLog(), new MemoryCommitLog());
  assert.equal(fixed.supportsReplacement, false);
  assert.throws(() => fixed.replace([]), /does not support journal replacement/);
});
