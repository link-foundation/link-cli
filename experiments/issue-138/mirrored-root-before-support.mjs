import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { CheckpointPagedCommitLog } from '../../src/engine-checkpoint-pages.js';
const vectors = JSON.parse(readFileSync(new URL('../../../../docs/protocol/checkpoint-pages-v1.json', import.meta.url)));
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

export { vectors, clone, blank, Device, makeDevices, open, setup, durable, records };
