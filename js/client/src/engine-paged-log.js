// SQL-free ordered log; internal page wire helpers are shared with repair.
import { PAGE, CELLS, MAX_PAGES, encoder, decoder, PAGE_MAGIC, EXTERNAL, PAYLOAD, fail, same, view, pageHash, number, finish, metadata, decodeMetadata } from './engine-page-format.js';

export class PagedCommitLog {
  #device;
  #state;
  #pending = [];
  #pendingRows = 0;
  #fenced = false;
  #maxPages;
  constructor(device, { maxPages = MAX_PAGES } = {}) {
    if (!Number.isSafeInteger(maxPages) || maxPages < 1 || maxPages > MAX_PAGES) throw new RangeError('invalid native page limit');
    this.#maxPages = maxPages;
    this.#device = device;
    const flags = device.metadataFlags ?? 0;
    if (flags !== 0 && flags !== 1) fail('unsupported device mode');
    const raw = [device.readMetadata(0), device.readMetadata(1)];
    const states = raw.map((bytes) => decodeMetadata(bytes, flags));
    const valid = states.filter((state) => state !== null && state !== undefined);
    if (valid.length === 0) {
      if (states.some((state) => state === null) || device.dataLength !== 0) fail('no valid metadata decision');
      const identity = device.newDatabaseId();
      if (!(identity instanceof Uint8Array) || identity.length !== 16) fail('database identity');
      this.#state = { flags, identity: identity.slice(), generation: 0, pages: 0, rows: 0, records: 0, root: 0, lastHash: 0n };
      finish(device.syncData());
      finish(device.writeMetadata(0, metadata(this.#state))); finish(device.syncMetadata(0));
    } else {
      if (valid.length === 2 && (!same(valid[0].identity, valid[1].identity)
        || (valid[0].generation === valid[1].generation && !same(raw[0], raw[1])))) fail('conflicting metadata decisions');
      this.#state = valid.reduce((left, right) => left.generation > right.generation ? left : right);
      if (this.#state.pages > maxPages) fail('committed pages exceed selected limit');
      const committedLength = this.#state.pages * PAGE;
      if (!Number.isSafeInteger(device.dataLength) || device.dataLength < committedLength) fail('truncated committed pages');
      this.#readCommitted(false); finish(device.completeRecovery?.());
      if (device.dataLength > committedLength) { finish(device.truncateData(committedLength)); finish(device.syncData()); }
    }
  }
  get supportsDurableFlush() { return this.#device.supportsDurableFlush === true; }
  #check() { if (this.#fenced) throw new Error('uncertain native page owner is fenced; reopen the log'); }
  read() {
    this.#check();
    try {
      const committed = this.#readCommitted(true); finish(this.#device.completeRecovery?.());
      return JSON.stringify([...committed, ...this.#pending.map((entry) => entry.record)]);
    }
    catch (error) { this.#fenced = true; throw error; }
  }
  append(record) {
    this.#check();
    if (typeof record !== 'string' || !record.isWellFormed() || /[\r\n]/.test(record))
      throw new TypeError('journal records must be well-formed strings without line breaks');
    const bytes = encoder.encode(record);
    const rows = Math.ceil(bytes.length / 7) + 1;
    if (this.#state.pages + Math.ceil((this.#pendingRows + rows) / CELLS) > this.#maxPages)
      throw new RangeError('native page address space exhausted');
    this.#pending.push({ record, bytes }); this.#pendingRows += rows;
  }
  flush() {
    this.#check();
    if (this.#pending.length === 0) return;
    this.#fenced = true;
    const next = { ...this.#state, generation: this.#state.generation + 1 };
    let page = new Uint8Array(PAGE), count = 0;
    const writePage = () => {
      page.set(PAGE_MAGIC); page.set(next.identity, 8);
      const data = view(page);
      data.setBigUint64(24, BigInt(next.pages), true); data.setBigUint64(32, BigInt(next.generation), true);
      data.setUint32(40, count, true); data.setBigUint64(48, next.lastHash, true);
      next.lastHash = pageHash(page); data.setBigUint64(56, next.lastHash, true);
      finish(this.#device.writePage(next.pages, page));
      next.pages++; page = new Uint8Array(PAGE); count = 0;
    };
    const cell = (source, target) => {
      if (count === CELLS) writePage();
      const id = next.pages * CELLS + count + 1, data = view(page);
      data.setBigUint64(64 + count * 16, BigInt(source), true); data.setBigUint64(72 + count * 16, BigInt(target), true);
      count++; next.rows++; return id;
    };
    for (const entry of this.#pending) {
      let string = 0;
      for (let start = Math.floor((entry.bytes.length - 1) / 7) * 7; start >= 0; start -= 7) {
        const chunk = entry.bytes.subarray(start, start + 7);
        let payload = 0n;
        for (let i = 0; i < chunk.length; i++) payload |= BigInt(chunk[i]) << BigInt(i * 8);
        string = cell(EXTERNAL | BigInt(chunk.length) << 56n | payload, string);
      }
      next.root = cell(string, next.root); next.records++;
    }
    writePage(); finish(this.#device.syncData());
    const slot = next.generation % 2;
    finish(this.#device.writeMetadata(slot, metadata(next))); finish(this.#device.syncMetadata(slot));
    this.#state = next; this.#pending = []; this.#pendingRows = 0; this.#fenced = false;
  }
  #readCommitted(retain) {
    const state = this.#state, output = [];
    if (state.pages === 0) return output;
    let index = state.pages - 1, expected = state.lastHash, generation = state.generation;
    let page, count = 0, consumed = 0, recordCount = 0;
    const load = () => {
      page = this.#device.readCheckedPage === undefined
        ? this.#device.readPage(index) : this.#device.readCheckedPage(index, expected);
      if (!(page instanceof Uint8Array) || page.length !== PAGE) fail('short page');
      const data = view(page), actualHash = pageHash(page);
      if (!same(page.subarray(0, 8), PAGE_MAGIC) || !same(page.subarray(8, 24), state.identity)
        || number(data.getBigUint64(24, true)) !== index || actualHash !== expected || actualHash !== data.getBigUint64(56, true))
        fail('page checksum or identity');
      const created = number(data.getBigUint64(32, true));
      if (created === 0 || created > generation || created < generation - 1 || (index === state.pages - 1 && created !== generation)
        || (index === 0 && created !== 1)) fail('page generation');
      generation = created; count = data.getUint32(40, true); expected = data.getBigUint64(48, true);
      if (count === 0 || count > CELLS || data.getUint32(44, true) !== 0
        || page.subarray(64 + count * 16).some(Boolean) || (index === 0 && expected !== 0n)) fail('page count or padding');
    };
    load();
    const current = () => index < 0 ? 0 : index * CELLS + count;
    const take = (wanted) => {
      if (wanted === 0 || wanted !== current()) fail('noncanonical doublet reference');
      const data = view(page), offset = 64 + (count - 1) * 16;
      const result = [data.getBigUint64(offset, true), number(data.getBigUint64(offset + 8, true))];
      consumed++; count--;
      if (count === 0) { index--; if (index >= 0) load(); }
      return result;
    };
    let root = state.root;
    while (root !== 0) {
      const [source, previous] = take(root);
      let string = number(source), bytes = [];
      while (string !== 0) {
        const [external, next] = take(string), length = Number((external >> 56n) & 7n), payload = external & PAYLOAD;
        if ((external & EXTERNAL) === 0n || (external & 0x7800000000000000n) !== 0n || length === 0
          || (payload >> BigInt(length * 8)) !== 0n || (next !== 0 && length !== 7)) fail('string doublet');
        for (let i = 0; i < length; i++) bytes.push(Number((payload >> BigInt(i * 8)) & 255n));
        string = next;
      }
      const text = decoder.decode(new Uint8Array(bytes));
      if (/[\r\n]/.test(text)) fail('record delimiter');
      if (previous !== current()) fail('record predecessor');
      if (retain) output.push(text);
      root = previous; recordCount++;
    }
    if (current() !== 0 || consumed !== state.rows || recordCount !== state.records) fail('record counts');
    return output.reverse();
  }
}
