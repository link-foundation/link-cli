// Finite independent durable images; a root write may persist before its reply.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { CheckpointPagedCommitLog } from '../src/engine-checkpoint-pages.js';
import { MirroredPageDevice } from '../src/engine-mirrored-pages.js';

const vectors = JSON.parse(readFileSync(new URL('../../../docs/protocol/checkpoint-pages-v1.json', import.meta.url)));
const clone = (value) => structuredClone(value);
const blank = () => ({ metadata: [undefined, undefined], pages: [] });
class Device {
  supportsDurableFlush = true;
  constructor(trace, name, image = blank()) { this.trace = trace; this.name = name; this.live = clone(image); this.durable = clone(image); }
  action(operation, body) {
    const name = this.name + '.' + operation;
    this.trace.calls.push(name);
    const fault = this.trace.fault?.at === this.trace.calls.length;
    if (fault && !this.trace.fault.after) throw new Error('injected ' + name);
    const value = body();
    if (fault) {
      if (operation.startsWith('writeMetadata')) this.durable.metadata = clone(this.live.metadata);
      throw new Error('injected ' + name);
    }
    return value;
  }
  newDatabaseId() { return this.action('newDatabaseId', () => new Uint8Array(Buffer.from(vectors.identity, 'hex'))); }
  get dataLength() { return this.action('dataLength', () => this.live.pages.length * 4096); }
  readMetadata(slot) { return this.action('readMetadata' + slot, () => this.live.metadata[slot]?.slice()); }
  writeMetadata(slot, bytes) { return this.action('writeMetadata' + slot, () => { this.live.metadata[slot] = bytes.slice(); }); }
  syncMetadata(slot) { return this.action('syncMetadata' + slot, () => { this.durable.metadata[slot] = this.live.metadata[slot]?.slice(); }); }
  readPage(index) { return this.action('readPage', () => this.live.pages[index].slice()); }
  writePage(index, bytes) { return this.action('writePage', () => { assert.equal(index, this.live.pages.length); this.live.pages.push(bytes.slice()); }); }
  syncData() { return this.action('syncData', () => { this.durable.pages = clone(this.live.pages); }); }
  truncateData(length) { return this.action('truncateData', () => { this.live.pages.length = length / 4096; }); }
  repairPage(index, bytes) { return this.action('repairPage', () => { this.live.pages[index] = bytes.slice(); }); }
  completeRecovery() { return this.action('completeRecovery', () => {}); }
}
const makeDevices = (images = [blank(), blank(), blank()]) => {
  const trace = { calls: [] };
  const devices = images.map((image, i) => new Device(trace, 'device' + i, image));
  return { trace, devices };
};
const open = (devices, options = { maxPages: 4 }) => new CheckpointPagedCommitLog(devices[0], devices.slice(1), options);
const setup = (images) => {
  const { trace, devices } = makeDevices(images);
  const log = new CheckpointPagedCommitLog(devices[0], devices.slice(1), { maxPages: 4 });
  return { trace, devices, log };
};
const durable = (devices) => devices.map((device) => clone(device.durable));
const records = (log) => JSON.parse(log.read());

test('checkpoint pages use independent root vectors, repeated swaps and exact following groups', () => {
  const { log, devices } = setup();
  assert.equal(Buffer.from(devices[0].live.metadata[0]).toString('hex'), vectors.decisions['0']);
  log.append('old'); log.flush();
  for (let epoch = 1; epoch <= 2; epoch++) {
    const group = ['雪'.repeat(700), '\ufeff\0😀 ' + epoch, ''];
    assert.equal(log.replace(group), true);
    assert.deepEqual(records(log), group);
    assert.equal(Buffer.from(devices[0].live.metadata[epoch % 2]).toString('hex'), vectors.decisions[String(epoch)]);
    const reopened = setup(durable(devices));
    assert.deepEqual(records(reopened.log), group);
    log.append('following'); log.flush();
    assert.deepEqual(records(log), [...group, 'following']);
  }
});

test('every finite replacement I/O failure fences and reopens a complete durable old or new journal', () => {
  const start = setup(); start.log.append('acknowledged'); start.log.flush();
  const images = durable(start.devices), replacement = ['雪'.repeat(700), '\ufeff\0😀', ''];
  const probe = setup(images); probe.trace.calls.length = 0; probe.log.replace(replacement);
  const operations = probe.trace.calls.length;
  assert.ok(operations >= 12 && operations <= 40, probe.trace.calls);
  for (let at = 1; at <= operations; at++) for (const after of [false, true]) {
    const { log, devices, trace } = setup(images);
    trace.calls.length = 0; trace.fault = { at, after };
    assert.throws(() => log.replace(replacement), /injected/);
    const count = trace.calls.length;
    for (const operation of [() => log.read(), () => log.append('rejected'), () => log.flush(), () => log.replace(['rejected'])])
      assert.throws(operation, /fenced|reopen|uncertain/);
    assert.equal(trace.calls.length, count);
    const reopened = setup(durable(devices));
    const found = records(reopened.log);
    assert.ok(JSON.stringify(found) === JSON.stringify(['acknowledged']) || JSON.stringify(found) === JSON.stringify(replacement));
    reopened.log.append('next'); reopened.log.flush();
    assert.deepEqual(records(setup(durable(reopened.devices)).log), [...found, 'next']);
  }
});

test('invalid and pending replacement inputs leave committed journals usable; identical replacement writes nothing', () => {
  const { log, trace } = setup();
  log.append('old'); log.flush();
  trace.calls.length = 0;
  assert.equal(log.replace(['old']), false);
  assert.ok(trace.calls.every((call) => call.endsWith('.readPage') || call.endsWith('.completeRecovery')));
  for (const invalid of [null, 'not an array', [4], ['\ud800'], ['a\nb'], ['\r'], ['x'.repeat(10000)]])
    assert.throws(() => log.replace(invalid));
  assert.deepEqual(records(log), ['old']);
  log.append('pending'); assert.throws(() => log.replace(['new']), /pending/);
  log.flush(); assert.deepEqual(records(log), ['old', 'pending']);
  assert.equal(log.replace([]), true); assert.deepEqual(records(log), []);
  assert.equal(log.supportsDurableFlush, true);
  assert.equal(log.supportsReplacement, true);
});

const unhex = (text) => new Uint8Array(Buffer.from(text, 'hex'));
test('malformed roots, stale selected banks and unrooted journals never trigger recovery writes', () => {
  const start = setup(); start.log.append('old'); start.log.flush(); start.log.replace(['new']);
  const source = durable(start.devices), invalid = [];
  const add = (change) => { const image = clone(source); change(image); invalid.push(image); };
  for (const decision of vectors.invalid) add((image) => { image[0].metadata[0] = unhex(decision); });
  for (const decision of [vectors.decisions['3'], vectors.foreignDecision]) add((image) => { image[0].metadata[1] = unhex(decision); });
  add((image) => { image[0].metadata.reverse(); });
  add((image) => { image[0].pages.push(new Uint8Array(4096)); });
  add((image) => { image[2].metadata = [undefined, undefined]; });
  add((image) => { image[2].metadata = clone(image[1].metadata); });
  add((image) => { image[2].pages[0][56] ^= 1; });
  add((image) => { image[0].metadata = [undefined, undefined]; });
  add((image) => { image[0].metadata = [undefined, undefined]; image[1].pages = []; image[2].pages = []; });
  add((image) => { image[0].metadata[0] = 'invalid root bytes'; });
  for (const image of invalid) {
    const { devices, trace } = makeDevices(image);
    assert.throws(() => open(devices));
    assert.ok(trace.calls.every((call) => call.includes('.read') || call.endsWith('.dataLength')), trace.calls);
    assert.deepEqual(devices.map((device) => device.live), image);
  }
});

test('all torn root lengths and damaged checksums recover the prior acknowledgement', () => {
  const start = setup(); start.log.append('old'); start.log.flush(); start.log.replace(['new']);
  const source = durable(start.devices);
  for (let length = 0; length < 128; length++) {
    const image = clone(source); image[0].metadata[1] = image[0].metadata[1].slice(0, length);
    const { log } = setup(image); assert.equal(log.epoch, 0n); assert.deepEqual(records(log), ['old']);
  }
  source[0].metadata[1][120] ^= 1; assert.deepEqual(records(setup(source).log), ['old']);
});

test('exact u64 epochs including exhaustion reject replacement before any mutation', () => {
  for (const epoch of [9007199254740993n, 0xffffffffffffffffn]) {
    const image = [blank(), blank(), blank()];
    image[0].metadata[1] = unhex(vectors.decisions[String(epoch)]);
    image[2].metadata = [unhex(vectors.emptyBanks[String(epoch)]), undefined];
    const { log, devices, trace } = setup(image); assert.equal(log.epoch, epoch);
    assert.equal(log.replace([]), false); trace.calls.length = 0;
    if (epoch === 0xffffffffffffffffn) {
      assert.throws(() => log.replace(['new']), /exhausted/);
      assert.ok(trace.calls.every((call) => call.endsWith('.completeRecovery')));
      log.append('following'); log.flush();
    } else { assert.equal(log.replace(['new']), true); assert.equal(log.epoch, epoch + 1n); }
    assert.deepEqual(records(setup(durable(devices)).log), [epoch === 0xffffffffffffffffn ? 'following' : 'new']);
  }
});

test('invalid device modes, limits and identities are rejected; durability requires all devices', () => {
  const { devices } = makeDevices();
  for (const banks of [undefined, [], [devices[1]], [devices[1], devices[1]], [devices[0], devices[1]], [null, devices[2]]])
    assert.throws(() => new CheckpointPagedCommitLog(devices[0], banks), /three distinct/);
  assert.throws(() => new CheckpointPagedCommitLog(null, devices.slice(1)), /three distinct/);
  for (const maxPages of [0, -1, 0.5, NaN, Infinity, Number.MAX_SAFE_INTEGER, '4', null])
    assert.throws(() => open(devices, { maxPages }), /limit/);
  for (const flags of [[1, 0, 0], [0, 2, 2], [0, 0, 1]]) {
    const { devices, trace } = makeDevices(); devices.forEach((device, i) => { device.metadataFlags = flags[i]; });
    assert.throws(() => open(devices), /modes/); assert.deepEqual(trace.calls, []);
  }
  for (const identity of [null, new Uint8Array(15), Array(16).fill(0)]) {
    const { devices } = makeDevices(); devices[0].newDatabaseId = () => identity;
    assert.throws(() => open(devices), /identity length/);
  }
  for (let disabled = 0; disabled < 3; disabled++) {
    const { devices } = makeDevices(); devices[disabled].supportsDurableFlush = false;
    assert.equal(open(devices).supportsDurableFlush, false);
  }
  // Explicit single-copy flags exercise the same bytes as omitted flags.
  const explicit = makeDevices(); explicit.devices.forEach((device) => { device.metadataFlags = 0; });
  assert.equal(open(explicit.devices).supportsDurableFlush, true);
});

test('every finite initialization failure reopens empty and accepts a following group', () => {
  const probe = setup(), operations = probe.trace.calls.length;
  assert.ok(operations >= 20 && operations <= 40, probe.trace.calls);
  for (let at = 1; at <= operations; at++) for (const after of [false, true]) {
    const { devices, trace } = makeDevices(); trace.fault = { at, after };
    assert.throws(() => open(devices), /injected/);
    const recovered = setup(durable(devices)); assert.deepEqual(records(recovered.log), []);
    recovered.log.append('following'); recovered.log.flush();
    assert.deepEqual(records(setup(durable(recovered.devices)).log), ['following']);
  }
});

test('non-void device results fence replacement; ordinary read and flush faults also fence', () => {
  for (const operation of ['truncateData', 'syncData', 'writeMetadata', 'syncMetadata']) {
    const { log, devices } = setup(); log.append('old'); log.flush();
    devices[2][operation] = () => Promise.resolve();
    assert.throws(() => log.replace(['new']), /synchronous/);
    assert.throws(() => log.replace(['again']), /fenced/);
    assert.deepEqual(records(setup(durable(devices)).log), ['old']);
  }
  for (const operation of ['read', 'flush']) {
    const { log, trace } = setup(); log.append('old'); log.flush();
    if (operation === 'flush') log.append('pending');
    trace.calls.length = 0; trace.fault = { at: 1, after: false };
    assert.throws(() => log[operation](), /injected/);
    assert.throws(() => log.read(), /fenced/);
  }
});

test('mirrored banks repair only the selected epoch then accept repeated swaps', () => {
  const initial = makeDevices(Array.from({ length: 5 }, blank));
  const openMirrored = (devices) => new CheckpointPagedCommitLog(devices[0], [
    new MirroredPageDevice(devices[1], devices[2]), new MirroredPageDevice(devices[3], devices[4]),
  ]);
  const log = openMirrored(initial.devices); log.append('old'); log.flush(); log.replace(['new']);
  assert.equal(Buffer.from(initial.devices[0].live.metadata[1]).toString('hex'), vectors.mirroredDecision);
  const images = durable(initial.devices); images[3].pages[0][56] ^= 1; images[3].metadata[0] = new Uint8Array(17);
  const { devices } = makeDevices(images), reopened = openMirrored(devices);
  assert.deepEqual(records(reopened), ['new']); assert.deepEqual(devices[3].durable, devices[4].durable);
  assert.equal(reopened.replace(['third']), true); assert.equal(reopened.replace(['fourth']), true);
  assert.deepEqual(records(reopened), ['fourth']);
  assert.equal(reopened.supportsDurableFlush, true);
});

test('interrupted mirrored replacements recover old or new records and can reuse the partial inactive bank', () => {
  const openMirrored = (devices) => new CheckpointPagedCommitLog(devices[0], [
    new MirroredPageDevice(devices[1], devices[2]), new MirroredPageDevice(devices[3], devices[4]),
  ], { maxPages: 4 });
  const start = makeDevices(Array.from({ length: 5 }, blank));
  const original = openMirrored(start.devices); original.append('old'); original.flush();
  const initial = durable(start.devices), replacement = ['雪'.repeat(700), '\ufeff\0😀'];
  const probe = makeDevices(initial), success = openMirrored(probe.devices); probe.trace.calls.length = 0;
  success.replace(replacement); const operations = probe.trace.calls.length;
  assert.ok(operations >= 30 && operations <= 90, probe.trace.calls);
  for (let at = 1; at <= operations; at++) for (const after of [false, true]) {
    const { devices, trace } = makeDevices(initial), log = openMirrored(devices); trace.calls.length = 0; trace.fault = { at, after };
    try { log.replace(replacement); } catch (error) {
      assert.match(error.message, /injected/); const count = trace.calls.length;
      assert.throws(() => log.read(), /fenced/); assert.throws(() => log.replace(['rejected']), /fenced/); assert.equal(trace.calls.length, count);
    }
    const recovered = makeDevices(durable(devices)), reopened = openMirrored(recovered.devices), found = records(reopened);
    assert.ok(JSON.stringify(found) === JSON.stringify(['old']) || JSON.stringify(found) === JSON.stringify(replacement));
    reopened.replace(['after recovery']);
    assert.deepEqual(records(openMirrored(makeDevices(durable(recovered.devices)).devices)), ['after recovery']);
  }
});
