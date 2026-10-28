// Optional two-copy repair. The caller owns both underlying devices.
import { PAGE, MAX_PAGES, same, view, pageHash, decodeMetadata, finish } from './engine-page-format.js';
const invalid = (message) => { throw new Error('invalid mirrored pages: ' + message); };
const checked = (page, expected) => page instanceof Uint8Array && page.length === PAGE
  && pageHash(page) === expected && view(page).getBigUint64(56, true) === expected;

export class MirroredPageDevice {
  #devices;
  #pages = new Map();
  #metadata = new Map();
  #fenced = false;
  constructor(primary, secondary) {
    if (primary === secondary || !primary || !secondary
      || [primary, secondary].some((device) => typeof device.repairPage !== 'function'))
      throw new TypeError('mirroring requires two distinct repair-capable devices');
    this.#devices = [primary, secondary];
  }
  get metadataFlags() { return 1; }
  get supportsDurableFlush() { return this.#devices.every((device) => device.supportsDurableFlush === true); }
  #check() { if (this.#fenced) invalid('uncertain owner is fenced; reopen both devices'); }
  #mutate(action) {
    this.#check();
    try { action(); } catch (error) { this.#fenced = true; throw error; }
  }
  #both(method, ...args) { this.#mutate(() => { for (const device of this.#devices) finish(device[method](...args)); }); }
  newDatabaseId() { this.#check(); return this.#devices[0].newDatabaseId(); }
  get dataLength() {
    this.#check(); const lengths = this.#devices.map((device) => device.dataLength);
    if (lengths.some((length) => !Number.isSafeInteger(length) || length < 0 || length > MAX_PAGES * PAGE)) invalid('physical length');
    return Math.max(...lengths);
  }
  readMetadata(slot) {
    this.#check();
    const raw = this.#devices.map((device) => {
      try { return device.readMetadata(slot); } catch { return new Uint8Array(1); }
    });
    const decoded = raw.map((bytes) => decodeMetadata(bytes, 1));
    const valid = decoded.map((state, side) => ({ state, side })).filter(({ state }) => state !== null && state !== undefined);
    if (valid.length === 0) {
      if (raw.some((bytes) => bytes !== undefined && bytes.length !== 0)) invalid('both metadata copies are unreadable');
      return undefined;
    }
    if (valid.length === 2 && (!same(valid[0].state.identity, valid[1].state.identity)
      || (valid[0].state.generation === valid[1].state.generation && !same(raw[0], raw[1])))) invalid('conflicting metadata copies');
    const chosen = valid.reduce((left, right) => left.state.generation > right.state.generation ? left : right);
    const bytes = raw[chosen.side].slice();
    this.#metadata.set(slot, { bytes, sides: raw.map((copy, side) => copy !== undefined && same(copy, bytes) ? -1 : side).filter((side) => side >= 0) });
    return bytes;
  }
  readPage() { invalid('expected page checksum is required'); }
  readCheckedPage(index, expected) {
    this.#check();
    const copies = this.#devices.map((device) => {
      try { return device.readPage(index); } catch { return undefined; }
    });
    const valid = copies.map((copy) => checked(copy, expected));
    if (!valid.some(Boolean)) invalid('neither page copy matches the expected checksum');
    if (valid.every(Boolean) && !same(copies[0], copies[1])) invalid('conflicting page copies');
    const source = valid[0] ? 0 : 1;
    if (!valid.every(Boolean)) this.#pages.set(index, { expected, source, destination: 1 - source });
    return copies[source];
  }
  completeRecovery() {
    this.#mutate(() => {
      for (const [index, { expected, source, destination }] of [...this.#pages].sort(([left], [right]) => left - right)) {
        const page = this.#devices[source].readPage(index);
        if (!checked(page, expected)) invalid('repair source changed');
        finish(this.#devices[destination].repairPage(index, page));
      }
      if (this.#pages.size) for (const device of this.#devices) finish(device.syncData());
      for (const [slot, { bytes, sides }] of this.#metadata) for (const side of sides) {
        finish(this.#devices[side].writeMetadata(slot, bytes)); finish(this.#devices[side].syncMetadata(slot));
      }
      this.#pages.clear(); this.#metadata.clear();
    });
  }
  writePage(index, bytes) { this.#both('writePage', index, bytes); }
  syncData() { this.#both('syncData'); }
  writeMetadata(slot, bytes) { this.#both('writeMetadata', slot, bytes); }
  syncMetadata(slot) { this.#both('syncMetadata', slot); }
  truncateData(length) { this.#both('truncateData', length); }
}
