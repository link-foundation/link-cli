import assert from 'node:assert/strict';
import test from 'node:test';
import { Device } from '../../js/client/test/support/sorted-table.mjs';
import { CheckpointPagedCommitLog } from '../../js/client/src/engine-checkpoint-pages.js';
import { decodeMetadata, metadata } from '../../js/client/src/engine-page-format.js';
import { PhysicalLsmJournalStore } from '../../js/client/src/engine-lsm-store.js';
import { LsmHarness, selected } from '../../js/client/test/support/lsm-store.mjs';
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
