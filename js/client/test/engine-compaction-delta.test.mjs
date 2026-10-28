// Physical delta replacement contracts use independently generated wire vectors.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { DeltaCommitLog, MemoryCommitLog } from '../src/index.js';
const fixture = JSON.parse(readFileSync(new URL('../../../docs/protocol/engine-checkpoint-vectors.json', import.meta.url))).physicalDelta;
class ReplacingLog {
  records = []; fault; calls = 0; supportsReplacement = true; supportsDurableFlush = false;
  read() { return JSON.stringify(this.records); }
  append(record) { this.records.push(record); }
  flush() {}
  replace(records) {
    this.calls++;
    if (this.fault === 'before') throw Error('before replacement');
    const changed = JSON.stringify(this.records) !== JSON.stringify(records);
    this.records = [...records];
    if (this.fault === 'after') throw Error('after replacement');
    if (this.fault === 'promise') return Promise.resolve(changed);
    if (this.fault === 'void') return undefined;
    if (this.fault === 'string') return 'true';
    return changed;
  }
}
test('replacement prepares exact fresh delta records and resumes its codec after acknowledgement', () => {
  const inner = new ReplacingLog(); const log = new DeltaCommitLog(inner);
  assert.equal(log.supportsReplacement, true);
  assert.equal(log.replace(fixture.logical.slice(0, 2)), true);
  assert.deepEqual(inner.records, fixture.physical.slice(0, 2));
  assert.equal(log.replace(fixture.logical.slice(0, 2)), false);
  for (const record of fixture.logical.slice(2)) log.append(record);
  log.flush(); assert.deepEqual(inner.records, fixture.physical);
  assert.deepEqual(JSON.parse(log.read()), fixture.logical);
  assert.equal(log.replace([]), true); assert.equal(log.replace([]), false);
  const zero = 'engine3|0|1||4ebe798a0a94e6d5';
  assert.equal(log.replace([zero]), true); assert.deepEqual(JSON.parse(log.read()), [zero]);
});
test('unsupported capabilities and malformed replacement sequences reject before inner I/O and permit reuse', () => {
  for (const value of [undefined, null, false, 1, 'true']) {
    const inner = new ReplacingLog(); inner.supportsReplacement = value;
    const log = new DeltaCommitLog(inner); assert.equal(log.supportsReplacement, false);
    assert.throws(() => log.replace([]), /support/); assert.equal(inner.calls, 0);
    log.append('engine1|1|2|1,0,0,-|4f0139770a69b047'); log.flush();
  }
  const inner = new ReplacingLog(); const log = new DeltaCommitLog(inner);
  for (const records of [null, 'bad', {}, [null], [,], ['bad'], [fixture.logical[0], 'bad'], [fixture.logical[0], fixture.logical[0]]]) {
    assert.throws(() => log.replace(records)); assert.equal(inner.calls, 0);
  }
  assert.equal(log.replace(fixture.logical), true); assert.deepEqual(inner.records, fixture.physical);
  assert.equal(new DeltaCommitLog(new MemoryCommitLog()).supportsReplacement, false);
});
test('unknown replacement outcomes and non-boolean asynchronous acknowledgements fence the codec', () => {
  for (const fault of ['before', 'after', 'promise', 'void', 'string']) {
    const inner = new ReplacingLog(); const log = new DeltaCommitLog(inner);
    log.replace(fixture.logical.slice(0, 1)); inner.fault = fault;
    assert.throws(() => log.replace(fixture.logical));
    const calls = inner.calls;
    for (const operation of [() => log.read(), () => log.append(fixture.logical[1]), () => log.flush(), () => log.replace([])]) assert.throws(operation, /indeterminate/);
    assert.equal(inner.calls, calls);
    inner.fault = undefined;
    const reopened = new DeltaCommitLog(inner);
    assert.deepEqual(JSON.parse(reopened.read()), fault === 'before' ? fixture.logical.slice(0, 1) : fixture.logical);
    assert.equal(reopened.replace(fixture.logical), fault === 'before');
  }
});
