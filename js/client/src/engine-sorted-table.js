// Optional immutable physical sorted table over SQL-free doublets pages.
import { PAGE, CELLS, encoder, decodeMetadata, same, finish, fail } from './engine-page-format.js';
import { PageCursor, readPageRecords } from './engine-page-reader.js';
import { PagedCommitLog } from './engine-paged-log.js';
const sealed = Symbol();
const marker = '["native-sst1"]';
function options({ maxKeys = 4096, maxBytes = 8_388_608 } = {}) {
  if (!Number.isSafeInteger(maxKeys) || maxKeys < 1 || maxKeys > 65536
    || !Number.isSafeInteger(maxBytes) || maxBytes < 1 || maxBytes > 67108864) throw new RangeError('sorted table admission bounds');
  return { maxKeys, maxBytes, pages: Math.ceil((Math.ceil(maxBytes / 7) + 2 * (maxKeys + 1)) / CELLS) };
}
function key(value) {
  if (typeof value !== 'string' || value.length < 1 || value.length > 128 || /[^A-Za-z0-9_.-]/.test(value)) throw new TypeError('sorted table key');
}
function row(value) {
  if (!Array.isArray(value) || value.length !== 3) throw new TypeError('sorted table record shape');
  const [name, generation, entries] = value; key(name);
  if (typeof generation !== 'string' || (!/^[1-9]/.test(generation) || /[^0-9]/.test(generation)) || generation.length > 20 || BigInt(generation) > 18446744073709551615n)
    throw new TypeError('sorted table canonical generation');
  if (entries !== null) {
    if (!Array.isArray(entries)) throw new TypeError('sorted table records');
    for (const entry of entries) if (typeof entry !== 'string' || !entry.isWellFormed()) throw new TypeError('sorted table record type');
  }
  return { key: name, generation, entries };
}
export class SortedJournalTable {
  #device; #state; #index; #hashes; #bytes;
  constructor(token, device, state, index, hashes, bytes) {
    // Use factories: arguments carry a completely validated physical state.
    if (token !== sealed) throw new TypeError('use SortedJournalTable.create/open');
    this.#device = device; this.#state = state; this.#index = index; this.#hashes = hashes; this.#bytes = bytes;
  }
  static create(device, entries, selected) {
    const bounds = options(selected);
    if (!Array.isArray(entries) || entries.length > bounds.maxKeys) throw new RangeError('sorted table key limit');
    const records = [marker]; let bytes = marker.length, previous = '';
    for (const entry of entries) {
      if (entry === null || typeof entry !== 'object') throw new TypeError('sorted table entry');
      const value = row([entry.key, entry.generation, entry.entries]);
      if (value.key <= previous) throw new TypeError('sorted table key ordering');
      const record = JSON.stringify([value.key, value.generation, value.entries]); bytes += encoder.encode(record).length;
      if (bytes > bounds.maxBytes) throw new RangeError('sorted table byte limit'); records.push(record); previous = value.key;
    }
    if (bytes > bounds.maxBytes) throw new RangeError('sorted table byte limit');
    const raw = [device.readMetadata(0), device.readMetadata(1)];
    if (raw.some((value) => value !== undefined && value.length !== 0) || device.dataLength !== 0) throw new Error('sorted table creation requires an empty device');
    const log = new PagedCommitLog(device, { maxPages: bounds.pages });
    for (const record of records) log.append(record); log.flush(); return SortedJournalTable.open(device, selected);
  }
  static open(device, selected) {
    const bounds = options(selected), flags = device.metadataFlags ?? 0;
    if (flags !== 0 && flags !== 1) fail('unsupported sorted table device mode');
    const raw = [device.readMetadata(0), device.readMetadata(1)], states = raw.map((bytes) => decodeMetadata(bytes, flags)).filter((state) => state !== null && state !== undefined);
    if (states.length === 2 && (!same(states[0].identity, states[1].identity) || (states[0].generation === states[1].generation && !same(raw[0], raw[1]))))
      fail('conflicting sorted table decisions');
    const state = states.sort((a, b) => b.generation - a.generation)[0];
    if (state === undefined || state.generation !== 1 || state.records > bounds.maxKeys + 1 || state.pages > bounds.pages || device.dataLength !== state.pages * PAGE)
      fail('sorted table seal, length or admission bounds');
    const records = readPageRecords(device, state, true);
    if (records[0] !== marker) fail('unsupported sorted table marker');
    const index = []; let bytes = 0, root = 0, previous = '';
    for (let i = 0; i < records.length; i++) {
      const length = encoder.encode(records[i]).length; bytes += length; root += Math.ceil(length / 7) + 1;
      if (bytes > bounds.maxBytes) fail('sorted table byte limit');
      if (i !== 0) {
        const value = row(JSON.parse(records[i])); if (value.key <= previous) fail('sorted table key ordering');
        index.push({ key: value.key, root }); previous = value.key;
      }
    }
    if (root !== state.root || root !== state.rows) fail('sorted table packed addresses');
    const cursor = new PageCursor(device, state), hashes = new Array(state.pages);
    while (cursor.index >= 0) { const expected = cursor.expected; cursor.load(); hashes[cursor.index] = expected; cursor.index--; }
    finish(device.completeRecovery?.()); return new SortedJournalTable(sealed, device, state, index, hashes, bytes);
  }
  get count() { return this.#index.length; }
  get storedBytes() { return this.#bytes; }
  get identity() { return this.#state.identity.slice(); }
  get digest() { return this.#state.lastHash.toString(); }
  get supportsDurableFlush() { return this.#device.supportsDurableFlush === true; }
  get(name) {
    key(name); let low = 0, high = this.#index.length - 1;
    while (low <= high) {
      const mid = low + Math.floor((high - low) / 2), current = this.#index[mid].key;
      if (current === name) return this.#at(mid);
      if (current < name) low = mid + 1; else high = mid - 1;
    }
    return undefined;
  }
  entries() { return this.#index.map((_, index) => this.#at(index)); }
  #at(index) {
    const entry = this.#index[index], page = Math.floor((entry.root - 1) / CELLS);
    const cursor = new PageCursor(this.#device, this.#state, this.#hashes[page]); cursor.startAt(entry.root);
    return row(JSON.parse(cursor.record(entry.root)[0]));
  }
}
