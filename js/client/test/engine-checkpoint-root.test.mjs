import test from 'node:test';
import assert from 'node:assert/strict';
import { CheckpointPagedCommitLog } from '../src/engine-checkpoint-pages.js';
import { MirroredCheckpointRoot } from '../src/engine-checkpoint-root.js';
import { blank, makeDevices, durable, records } from './support/checkpoint-pages.mjs';

test('mirrored root recovers acknowledged epoch after one copy damage', () => {
  const { devices } = makeDevices(Array.from({ length: 4 }, blank));
  let root = new MirroredCheckpointRoot(devices[0], devices[3], 0);
  let log = new CheckpointPagedCommitLog(root, devices.slice(1, 3), { maxPages: 4 });
  log.append('old'); log.flush(); log.replace(['acknowledged']);
  const images = durable(devices); images[0].metadata[1][120] ^= 1;
  const reopened = makeDevices(images).devices;
  root = new MirroredCheckpointRoot(reopened[0], reopened[3], 0);
  log = new CheckpointPagedCommitLog(root, reopened.slice(1, 3), { maxPages: 4 });
  assert.equal(log.epoch, 1n); assert.deepEqual(records(log), ['acknowledged']);
  assert.deepEqual(durable(reopened)[0].metadata, durable(reopened)[3].metadata);
});

import { readFileSync } from 'node:fs';
import { MirroredPageDevice } from '../src/engine-mirrored-pages.js';
import { clone } from './support/checkpoint-pages.mjs';
const vectors = JSON.parse(readFileSync(new URL('../../../docs/protocol/checkpoint-root-mirror-v1.json', import.meta.url)));
const decoded = (key) => new Uint8Array(Buffer.from(vectors[key], 'hex'));
function rootSetup(images = Array.from({ length: 6 }, blank), bankFlags = 0) {
  const { devices, trace } = makeDevices(images);
  const root = new MirroredCheckpointRoot(devices[0], devices[5], bankFlags);
  const banks = bankFlags ? [new MirroredPageDevice(devices[1], devices[3]), new MirroredPageDevice(devices[2], devices[4])] : devices.slice(1, 3);
  return { log: new CheckpointPagedCommitLog(root, banks, { maxPages: 4 }), devices, trace, root };
}
function assertFenced(root, trace) {
  const before = trace.calls.length;
  for (const operation of [() => root.newDatabaseId(), () => root.dataLength, () => root.readMetadata(0), () => root.writeMetadata(0, new Uint8Array(128)),
    () => root.syncMetadata(0), () => root.syncData(), () => root.truncateData(0), () => root.readPage(0), () => root.writePage(0, new Uint8Array(4096)), () => root.completeRecovery()])
    assert.throws(operation, /fenced/);
  assert.equal(trace.calls.length, before);
}
function assertUnchanged(devices, images) { assert.deepEqual(devices.map((d) => d.live), images); }

test('independent root-mode vectors preserve full u64 epochs and caller-owned bytes', () => {
  for (const vector of vectors.decisions) {
    const { devices, trace } = makeDevices([blank(), blank()]);
    const bytes = new Uint8Array(Buffer.from(vector.bytes, 'hex'));
    devices[0].live.metadata[vector.slot] = bytes.slice(); devices[1].live.metadata[vector.slot] = bytes.slice();
    const root = new MirroredCheckpointRoot(...devices, vector.bankFlags);
    assert.equal(root.metadataFlags, 2 | vector.bankFlags); assert.equal(root.supportsDurableFlush, true);
    const selected = root.readMetadata(vector.slot); assert.deepEqual(selected, bytes); selected[0] ^= 1;
    assert.deepEqual(root.readMetadata(vector.slot), bytes);
    trace.calls.length = 0; root.completeRecovery(); assert.deepEqual(trace.calls, []);
    devices[1].supportsDurableFlush = false; assert.equal(root.supportsDurableFlush, false);
  }
});

test('every single-root byte tear preserves the latest acknowledged root and following groups', () => {
  for (const bankFlags of [0, 1]) {
    const start = rootSetup(undefined, bankFlags); start.log.append('old'); start.log.flush(); start.log.replace(['acknowledged']);
    const source = durable(start.devices);
    for (const side of [0, 5]) for (let byte = 0; byte < 128; byte++) {
      const image = clone(source); image[side].metadata[1][byte] ^= 1;
      const recovered = rootSetup(image, bankFlags);
      assert.equal(recovered.log.epoch, 1n); assert.deepEqual(records(recovered.log), ['acknowledged']);
      assert.deepEqual(durable(recovered.devices)[0].metadata, durable(recovered.devices)[5].metadata);
    }
    for (let epoch = 2; epoch <= 4; epoch++) {
      assert.equal(start.log.replace(['acknowledged' + epoch]), true);
      const image = durable(start.devices); image[epoch % 2 ? 0 : 5].metadata[epoch % 2] = undefined;
      const recovered = rootSetup(image, bankFlags); assert.equal(recovered.log.epoch, BigInt(epoch));
      recovered.log.append('following'); recovered.log.flush(); recovered.log.replace(['next', 'following']);
      assert.deepEqual(records(rootSetup(durable(recovered.devices), bankFlags).log), ['next', 'following']);
    }
  }
});

test('invalid roots, contradictory copies and invalid selected banks never trigger root repair', () => {
  for (const bankFlags of [0, 1]) {
    const start = rootSetup(undefined, bankFlags); start.log.append('old'); start.log.flush(); start.log.replace(['acknowledged']);
    const source = durable(start.devices), invalid = [];
    const add = (change) => { const image = clone(source); change(image); invalid.push(image); };
    add((i) => { i[0].metadata[1][120] ^= 1; i[5].metadata[1][120] ^= 1; });
    add((i) => { i[5].pages.push(new Uint8Array(4096)); });
    const vector = (epoch) => new Uint8Array(Buffer.from(vectors.decisions.find((v) => v.bankFlags === bankFlags && v.epoch === String(epoch)).bytes, 'hex'));
    add((i) => { i[0].metadata[0] = vector(4); });
    add((i) => { i[0].metadata[0] = vector(4); i[5].metadata[0] = vector(4); });
    if (bankFlags === 0) for (const key of ['foreign', 'wrongMode', 'unknownMode', 'unknownVersion', 'nonzeroPadding']) add((i) => { i[0].metadata[1] = decoded(key); });
    add((i) => { i[0].metadata[1][120] ^= 1; i[2].pages[0][56] ^= 1; if (bankFlags) i[4].pages[0][56] ^= 1; });
    add((i) => { i[0].metadata[1][120] ^= 1; i[2].metadata = [undefined, undefined]; if (bankFlags) i[4].metadata = [undefined, undefined]; });
    for (const images of invalid) {
      const { devices, trace } = makeDevices(images);
      const root = new MirroredCheckpointRoot(devices[0], devices[5], bankFlags);
      const banks = bankFlags ? [new MirroredPageDevice(devices[1], devices[3]), new MirroredPageDevice(devices[2], devices[4])] : devices.slice(1, 3);
      assert.throws(() => new CheckpointPagedCommitLog(root, banks, { maxPages: 4 }));
      assert.ok(trace.calls.every((call) => !/write|sync|truncate|repair/.test(call)), trace.calls);
      assertUnchanged(devices, images);
    }
  }
});

test('repair rechecks all selected sources before any destination write and fences on change', () => {
  const start = rootSetup(); start.log.replace(['acknowledged']); const images = durable(start.devices);
  images[0].metadata[0][120] ^= 1; images[5].metadata[1][120] ^= 1;
  const { devices, trace } = makeDevices(images), root = new MirroredCheckpointRoot(devices[0], devices[5], 0);
  root.readMetadata(0); root.readMetadata(1); trace.calls.length = 0;
  devices[0].live.metadata[1][120] ^= 1;
  assert.throws(() => root.completeRecovery(), /source changed/);
  assert.ok(trace.calls.every((call) => call.includes('readMetadata'))); assertFenced(root, trace);
});

test('both publication copies synchronize before acknowledgment at every finite I/O cut', () => {
  for (const bankFlags of [0, 1]) {
    const start = rootSetup(undefined, bankFlags); start.log.append('old'); start.log.flush(); const images = durable(start.devices);
    const replacement = ['雪'.repeat(700), '\ufeff\0😀', ''];
    const success = rootSetup(images, bankFlags); success.trace.calls.length = 0; success.log.replace(replacement);
    const operations = success.trace.calls.length; assert.ok(operations >= 18 && operations <= 80);
    assert.deepEqual(success.trace.calls.slice(-4), ['device0.writeMetadata1', 'device5.writeMetadata1', 'device0.syncMetadata1', 'device5.syncMetadata1']);
    for (let at = 1; at <= operations; at++) for (const after of [false, true]) {
      const current = rootSetup(images, bankFlags); current.trace.calls.length = 0; current.trace.fault = { at, after };
      let acknowledged = false;
      try { acknowledged = current.log.replace(replacement); } catch (error) {
        assert.match(error.message, /injected/);
        for (const action of [() => current.log.read(), () => current.log.append('bad'), () => current.log.flush(), () => current.log.replace(['bad'])]) assert.throws(action, /fenced/);
      }
      const recovered = rootSetup(durable(current.devices), bankFlags), found = records(recovered.log);
      if (acknowledged) assert.deepEqual(found, replacement);
      else assert.ok(JSON.stringify(found) === JSON.stringify(['old']) || JSON.stringify(found) === JSON.stringify(replacement));
      recovered.log.append('following'); recovered.log.flush(); recovered.log.replace([...found, 'following', 'compacted']);
      assert.deepEqual(records(rootSetup(durable(recovered.devices), bankFlags).log), [...found, 'following', 'compacted']);
    }
  }
});

test('every finite root-repair interruption preserves the acknowledged epoch and subsequent reuse', () => {
  for (const bankFlags of [0, 1]) for (const side of [0, 5]) {
    const start = rootSetup(undefined, bankFlags); start.log.append('old'); start.log.flush(); start.log.replace(['acknowledged']);
    const images = durable(start.devices); images[side].metadata[1][120] ^= 1;
    const success = rootSetup(images, bankFlags), operations = success.trace.calls.length;
    assert.ok(operations >= 12 && operations <= 80);
    for (let at = 1; at <= operations; at++) for (const after of [false, true]) {
      const attempt = makeDevices(images); attempt.trace.fault = { at, after };
      const root = new MirroredCheckpointRoot(attempt.devices[0], attempt.devices[5], bankFlags);
      const banks = bankFlags ? [new MirroredPageDevice(attempt.devices[1], attempt.devices[3]), new MirroredPageDevice(attempt.devices[2], attempt.devices[4])] : attempt.devices.slice(1, 3);
      try { const log = new CheckpointPagedCommitLog(root, banks, { maxPages: 4 }); assert.deepEqual(records(log), ['acknowledged']); }
      catch (error) { assert.match(error.message, /injected|unreadable/); }
      const recovered = rootSetup(durable(attempt.devices), bankFlags);
      assert.equal(recovered.log.epoch, 1n); assert.deepEqual(records(recovered.log), ['acknowledged']);
      recovered.log.append('following'); recovered.log.flush(); recovered.log.replace(['reused', 'following']);
      assert.deepEqual(records(rootSetup(durable(recovered.devices), bankFlags).log), ['reused', 'following']);
    }
  }
});

test('root selection rejects wrong bank modes and both root-mode conversions before mutation', () => {
  for (const bankFlags of [0, 1]) {
    const start = rootSetup(undefined, bankFlags); start.log.replace(['acknowledged']); const images = durable(start.devices);
    const { devices, trace } = makeDevices(images);
    const banks = bankFlags ? [new MirroredPageDevice(devices[1], devices[3]), new MirroredPageDevice(devices[2], devices[4])] : devices.slice(1, 3);
    assert.throws(() => new CheckpointPagedCommitLog(devices[0], banks), /flags/);
    const wrongRoot = new MirroredCheckpointRoot(devices[0], devices[5], 1 - bankFlags);
    assert.throws(() => new CheckpointPagedCommitLog(wrongRoot, banks), /flags/);
    assertUnchanged(devices, images); assert.ok(trace.calls.every((call) => !/write|sync|truncate|repair/.test(call)));
  }
  const original = makeDevices(); const plain = new CheckpointPagedCommitLog(original.devices[0], original.devices.slice(1)); plain.replace(['old']);
  const images = [...durable(original.devices), blank()]; const { devices } = makeDevices(images);
  const root = new MirroredCheckpointRoot(devices[0], devices[3], 0);
  assert.throws(() => new CheckpointPagedCommitLog(root, devices.slice(1, 3)), /flags/); assertUnchanged(devices, images);
});

test('raw root contract rejects invalid owners, slots, publications and data without mutating', () => {
  const { devices, trace } = makeDevices([blank(), blank()]);
  for (const args of [[null, devices[1], 0], [devices[0], null, 0], [devices[0], devices[0], 0], [devices[0], devices[1], -1], [devices[0], devices[1], undefined], [devices[0], devices[1], 2]]) assert.throws(() => new MirroredCheckpointRoot(...args), TypeError);
  devices[1].metadataFlags = 1; assert.throws(() => new MirroredCheckpointRoot(...devices, 0)); delete devices[1].metadataFlags;
  const root = new MirroredCheckpointRoot(...devices, 0); assert.deepEqual(root.newDatabaseId(), Uint8Array.from({ length: 16 }, (_, i) => i));
  trace.calls.length = 0;
  for (const slot of [-1, 2, null, '0', 0.5]) for (const action of [() => root.readMetadata(slot), () => root.writeMetadata(slot, new Uint8Array(128)), () => root.syncMetadata(slot)]) assert.throws(action, RangeError);
  for (const bytes of [undefined, null, [], new Uint8Array(128)]) assert.throws(() => root.writeMetadata(0, bytes));
  assert.throws(() => root.readPage(0)); assert.throws(() => root.writePage(0, new Uint8Array(4096))); assert.throws(() => root.truncateData(1)); assert.deepEqual(trace.calls, []);
  root.truncateData(0); assert.deepEqual(trace.calls, ['device0.truncateData', 'device1.truncateData']);
  devices[0].live.pages.push(new Uint8Array(4096)); assert.throws(() => root.dataLength, /empty/);
  devices[0].live.pages.length = 0; devices[1].live.pages.push(new Uint8Array(4096)); assert.throws(() => root.dataLength, /empty/);
});

test('empty incompatible root and bank modes reject before initialization writes', () => {
  const { devices, trace } = makeDevices(Array.from({ length: 6 }, blank));
  const root = new MirroredCheckpointRoot(devices[0], devices[5], 1);
  assert.throws(() => new CheckpointPagedCommitLog(root, devices.slice(1, 3)), /flags/);
  assert.deepEqual(trace.calls, []); assertUnchanged(devices, Array.from({ length: 6 }, blank));
});

test('every raw root mutation failure fences without retry, while empty staged selections clear safely', () => {
  const bytes = new Uint8Array(Buffer.from(vectors.decisions[0].bytes, 'hex'));
  for (const method of ['syncData', 'truncateData', 'syncMetadata', 'writeMetadata']) for (const after of [false, true]) {
    const { devices, trace } = makeDevices([blank(), blank()]); const root = new MirroredCheckpointRoot(...devices, 0);
    trace.fault = { at: 1, after };
    const args = method === 'writeMetadata' ? [0, bytes] : method === 'syncData' ? [] : [0];
    assert.throws(() => root[method](...args), /injected/); assertFenced(root, trace);
  }
  const { devices, trace } = makeDevices([blank(), blank()]); const root = new MirroredCheckpointRoot(...devices, 0);
  devices[0].live.metadata[0] = bytes; assert.deepEqual(root.readMetadata(0), bytes);
  devices[0].live.metadata[0] = new Uint8Array(); devices[1].live.metadata[0] = new Uint8Array();
  assert.equal(root.readMetadata(0), undefined); trace.calls.length = 0; root.completeRecovery(); assert.deepEqual(trace.calls, []);
});
