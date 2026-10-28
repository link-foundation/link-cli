// Independent wire bytes and finite two-device persistence/repair contracts.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { PagedCommitLog } from '../src/engine-paged-log.js';
import { MirroredPageDevice } from '../src/engine-mirrored-pages.js';

const wire = JSON.parse(readFileSync(new URL('../../../docs/protocol/native-pages-mirrored-v1.json', import.meta.url)));
const clone = (value) => structuredClone(value);
const fromHex = (text) => new Uint8Array(Buffer.from(text, 'hex'));
const fixture = (name) => ({ metadata: wire[name].metadata.map((hex) => hex ? fromHex(hex) : undefined), pages: wire[name].pages.map(fromHex) });
const blank = () => ({ metadata: [undefined, undefined], pages: [] });
const records = (log) => JSON.parse(log.read());
const fnv = (bytes) => bytes.reduce((hash, byte) => BigInt.asUintN(64, (hash ^ BigInt(byte)) * 1099511628211n), 14695981039346656037n);
const view = (bytes) => new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
const signMeta = (bytes) => view(bytes).setBigUint64(120, fnv(bytes.subarray(0, 120)), true);
const signPage = (page) => view(page).setBigUint64(56, fnv([...page.subarray(0, 56), ...page.subarray(64)]), true);

class Device {
  supportsDurableFlush = true;
  constructor(side, trace, image = blank()) { this.side = side; this.trace = trace; this.live = clone(image); this.durable = clone(image); }
  get dataLength() { return this.live.pages.length * 4096; }
  newDatabaseId() { return fromHex(wire.databaseId); }
  readMetadata(slot) { if (this.metaError) throw new Error('injected metadata read'); return this.live.metadata[slot]?.slice(); }
  readPage(index) { if (this.readError) throw new Error('injected page read'); return this.live.pages[index]?.slice(); }
  action(name, body) {
    this.trace.calls.push(`${this.side}:${name}`);
    const fails = this.trace.fault?.at === this.trace.calls.length;
    if (fails && !this.trace.fault.after) throw new Error('injected ' + name);
    body();
    if (fails) {
      if (name.startsWith('writeMetadata')) this.durable.metadata = clone(this.live.metadata);
      throw new Error('injected ' + name);
    }
  }
  writePage(index, bytes) { this.action('writePage', () => { assert.equal(index, this.live.pages.length); this.live.pages.push(bytes.slice()); }); }
  repairPage(index, bytes) { this.action('repairPage', () => { assert.ok(index <= this.live.pages.length); this.live.pages[index] = bytes.slice(); }); }
  syncData() { this.action('syncData', () => { this.durable.pages = clone(this.live.pages); }); }
  writeMetadata(slot, bytes) { this.action('writeMetadata' + slot, () => { this.live.metadata[slot] = bytes.slice(); }); }
  syncMetadata(slot) { this.action('syncMetadata' + slot, () => { this.durable.metadata[slot] = this.live.metadata[slot]?.slice(); }); }
  truncateData(length) { this.action('truncateData', () => { this.live.pages.length = length / 4096; }); }
}
function pair(images = [blank(), blank()]) {
  const trace = { calls: [] };
  const devices = images.map((image, side) => new Device(side, trace, image));
  return { devices, trace, open: () => new PagedCommitLog(new MirroredPageDevice(...devices)) };
}
const crashed = (p) => pair(p.devices.map((device) => device.durable));
function damage(image, instruction) {
  if (instruction.kind === 'page') image.pages[instruction.page][instruction.offset] ^= 1;
  if (instruction.kind === 'metadata') image.metadata[instruction.slot] = image.metadata[instruction.slot].slice(0, instruction.length);
  if (instruction.kind === 'truncated') image.pages.length = instruction.pages;
  if (instruction.kind === 'stale') { image.pages[instruction.page][64] ^= 1; signPage(image.pages[instruction.page]); }
}

test('mirrored decisions match independent bytes and repair each finite single-copy fault', () => {
  const initial = pair(); const log = initial.open();
  assert.equal(log.supportsDurableFlush, true);
  for (const device of initial.devices) assert.deepEqual(device.live, fixture('empty'));
  for (const record of wire.oneGroup.records) log.append(record);
  log.flush(); for (const device of initial.devices) assert.deepEqual(device.live, fixture('oneGroup'));
  log.append(''); log.flush(); for (const device of initial.devices) assert.deepEqual(device.live, fixture('twoGroups'));
  assert.throws(() => new PagedCommitLog(initial.devices[0]), /flags|mode/);
  for (const instruction of wire.singleCopyFaults) {
    const images = [fixture('twoGroups'), fixture('twoGroups')]; damage(images[instruction.side], instruction);
    const recovered = pair(images); const opened = recovered.open(); assert.deepEqual(records(opened), wire.twoGroups.records);
    for (const device of recovered.devices) { assert.deepEqual(device.live, fixture('twoGroups')); assert.deepEqual(device.durable, fixture('twoGroups')); }
    const before = recovered.trace.calls.length; recovered.open(); assert.equal(recovered.trace.calls.length, before);
    opened.append('following 雪\0'); opened.flush();
    assert.deepEqual(records(crashed(recovered).open()), [...wire.twoGroups.records, 'following 雪\0']);
  }
});

test('every finite mirrored write/sync failure preserves an entire acknowledged prefix', () => {
  const images = [fixture('oneGroup'), fixture('oneGroup')], group = ['雪'.repeat(700), '', 'following'];
  const probe = pair(images), successful = probe.open(); probe.trace.calls.length = 0;
  for (const record of group) successful.append(record); successful.flush();
  const count = probe.trace.calls.length; assert.ok(count >= 10 && count <= 16);
  for (let at = 1; at <= count; at++) for (const after of [false, true]) {
    const p = pair(images), log = p.open(); for (const record of group) log.append(record);
    p.trace.calls.length = 0; p.trace.fault = { at, after }; assert.throws(() => log.flush(), /injected/);
    for (const run of [() => log.read(), () => log.append('retry'), () => log.flush()]) assert.throws(run, /fenced|uncertain|reopen/);
    const reopened = crashed(p), next = reopened.open(), survived = records(next);
    assert.deepEqual(survived, [...wire.oneGroup.records, ...(survived.length > wire.oneGroup.records.length ? group : [])]);
    if (at === count && after) assert.equal(survived.length, wire.oneGroup.records.length + group.length);
    next.append('next'); next.flush(); assert.deepEqual(records(crashed(reopened).open()), [...survived, 'next']);
  }
});

test('every finite repair failure retains a valid source and permits another recovery', () => {
  const images = [fixture('twoGroups'), fixture('twoGroups')];
  images[0].pages[0][64] ^= 1; images[1].pages[1][4095] ^= 1; images[0].metadata[0][120] ^= 1;
  const probe = pair(images); probe.open(); const count = probe.trace.calls.length; assert.ok(count >= 6 && count <= 10);
  for (let at = 1; at <= count; at++) for (const after of [false, true]) {
    const p = pair(images); p.trace.fault = { at, after }; assert.throws(() => p.open(), /injected/);
    const reopened = crashed(p), log = reopened.open(); assert.deepEqual(records(log), wire.twoGroups.records);
    for (const device of reopened.devices) assert.deepEqual(device.durable, fixture('twoGroups'));
    log.append('after repair'); log.flush(); assert.deepEqual(records(crashed(reopened).open()), [...wire.twoGroups.records, 'after repair']);
  }
});

test('both-copy damage and canonical errors reject before any repair or tail truncation', () => {
  const images = [fixture('twoGroups'), fixture('twoGroups')];
  for (const image of images) image.pages[0][64] ^= 1;
  const damaged = pair(images); assert.throws(() => damaged.open(), /page|copy|checksum/); assert.deepEqual(damaged.trace.calls, []);
  const invalid = [fixture('oneGroup'), fixture('oneGroup')];
  for (const image of invalid) {
    view(image.pages[0]).setBigUint64(128, 0x81000000000000ffn, true); signPage(image.pages[0]);
    view(image.metadata[1]).setBigUint64(72, view(image.pages[0]).getBigUint64(56, true), true); signMeta(image.metadata[1]);
  }
  invalid[0].pages[0][4095] ^= 1; const canonical = pair(invalid);
  assert.throws(() => canonical.open(), /UTF|encoded|invalid/i); assert.deepEqual(canonical.trace.calls, []);
  const foreign = [fixture('oneGroup'), fixture('oneGroup')]; foreign[0].metadata[1][16] ^= 1; signMeta(foreign[0].metadata[1]);
  assert.throws(() => pair(foreign).open(), /conflict|identity/);
  const diverged = [fixture('oneGroup'), fixture('oneGroup')]; view(diverged[0].metadata[1]).setBigUint64(48, 5n, true); signMeta(diverged[0].metadata[1]);
  assert.throws(() => pair(diverged).open(), /conflict/);
});

test('mirroring costs are selected and only new groups are written', () => {
  for (const history of [0, 1, 32]) {
    const p = pair(), log = p.open();
    for (let i = 0; i < history; i++) { log.append('prior'); log.flush(); }
    const prior = p.devices.map((device) => clone(device.live.pages)); p.trace.calls.length = 0;
    log.flush(); assert.deepEqual(p.trace.calls, []); log.append('new 雪'); log.flush();
    assert.deepEqual(p.trace.calls, ['0:writePage', '1:writePage', '0:syncData', '1:syncData',
      `0:writeMetadata${(history + 1) % 2}`, `1:writeMetadata${(history + 1) % 2}`, `0:syncMetadata${(history + 1) % 2}`, `1:syncMetadata${(history + 1) % 2}`]);
    p.devices.forEach((device, side) => assert.deepEqual(device.live.pages.slice(0, history), prior[side]));
  }
});

test('device validation, degraded reads and stale decisions have explicit outcomes', () => {
  const p = pair(), [a, b] = p.devices;
  for (const devices of [[a, a], [undefined, b], [a, undefined], [a, {}]]) assert.throws(() => new MirroredPageDevice(...devices), TypeError);
  a.supportsDurableFlush = false; assert.equal(new MirroredPageDevice(a, b).supportsDurableFlush, false);
  a.supportsDurableFlush = true; b.supportsDurableFlush = false; assert.equal(new MirroredPageDevice(a, b).supportsDurableFlush, false);
  const mode = pair(); mode.devices[0].metadataFlags = 2; assert.throws(() => new PagedCommitLog(mode.devices[0]), /mode/);
  for (const length of [-1, 0.5, NaN, Number.MAX_SAFE_INTEGER]) {
    const bad = pair(); Object.defineProperty(bad.devices[0], 'dataLength', { value: length });
    assert.throws(() => new MirroredPageDevice(...bad.devices).dataLength, /length/);
  }
  assert.throws(() => new MirroredPageDevice(a, b).readPage(0), /expected.*checksum/);
  for (const failure of ['metaError', 'readError']) {
    const degraded = pair([fixture('twoGroups'), fixture('twoGroups')]); degraded.devices[0][failure] = true;
    assert.deepEqual(records(degraded.open()), wire.twoGroups.records);
  }
  const unreadable = pair(); for (const device of unreadable.devices) device.metaError = true;
  assert.throws(() => unreadable.open(), /metadata/);
  const stale = [fixture('oneGroup'), fixture('oneGroup')]; stale[1].metadata[1] = fixture('empty').metadata[0];
  const newer = pair(stale); assert.deepEqual(records(newer.open()), wire.oneGroup.records);
  assert.deepEqual(newer.devices[1].durable, fixture('oneGroup'));
  const unsupported = [fixture('oneGroup'), fixture('oneGroup')];
  view(unsupported[0].metadata[1]).setUint32(12, 2, true); signMeta(unsupported[0].metadata[1]);
  assert.throws(() => pair(unsupported).open(), /flags/);
});

test('changed repair sources and concurrent buffer mutation reject without copying damage', () => {
  const images = [fixture('twoGroups'), fixture('twoGroups')]; images[0].pages[1][64] ^= 1;
  const p = pair(images), read = p.devices[1].readPage.bind(p.devices[1]); let calls = 0;
  p.devices[1].readPage = (index) => { const page = read(index); if (index === 1 && ++calls === 2) page[64] ^= 1; return page; };
  assert.throws(() => p.open(), /source changed/); assert.deepEqual(p.trace.calls, []);
  const concurrent = pair([fixture('twoGroups'), fixture('twoGroups')]);
  const first = concurrent.devices[0].readPage.bind(concurrent.devices[0]); let earlier;
  concurrent.devices[0].readPage = (index) => (earlier = first(index));
  const second = concurrent.devices[1].readPage.bind(concurrent.devices[1]);
  concurrent.devices[1].readPage = (index) => {
    const bytes = second(index), buffer = bytes.buffer;
    Object.defineProperty(bytes, 'buffer', { get() { earlier[64] ^= 1; return buffer; } }); return bytes;
  };
  assert.throws(() => concurrent.open(), /conflicting page/); assert.deepEqual(concurrent.trace.calls, []);
});

test('unpublished tails and synchronous I/O failures fence the selected device', () => {
  const images = [fixture('twoGroups'), fixture('twoGroups')]; images[0].pages.push(new Uint8Array(4096));
  const probe = pair(images); probe.open(); assert.deepEqual(probe.trace.calls, ['0:truncateData', '1:truncateData', '0:syncData', '1:syncData']);
  for (let at = 1; at <= 4; at++) for (const after of [false, true]) {
    const p = pair(images), device = new MirroredPageDevice(...p.devices); p.trace.fault = { at, after };
    assert.throws(() => new PagedCommitLog(device), /injected/);
    for (const run of [() => device.readMetadata(0), () => device.readCheckedPage(0, 0n), () => device.newDatabaseId(), () => device.dataLength, () => device.completeRecovery()])
      assert.throws(run, /fenced/);
    assert.deepEqual(records(crashed(p).open()), wire.twoGroups.records);
  }
  const asyncWrite = pair(); asyncWrite.devices[0].syncData = () => Promise.resolve();
  assert.throws(() => asyncWrite.open(), /synchronously/);
  const readFailure = pair([fixture('twoGroups'), fixture('twoGroups')]), log = readFailure.open();
  for (const device of readFailure.devices) device.readError = true;
  assert.throws(() => log.read(), /checksum/); assert.throws(() => log.flush(), /fenced/);
});
