import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { DeltaCommitLog, MemoryCommitLog, ReplicatedCommitLog } from '../src/index.js';

const vectors = JSON.parse(readFileSync(new URL('../../../docs/protocol/engine-delta-vectors.json', import.meta.url), 'utf8'));
const [sequence] = vectors.validSequences;

test('mixed logical change records pass through and reset complete-snapshot delta bases', () => {
  const fixture = JSON.parse(readFileSync(new URL('../../../docs/protocol/engine-change-vectors.json', import.meta.url), 'utf8')).physicalMixed;
  const raw = new MemoryCommitLog(); const log = new DeltaCommitLog(raw);
  for (const record of fixture.logical) log.append(record);
  assert.deepEqual(JSON.parse(raw.read()), fixture.physical);
  assert.deepEqual(JSON.parse(log.read()), fixture.logical);
});

test('invalid mixed-format bases reject before append and during physical replay', () => {
  const fixture = JSON.parse(readFileSync(new URL('../../../docs/protocol/engine-change-vectors.json', import.meta.url), 'utf8'));
  for (const { case: label, records } of fixture.invalidPhysical) {
    const raw = new MemoryCommitLog(); const log = new DeltaCommitLog(raw);
    for (const record of records.slice(0, -1)) log.append(record);
    const before = raw.read();
    assert.throws(() => log.append(records.at(-1)), undefined, label);
    assert.equal(raw.read(), before);
    assert.throws(() => new DeltaCommitLog(new MemoryCommitLog(records)).read(), undefined, label);
  }
});

test('delta journals replay shared vectors and reject every complete corrupt sequence', () => {
  for (const { physical, logical } of vectors.validSequences) {
    const log = new DeltaCommitLog(new MemoryCommitLog(physical));
    assert.deepEqual(JSON.parse(log.read()), logical);
  }
  for (const { name, records } of vectors.invalidSequences) {
    const log = new DeltaCommitLog(new MemoryCommitLog(records));
    assert.throws(() => log.read(), undefined, name);
  }
});

test('localized edits use portable deltas and checkpoints bound the dependency chain', () => {
  const physical = new MemoryCommitLog();
  const log = new DeltaCommitLog(physical);
  for (const record of sequence.logical) { log.append(record); log.flush(); }
  assert.deepEqual(JSON.parse(physical.read()), sequence.physical);
  assert.deepEqual(JSON.parse(log.read()), sequence.logical);
  assert.equal(log.supportsDurableFlush, false);
  assert.ok(sequence.physical[1].length < sequence.logical[1].length / 8);
  const periodic = new MemoryCommitLog();
  const bounded = new DeltaCommitLog(periodic, { checkpointInterval: 2 });
  for (const record of sequence.logical) bounded.append(record);
  assert.match(JSON.parse(periodic.read())[1], /^delta1\|/);
  assert.match(JSON.parse(periodic.read())[2], /^engine1\|/);
  const fullOnly = new MemoryCommitLog();
  const plain = new DeltaCommitLog(fullOnly, { checkpointInterval: 1 });
  for (const record of sequence.logical) plain.append(record);
  assert.deepEqual(JSON.parse(fullOnly.read()), sequence.logical);
  const resumed = new DeltaCommitLog(new MemoryCommitLog(sequence.physical.slice(0, 2)));
  resumed.append(sequence.logical[2]);
  assert.deepEqual(JSON.parse(resumed.read()), sequence.logical);
  const small = vectors.validSequences.at(-1);
  const fallback = new MemoryCommitLog(); const tiny = new DeltaCommitLog(fallback);
  for (const record of small.logical) tiny.append(record);
  assert.deepEqual(JSON.parse(fallback.read()), small.physical);
});

test('configuration and logical record validation happen before mutation', () => {
  let reads = 0; let writes = 0;
  const inner = { read() { reads++; return '[]'; }, append() { writes++; }, flush() {} };
  for (const checkpointInterval of [0, -1, 1.5, NaN, Infinity, 2 ** 32, '2', null])
    assert.throws(() => new DeltaCommitLog(inner, { checkpointInterval }));
  assert.equal(reads, 0);
  const log = new DeltaCommitLog(inner, { checkpointInterval: 2 ** 32 - 1 });
  for (const record of [null, 12, '', 'bad', ...vectors.invalidSequences.flatMap(({ records }) => records.length === 1 ? records : [])])
    assert.throws(() => log.append(record));
  assert.equal(writes, 0);
  log.append(sequence.logical[0]);
  assert.equal(writes, 1);
  assert.throws(() => log.append(sequence.logical[0]), /revision/);
  assert.equal(writes, 1);
});

test('malformed inner reads fail without changing the cached base', () => {
  let text = 'invalid'; let writes = 0;
  const inner = { read: () => text, append() { writes++; }, flush() {} };
  const log = new DeltaCommitLog(inner);
  for (text of ['invalid', 'null', '{}', '[null]', '[1]', '[{}]']) assert.throws(() => log.read());
  text = JSON.stringify(sequence.physical);
  assert.deepEqual(JSON.parse(log.read()), sequence.logical);
  const unavailable = new DeltaCommitLog({ ...inner, read() { throw Error('read unavailable'); } });
  assert.throws(() => unavailable.append(sequence.logical[0]), /unavailable/);
  text = JSON.stringify(sequence.physical.slice(0, 2)); log.read();
  text = '{}'; assert.throws(() => log.read());
  assert.throws(() => log.append(sequence.logical[2]), /JSON array/);
  assert.equal(writes, 0, 'a failed reread must invalidate cached admission to append');
  text = JSON.stringify(sequence.physical.slice(0, 2));
  log.append(sequence.logical[2]); assert.equal(writes, 1);
});

test('append and flush uncertainty fence the decorator until a fresh reopen', () => {
  for (const mode of ['append-before', 'append-after', 'append-value', 'flush-throw', 'flush-value']) {
    const raw = new MemoryCommitLog();
    let calls = 0;
    const inner = {
      read: () => raw.read(),
      append(record) {
        calls++;
        if (mode === 'append-before') throw Error(mode);
        raw.append(record);
        if (mode === 'append-after') throw Error(mode);
        if (mode === 'append-value') return Promise.resolve();
      },
      flush() {
        calls++;
        if (mode === 'flush-throw') throw Error(mode);
        if (mode === 'flush-value') return false;
      },
    };
    const log = new DeltaCommitLog(inner);
    if (mode.startsWith('append')) assert.throws(() => log.append(sequence.logical[0]));
    else { log.append(sequence.logical[0]); assert.throws(() => log.flush()); }
    const before = calls;
    assert.throws(() => log.read(), /indeterminate/);
    assert.throws(() => log.append(sequence.logical[0]), /indeterminate/);
    assert.throws(() => log.flush(), /indeterminate/);
    assert.equal(calls, before);
    const recovered = new DeltaCommitLog(raw);
    assert.equal(JSON.parse(recovered.read()).length, mode === 'append-before' ? 0 : 1);
  }
  for (const value of [false, undefined, 'true', true]) {
    const log = new DeltaCommitLog({ supportsDurableFlush: value });
    assert.equal(log.supportsDurableFlush, value === true);
  }
});

test('replication compares logical records or copies physical deltas in either composition order', () => {
  for (const compressOutside of [false, true]) {
    const primary = new MemoryCommitLog(); const follower = new MemoryCommitLog();
    const replication = new ReplicatedCommitLog(
      compressOutside ? primary : new DeltaCommitLog(primary, { checkpointInterval: 1 }),
      [compressOutside ? follower : new DeltaCommitLog(follower)], { acknowledgements: 2 });
    const log = compressOutside ? new DeltaCommitLog(replication) : replication;
    for (const record of sequence.logical) { log.append(record); log.flush(); }
    assert.equal(replication.synchronize(), 2);
    assert.deepEqual(JSON.parse(new DeltaCommitLog(primary).read()), sequence.logical);
    assert.deepEqual(JSON.parse(new DeltaCommitLog(follower).read()), sequence.logical);
    if (compressOutside) assert.deepEqual(primary.read(), follower.read());
    else assert.notDeepEqual(primary.read(), follower.read());
  }
});
