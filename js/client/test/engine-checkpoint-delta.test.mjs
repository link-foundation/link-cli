import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { DeltaCommitLog, MemoryCommitLog, ReplicatedCommitLog } from '../src/index.js';

const vectors = JSON.parse(readFileSync(new URL('../../../docs/protocol/engine-checkpoint-vectors.json', import.meta.url), 'utf8'));

test('physical delta accepts independent initial checkpoint vectors', () => {
  for (const records of vectors.valid) {
    assert.deepEqual(JSON.parse(new DeltaCommitLog(new MemoryCommitLog(records)).read()), records);
    const appended = new DeltaCommitLog(new MemoryCommitLog());
    for (const record of records) appended.append(record);
    assert.deepEqual(JSON.parse(appended.read()), records);
  }
});

test('checkpoint compression preserves exact bytes and logical change boundaries', () => {
  const { logical, physical, zeroFollow } = vectors.physicalDelta;
  const raw = new MemoryCommitLog(); const log = new DeltaCommitLog(raw);
  for (const record of logical) log.append(record);
  assert.deepEqual(JSON.parse(raw.read()), physical);
  assert.deepEqual(JSON.parse(log.read()), logical);
  assert.deepEqual(JSON.parse(new DeltaCommitLog(raw, { checkpointInterval: 1 }).read()), logical);
  assert.deepEqual(JSON.parse(new DeltaCommitLog(new MemoryCommitLog(zeroFollow)).read()), zeroFollow);
  for (const records of vectors.physicalDelta.invalid) {
    assert.throws(() => new DeltaCommitLog(new MemoryCommitLog(records)).read());
    const appended = new DeltaCommitLog(new MemoryCommitLog());
    for (const record of records.slice(0, -1)) appended.append(record);
    const before = appended.read();
    assert.throws(() => appended.append(records.at(-1)));
    assert.equal(appended.read(), before);
  }
});

test('checkpoint seed and following delta uncertainty fence until actual prefix reopen', () => {
  const { logical } = vectors.physicalDelta;
  for (const prefix of [0, 1]) {
    for (const mode of ['append-before', 'append-after', 'append-value', 'flush-throw', 'flush-value']) {
      const raw = new MemoryCommitLog(logical.slice(0, prefix)); let calls = 0;
      const inner = {
        read() { calls++; return raw.read(); },
        append(record) {
          calls++; if (mode === 'append-before') throw Error(mode);
          raw.append(record); if (mode === 'append-after') throw Error(mode);
          if (mode === 'append-value') return Promise.resolve();
        },
        flush() { calls++; if (mode === 'flush-throw') throw Error(mode); if (mode === 'flush-value') return false; },
      };
      const log = new DeltaCommitLog(inner);
      if (mode.startsWith('append')) assert.throws(() => log.append(logical[prefix]));
      else { log.append(logical[prefix]); assert.throws(() => log.flush()); }
      const before = calls;
      assert.throws(() => log.read(), /indeterminate/);
      assert.throws(() => log.append(logical[prefix]), /indeterminate/);
      assert.throws(() => log.flush(), /indeterminate/); assert.equal(calls, before);
      const recovered = new DeltaCommitLog(raw); const expected = prefix + (mode === 'append-before' ? 0 : 1);
      assert.deepEqual(JSON.parse(recovered.read()), logical.slice(0, expected));
      for (const record of logical.slice(expected)) recovered.append(record);
      assert.deepEqual(JSON.parse(recovered.read()), logical);
    }
  }
});

test('checkpoint replication accepts both physical composition orders', () => {
  const { logical } = vectors.physicalDelta;
  for (const outside of [false, true]) {
    const primary = new MemoryCommitLog(); const follower = new MemoryCommitLog();
    const replication = new ReplicatedCommitLog(outside ? primary : new DeltaCommitLog(primary, { checkpointInterval: 1 }),
      [outside ? follower : new DeltaCommitLog(follower)], { acknowledgements: 2 });
    const log = outside ? new DeltaCommitLog(replication) : replication;
    for (const record of logical) { log.append(record); log.flush(); }
    assert.equal(replication.synchronize(), 2);
    assert.deepEqual(JSON.parse(new DeltaCommitLog(primary).read()), logical);
    assert.deepEqual(JSON.parse(new DeltaCommitLog(follower).read()), logical);
    if (outside) assert.equal(primary.read(), follower.read());
    else assert.notEqual(primary.read(), follower.read());
  }
});
