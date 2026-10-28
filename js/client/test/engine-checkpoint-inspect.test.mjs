import assert from 'node:assert/strict';
import test from 'node:test';
import { Device } from './support/sorted-table.mjs';
import { CheckpointPagedCommitLog } from '../src/engine-checkpoint-pages.js';
import { decodeMetadata, metadata } from '../src/engine-page-format.js';
import { PhysicalLsmJournalStore } from '../src/engine-lsm-store.js';
import { LsmHarness, selected } from './support/lsm-store.mjs';
import { MAX_PAGES } from '../src/engine-page-format.js';
import { MirroredPageDevice } from '../src/engine-mirrored-pages.js';
import { MirroredCheckpointRoot } from '../src/engine-checkpoint-root.js';
import { blank, makeDevices, setup, durable } from './support/checkpoint-pages.mjs';
const inspectCheckpointPages = (...args) => CheckpointPagedCommitLog.inspect(...args);

const devices = image => image.wal.map(bytes => new Device(bytes));
const unchanged = (all, before) => {
  assert.deepEqual(all.map(device => device.live), before);
  for (const device of all) assert.ok(device.calls.every(call => call === 'readPage'), device.calls.join(','));
};
function fixture() {
  const harness = new LsmHarness(), store = PhysicalLsmJournalStore.open(harness.wal, harness.provider, selected);
  assert.equal(store.compareExchange(['a'], [{ generation:'0' }], [['durable\0\ufeff😀']]), true);
  store.flush(); store.close();
  return harness;
}

test('empty inspection allocates no identity and initializes no files', () => {
  const all = [new Device(), new Device(), new Device()], before = structuredClone(all.map(device => device.live));
  for (const device of all) device.newDatabaseId = () => { throw Error('inspection allocated identity'); };
  assert.deepEqual(inspectCheckpointPages(all[0], all.slice(1), { maxPages:128 }), []);
  unchanged(all, before);
});

test('selected prefix ignores a finite uncommitted suffix without removing it', () => {
  const harness = fixture(), image = harness.image(), epoch = Number(harness.wal.epoch % 2n);
  image.wal[epoch + 1].pages.push(new Uint8Array(4096).fill(7));
  const all = devices(image), before = structuredClone(all.map(device => device.live));
  const records = inspectCheckpointPages(all[0], all.slice(1), { maxPages:128 });
  assert.deepEqual(records, JSON.parse(harness.wal.read())); unchanged(all, before);
  const ordinary = new CheckpointPagedCommitLog(all[0], all.slice(1), { maxPages:128 });
  assert.deepEqual(JSON.parse(ordinary.read()), records);
  assert.equal(all[epoch + 1].live.pages.length, image.wal[epoch + 1].pages.length - 1);
  assert.ok(all[epoch + 1].calls.includes('truncateData'));
});

test('missing selected table rejects before the WAL suffix is truncated', () => {
  const harness = fixture(), image = harness.image(), epoch = Number(harness.wal.epoch % 2n);
  image.wal[epoch + 1].pages.push(new Uint8Array(4096).fill(9));
  const all = devices(image), before = structuredClone(all.map(device => device.live));
  const records = inspectCheckpointPages(all[0], all.slice(1), { maxPages:128 });
  const wal = { supportsReplacement:true, read:() => JSON.stringify(records) };
  for (const name of ['append','flush','replace']) wal[name] = () => { throw Error('early WAL mutation'); };
  harness.tables.clear(); harness.reset();
  assert.throws(() => PhysicalLsmJournalStore.open(wal, harness.provider, selected), /missing selected/);
  assert.deepEqual(harness.trace, []); unchanged(all, before);
});

test('selected page damage and a lower page bound fail without cleanup', () => {
  const harness = fixture(), image = harness.image(), epoch = Number(harness.wal.epoch % 2n);
  image.wal[epoch + 1].pages[0][100] ^= 1;
  let all = devices(image), before = structuredClone(all.map(device => device.live));
  assert.throws(() => inspectCheckpointPages(all[0], all.slice(1), { maxPages:128 }), /checksum/);
  unchanged(all, before);
  const clean = fixture();
  clean.wal.append('x'.repeat(2000)); clean.wal.flush();
  all = devices(clean.image()); before = structuredClone(all.map(device => device.live));
  assert.throws(() => inspectCheckpointPages(all[0], all.slice(1), { maxPages:1 }), /exceed selected/);
  unchanged(all, before);
});

test('unrooted committed banks and selected identity changes fail without writes', () => {
  const harness = fixture(), image = harness.image(), epoch = Number(harness.wal.epoch % 2n);
  image.wal[0].metadata = [undefined, undefined];
  let all = devices(image), before = structuredClone(all.map(device => device.live));
  assert.throws(() => inspectCheckpointPages(all[0], all.slice(1), { maxPages:128 }), /unrooted/); unchanged(all, before);
  const clean = harness.image();
  for (let slot = 0; slot < 2; slot++) {
    const bytes = clean.wal[epoch + 1].metadata[slot], state = decodeMetadata(bytes, 0);
    if (state !== null && state !== undefined) { state.identity[0] ^= 1; clean.wal[epoch + 1].metadata[slot] = metadata(state); }
  }
  all = devices(clean); before = structuredClone(all.map(device => device.live));
  assert.throws(() => inspectCheckpointPages(all[0], all.slice(1), { maxPages:128 }), /identity changed/); unchanged(all, before);
});

test('inspection validates finite options and never completes staged repair', () => {
  const harness = fixture(), all = devices(harness.image()), before = structuredClone(all.map(device => device.live));
  for (const device of all) {
    device.completeRecovery = () => { throw Error('early repair'); };
    device.readCheckedPage = index => device.readPage(index);
  }
  assert.ok(inspectCheckpointPages(all[0], all.slice(1), { maxPages:128 }).length > 0);
  for (const maxPages of [0,-1,1.5,Infinity]) assert.throws(() => inspectCheckpointPages(all[0], all.slice(1), { maxPages }), RangeError);
  assert.throws(() => inspectCheckpointPages(all[0], [all[1],all[1]]), /distinct/); unchanged(all, before);
});

test('inspection rejects invalid ownership, modes, lengths and selected metadata without writes', () => {
  const all = [new Device(), new Device(), new Device()];
  for (const [root, banks] of [[all[0],null],[all[0],[]],[null,all.slice(1)],[all[0],[null,all[2]]],[all[0],[all[0],all[2]]]])
    assert.throws(() => inspectCheckpointPages(root, banks), /distinct/);
  for (const maxPages of [NaN, MAX_PAGES + 1]) assert.throws(() => inspectCheckpointPages(all[0], all.slice(1), { maxPages }), RangeError);
  assert.deepEqual(inspectCheckpointPages(all[0], all.slice(1)), []);
  const mode = (root, first, second, valid = false) => {
    all.forEach((device,i) => { device.metadataFlags = [root,first,second][i]; });
    if (valid) assert.deepEqual(inspectCheckpointPages(all[0], all.slice(1)), []);
    else assert.throws(() => inspectCheckpointPages(all[0], all.slice(1)), /modes/);
  };
  mode(0,1,1,true); mode(2,0,0,true); mode(3,1,1,true);
  mode(1,0,0); mode(2,1,1); mode(0,2,2); mode(0,0,1);
  mode(0,0,0,true); all[0].live.pages.push(new Uint8Array(4096));
  assert.throws(() => inspectCheckpointPages(all[0], all.slice(1)), /root data/);
  const base = setup(); base.log.append('acknowledged'); base.log.flush();
  const images = durable(base.devices), cases = [];
  const add = (change, pattern) => { const image = structuredClone(images); change(image); cases.push([image,pattern]); };
  add(image => { image[1].metadata = [undefined,undefined]; }, /no valid metadata/);
  add(image => { image[1].pages = []; }, /truncated/);
  add(image => { image[0].metadata = [undefined,undefined]; image[1].pages = []; }, /committed metadata/);
  add(image => {
    const state = decodeMetadata(image[1].metadata[1],0); state.identity[0] ^= 1;
    image[1].metadata[0] = metadata(state);
  }, /identity changed/);
  add(image => {
    const state = decodeMetadata(image[1].metadata[1],0); state.lastHash ^= 1n;
    image[1].metadata[0] = metadata(state);
  }, /conflicting metadata/);
  for (const [image,pattern] of cases) {
    const current = image.map(item => new Device(item)), before = structuredClone(current.map(device => device.live));
    assert.throws(() => inspectCheckpointPages(current[0], current.slice(1)), pattern); unchanged(current,before);
  }
  const current = images.map(item => new Device(item));
  Object.defineProperty(current[1], 'dataLength', { get:() => Infinity });
  assert.throws(() => inspectCheckpointPages(current[0], current.slice(1)), /truncated/);
  // Two identical decisions and either missing slot remain valid and owned.
  for (const slot of [0,1,2]) {
    const image = structuredClone(images); image[1].metadata[0] = image[1].metadata[1].slice();
    if (slot < 2) image[1].metadata[slot] = undefined;
    const current = image.map(item => new Device(item));
    const records = inspectCheckpointPages(current[0], current.slice(1)); records[0] = 'caller edit';
    assert.deepEqual(inspectCheckpointPages(current[0], current.slice(1)), ['acknowledged']);
  }
  // A partially initialized, zero-page bank is inspected without finishing creation.
  const partial = structuredClone(images); partial[0].metadata = [undefined,undefined];
  partial[1].pages = []; partial[1].metadata[1] = undefined;
  const currentPartial = partial.map(item => new Device(item));
  assert.deepEqual(inspectCheckpointPages(currentPartial[0], currentPartial.slice(1)), []);
  unchanged(currentPartial,partial);
});

test('actual mirrored root and banks stage surviving-copy repairs until later validated opening', () => {
  const initial = makeDevices(Array.from({ length:6 },blank));
  const wrap = all => ({ root:new MirroredCheckpointRoot(all[0],all[5],1),
    banks:[new MirroredPageDevice(all[1],all[3]),new MirroredPageDevice(all[2],all[4])] });
  let owners = wrap(initial.devices), log = new CheckpointPagedCommitLog(owners.root,owners.banks,{ maxPages:4 });
  log.append('old'); log.flush(); log.replace(['acknowledged']);
  const images = durable(initial.devices); images[0].metadata[1][120] ^= 1; images[2].pages[0][100] ^= 1;
  const current = makeDevices(images); owners = wrap(current.devices); current.trace.calls.length = 0;
  assert.deepEqual(inspectCheckpointPages(owners.root,owners.banks,{ maxPages:4 }), ['acknowledged']);
  assert.deepEqual(current.devices.map(device => device.live),images);
  assert.ok(current.trace.calls.every(call => /\.(dataLength|readMetadata[01]|readPage)$/.test(call)));
  // Drop the inspection wrappers and reopen: only the ordinary validated open repairs.
  owners = wrap(current.devices); log = new CheckpointPagedCommitLog(owners.root,owners.banks,{ maxPages:4 });
  assert.deepEqual(JSON.parse(log.read()),['acknowledged']);
  assert.deepEqual(current.devices[0].live.metadata,current.devices[5].live.metadata);
  assert.deepEqual(current.devices[2].live.pages,current.devices[4].live.pages);
});
