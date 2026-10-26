// Finite contracts authored before the native page provider exists.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { PagedCommitLog } from './engine-paged-log.mjs';

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

const fnv = (values) => {
  let result = 14695981039346656037n;
  for (const value of values) result = ((result ^ BigInt(value)) * 1099511628211n) & 0xffffffffffffffffn;
  return result;
};
const data = (value) => new DataView(value.buffer, value.byteOffset, value.byteLength);
const setNumber = (value, offset, number) => data(value).setBigUint64(offset, BigInt(number), true);
const signMetadata = (value) => setNumber(value, 120, fnv(value.subarray(0, 120)));
function signPages(image) {
  let previous = 0n;
  for (const page of image.pages) {
    setNumber(page, 48, previous);
    previous = fnv([...page.subarray(0, 56), ...page.subarray(64)]);
    setNumber(page, 56, previous);
  }
  const slot = image.pages.length === 1 ? 1 : 0;
  setNumber(image.metadata[slot], 72, previous); signMetadata(image.metadata[slot]);
}

test('finite malformed metadata and ambiguous ownership reject without modifying input', () => {
  const mutations = [
    (meta) => { meta[0] ^= 1; },
    (meta) => { data(meta).setUint32(8, 2, true); },
    (meta) => { data(meta).setUint32(12, 1, true); },
    (meta) => { meta[80] = 1; },
    (meta) => setNumber(meta, 32, 3), // generation exceeds page count
    (meta) => setNumber(meta, 40, 2199023255552n), // portable physical limit
    (meta) => setNumber(meta, 48, 253),
    (meta) => setNumber(meta, 48, 0),
    (meta) => setNumber(meta, 56, 7),
    (meta) => setNumber(meta, 56, 0),
    (meta) => setNumber(meta, 64, 0),
    (meta) => setNumber(meta, 64, 253),
    (meta) => setNumber(meta, 32, 1n << 63n),
  ];
  for (const mutate of mutations) {
    const image = fixture('oneGroup'); mutate(image.metadata[1]); signMetadata(image.metadata[1]);
    const device = new ImageDevice(image), before = clone(device.live);
    assert.throws(() => new PagedCommitLog(device)); assert.deepEqual(device.live, before);
  }
  for (const mutate of [
    (image) => { image.metadata[0][16] ^= 1; signMetadata(image.metadata[0]); },
    (image) => { image.metadata[0] = image.metadata[1].slice(); image.metadata[0][16] ^= 1; signMetadata(image.metadata[0]); },
    (image) => { image.metadata = [new Uint8Array(1), undefined]; image.pages = []; },
    (image) => { image.metadata = [undefined, undefined]; },
    (image) => { image.pages = []; },
  ]) {
    const image = fixture('oneGroup'); mutate(image);
    assert.throws(() => new PagedCommitLog(new ImageDevice(image)));
  }
  const sameGeneration = fixture('oneGroup');
  sameGeneration.metadata[0] = sameGeneration.metadata[1].slice();
  new PagedCommitLog(new ImageDevice(sameGeneration)); // Identical duplicate decision is unambiguous.
  setNumber(sameGeneration.metadata[0], 48, 5); signMetadata(sameGeneration.metadata[0]);
  assert.throws(() => new PagedCommitLog(new ImageDevice(sameGeneration)), /conflicting/);
  const invalidEmpty = fixture('empty'); setNumber(invalidEmpty.metadata[0], 72, 1); signMetadata(invalidEmpty.metadata[0]);
  assert.throws(() => new PagedCommitLog(new ImageDevice(invalidEmpty)), /root/);
});

test('self-consistent checksums do not make invalid page structures acceptable', () => {
  const mutations = [
    (page) => { page[0] ^= 1; },
    (page) => { page[8] ^= 1; },
    (page) => setNumber(page, 24, 1),
    (page) => setNumber(page, 32, 0),
    (page) => setNumber(page, 32, 2),
    (page) => { data(page).setUint32(40, 0, true); },
    (page) => { data(page).setUint32(40, 253, true); },
    (page) => { data(page).setUint32(44, 1, true); },
    (page) => { page[4095] = 1; },
    (page) => setNumber(page, 64 + 5 * 16, 0), // Orphaned name cells.
    (page) => setNumber(page, 64 + 5 * 16, 4), // Nonadjacent name root.
    (page) => setNumber(page, 72 + 5 * 16, 1), // Wrong predecessor.
    (page) => setNumber(page, 64 + 4 * 16, 0), // Internal value where a chunk is required.
    (page) => setNumber(page, 64 + 4 * 16, 0x8800000000000000n), // Reserved bits.
    (page) => setNumber(page, 64 + 4 * 16, 0x8000000000000000n), // Zero-length chunk.
    (page) => setNumber(page, 64 + 4 * 16, 0x8100000000000100n), // Nonzero padding.
    (page) => setNumber(page, 64 + 4 * 16, 0x81000000000000ffn), // Invalid UTF-8.
    (page) => setNumber(page, 64 + 4 * 16, 0x810000000000000an), // Record delimiter.
    (page) => setNumber(page, 72 + 4 * 16, 3), // Short chunk with a continuation.
    (page) => setNumber(page, 64 + 5 * 16, 0x8000000000000000n), // Tagged record source.
  ];
  for (const mutate of mutations) {
    const image = fixture('oneGroup'); mutate(image.pages[0]); signPages(image);
    assert.throws(() => new PagedCommitLog(new ImageDevice(image)));
  }
  for (const [offset, value] of [[48, 5], [56, 2], [64, 5]]) {
    const image = fixture('oneGroup'); setNumber(image.metadata[1], offset, value); signMetadata(image.metadata[1]);
    assert.throws(() => new PagedCommitLog(new ImageDevice(image)));
  }
  const wrongChain = fixture('oneGroup'); setNumber(wrongChain.pages[0], 48, 3);
  const page = wrongChain.pages[0]; const checksum = fnv([...page.subarray(0, 56), ...page.subarray(64)]);
  setNumber(page, 56, checksum); setNumber(wrongChain.metadata[1], 72, checksum); signMetadata(wrongChain.metadata[1]);
  assert.throws(() => new PagedCommitLog(new ImageDevice(wrongChain)), /padding/);
});

test('selected finite page limits reject before append and preserve following reads', () => {
  const device = new ImageDevice();
  for (const maxPages of [0, -1, 0.5, Infinity, NaN, '1', 2199023255552])
    assert.throws(() => new PagedCommitLog(device, { maxPages }), RangeError);
  const log = new PagedCommitLog(device, { maxPages: 1 });
  log.append('accepted'); log.flush();
  assert.throws(() => log.append('rejected'), RangeError);
  assert.deepEqual(records(log), ['accepted']);
  assert.throws(() => new PagedCommitLog(new ImageDevice(fixture('twoGroups')), { maxPages: 1 }), /limit/);
  const broken = new ImageDevice(); broken.newDatabaseId = () => new Uint8Array(15);
  assert.throws(() => new PagedCommitLog(broken), /identity/);
  const asynchronous = new ImageDevice(); asynchronous.syncData = () => Promise.resolve();
  assert.throws(() => new PagedCommitLog(asynchronous), TypeError);
  const corruptAfterOpen = new ImageDevice(fixture('oneGroup'));
  const opened = new PagedCommitLog(corruptAfterOpen); corruptAfterOpen.live.pages[0][64] ^= 1;
  assert.throws(() => opened.read(), /checksum/); assert.throws(() => opened.append('rejected'), /fenced/);
});
