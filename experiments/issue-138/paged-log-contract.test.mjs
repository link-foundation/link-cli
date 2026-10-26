// Finite contracts authored before the native page provider exists.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { PagedCommitLog } from '../../js/client/src/engine-paged-log.js';

const vectors = JSON.parse(readFileSync(new URL('./native-page-vectors.json', import.meta.url)));
const clone = (value) => structuredClone(value);
const bytes = (text) => new Uint8Array(Buffer.from(text, 'hex'));
const blank = () => ({ metadata: [undefined, undefined], pages: [] });

class ImageDevice {
  supportsDurableFlush = true;
  calls = [];
  fault;
  constructor(image = blank()) { this.live = clone(image); this.durable = clone(image); }
  newDatabaseId() { return bytes(vectors.databaseId); }
  get dataLength() { return this.live.pages.length * 4096; }
  readMetadata(slot) { return this.live.metadata[slot]?.slice(); }
  readPage(index) { return this.live.pages[index]?.slice(); }
  action(name, body) {
    this.calls.push(name);
    const fail = this.fault?.at === this.calls.length;
    if (fail && !this.fault.after) throw new Error('injected ' + name);
    body();
    if (fail) {
      // A complete unsynced metadata write can reach storage before an error.
      if (name.startsWith('writeMetadata')) this.durable.metadata = clone(this.live.metadata);
      throw new Error('injected ' + name);
    }
  }
  writePage(index, page) { this.action('writePage', () => { this.live.pages[index] = page.slice(); }); }
  syncData() { this.action('syncData', () => { this.durable.pages = clone(this.live.pages); }); }
  writeMetadata(slot, value) { this.action('writeMetadata' + slot, () => { this.live.metadata[slot] = value.slice(); }); }
  syncMetadata(slot) { this.action('syncMetadata' + slot, () => { this.durable.metadata[slot] = this.live.metadata[slot].slice(); }); }
  truncateData(length) { this.action('truncateData', () => { this.live.pages.length = length / 4096; }); }
  crash() { return new ImageDevice(this.durable); }
}
const records = (log) => JSON.parse(log.read());
const fixture = (name) => ({ metadata: vectors[name].metadata.map((text) => text === '' ? undefined : bytes(text)), pages: vectors[name].pages.map(bytes) });

test('independent wire vectors and following immutable-page publication', () => {
  const device = new ImageDevice();
  const log = new PagedCommitLog(device);
  assert.deepEqual(device.live, fixture('empty'));
  for (const value of vectors.oneGroup.records) log.append(value);
  assert.deepEqual(records(log), vectors.oneGroup.records);
  log.flush();
  assert.deepEqual(device.live, fixture('oneGroup'));
  log.append(''); log.flush();
  assert.deepEqual(device.live, fixture('twoGroups'));
  for (const name of ['empty', 'oneGroup', 'twoGroups']) {
    const reopened = new PagedCommitLog(new ImageDevice(fixture(name)));
    assert.deepEqual(records(reopened), vectors[name].records);
  }
});

test('one acknowledgement per group; finite failures preserve the acknowledged prefix', () => {
  const baseline = new ImageDevice(fixture('oneGroup'));
  const group = ['雪'.repeat(700), '', 'following\0\ufeff'];
  const probe = new ImageDevice(baseline.durable);
  const success = new PagedCommitLog(probe);
  probe.calls.length = 0;
  for (const value of group) success.append(value);
  success.flush();
  const operations = probe.calls.length;
  assert.ok(operations >= 5 && operations <= 8, probe.calls.join(','));
  for (let at = 1; at <= operations; at++) for (const after of [false, true]) {
    const device = new ImageDevice(baseline.durable);
    const log = new PagedCommitLog(device);
    for (const value of group) log.append(value);
    device.calls.length = 0; device.fault = { at, after };
    assert.throws(() => log.flush(), /injected/);
    for (const operation of [() => log.read(), () => log.append('rejected'), () => log.flush()])
      assert.throws(operation, /reopen|fenced|uncertain/i);
    const reopened = new PagedCommitLog(device.crash());
    const survived = records(reopened);
    assert.ok([baseline.durable.pages.length, probe.durable.pages.length].includes(device.durable.pages.length));
    assert.deepEqual(survived, vectors.oneGroup.records.concat(survived.length === vectors.oneGroup.records.length ? [] : group));
    reopened.append('next'); reopened.flush();
    assert.deepEqual(records(reopened), [...survived, 'next']);
  }
});

test('committed page damage rejects instead of selecting an older acknowledged decision', () => {
  for (const offset of [0, 8, 24, 32, 40, 44, 48, 56, 64, 4095]) {
    const image = fixture('twoGroups'); image.pages[1][offset] ^= 1;
    const before = clone(image);
    assert.throws(() => new PagedCommitLog(new ImageDevice(image)), /page|checksum|corrupt|invalid/i);
    assert.deepEqual(image, before);
  }
});

test('torn latest decision recovers the preceding durable group without rewriting pages', () => {
  for (const length of [0, 1, 8, 16, 32, 64, 119, 120, 127]) {
    const image = fixture('twoGroups'); image.metadata[0] = image.metadata[0].slice(0, length);
    const device = new ImageDevice(image);
    const reopened = new PagedCommitLog(device);
    assert.deepEqual(records(reopened), vectors.oneGroup.records);
    assert.deepEqual(device.live.pages, fixture('oneGroup').pages);
    reopened.append('repair successor'); reopened.flush();
    assert.deepEqual(records(new PagedCommitLog(device.crash())), [...vectors.oneGroup.records, 'repair successor']);
  }
});

test('invalid UTF-16 and record delimiters leave the writer usable; empty flush does no I/O', () => {
  const device = new ImageDevice(); const log = new PagedCommitLog(device);
  const before = device.calls.length;
  log.flush(); assert.equal(device.calls.length, before);
  assert.equal(log.supportsDurableFlush, true);
  for (const value of [null, 4, {}, 'a\nb', '\r', '\ud800', '\udfff']) assert.throws(() => log.append(value), TypeError);
  assert.deepEqual(records(log), []);
  log.append('😀\0'); log.flush();
  assert.deepEqual(records(new PagedCommitLog(device.crash())), ['😀\0']);
  const volatile = new ImageDevice(); volatile.supportsDurableFlush = false;
  assert.equal(new PagedCommitLog(volatile).supportsDurableFlush, false);
});
