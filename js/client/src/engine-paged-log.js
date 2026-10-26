// SQL-free native doublets pages under the synchronous ordered-log contract.
// Wire format and persistence assumptions: docs/protocol/native-pages.md.
const PAGE = 4096, CELLS = 252, MAX_PAGES = Math.floor(Number.MAX_SAFE_INTEGER / PAGE);
const encoder = new TextEncoder();
const decoder = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });
const META_MAGIC = encoder.encode('LNKDMT01'), PAGE_MAGIC = encoder.encode('LNKDPG01');
const EXTERNAL = 1n << 63n, PAYLOAD = (1n << 56n) - 1n;
const fail = (text) => { throw new Error('invalid native pages: ' + text); };
const same = (a, b) => a.length === b.length && a.every((value, index) => value === b[index]);
const view = (bytes) => new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
const hash = (bytes, initial = 0xcbf29ce484222325n) => {
  let result = initial;
  for (const byte of bytes) result = BigInt.asUintN(64, (result ^ BigInt(byte)) * 0x100000001b3n);
  return result;
};
const pageHash = (bytes) => hash(bytes.subarray(64), hash(bytes.subarray(0, 56)));
const number = (value) => {
  if (value > BigInt(Number.MAX_SAFE_INTEGER)) fail('integer exceeds portable range');
  return Number(value);
};
const finish = (value) => { if (value !== undefined) throw new TypeError('page I/O must complete synchronously'); };

function metadata(state) {
  const bytes = new Uint8Array(128), data = view(bytes);
  bytes.set(META_MAGIC); data.setUint32(8, 1, true); bytes.set(state.identity, 16);
  for (const [index, value] of [state.generation, state.pages, state.rows, state.records, state.root, state.lastHash].entries())
    data.setBigUint64(32 + index * 8, BigInt(value), true);
  data.setBigUint64(120, hash(bytes.subarray(0, 120)), true);
  return bytes;
}
function decodeMetadata(bytes) {
  if (bytes === undefined || bytes.length === 0) return undefined;
  if (!(bytes instanceof Uint8Array)) fail('metadata bytes');
  if (bytes.length !== 128) return null;
  const data = view(bytes);
  if (hash(bytes.subarray(0, 120)) !== data.getBigUint64(120, true)) return null;
  if (!same(bytes.subarray(0, 8), META_MAGIC) || data.getUint32(8, true) !== 1) fail('unsupported metadata version');
  if (data.getUint32(12, true) !== 0 || bytes.subarray(80, 120).some(Boolean)) fail('unsupported metadata flags');
  const values = Array.from({ length: 5 }, (_, i) => number(data.getBigUint64(32 + i * 8, true)));
  const [generation, pages, rows, records, root] = values;
  const lastHash = data.getBigUint64(72, true);
  if (pages > MAX_PAGES || generation > pages || rows > pages * CELLS || records > rows || rows < pages || records < generation)
    fail('metadata counts');
  if (pages === 0 ? values.some(Boolean) || lastHash !== 0n
    : generation === 0 || root <= (pages - 1) * CELLS || root > pages * CELLS) fail('metadata root');
  return { identity: bytes.slice(16, 32), generation, pages, rows, records, root, lastHash };
}

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
    const raw = [device.readMetadata(0), device.readMetadata(1)];
    const states = raw.map(decodeMetadata);
    const valid = states.filter((state) => state !== null && state !== undefined);
    if (valid.length === 0) {
      if (states.some((state) => state === null) || device.dataLength !== 0) fail('no valid metadata decision');
      const identity = device.newDatabaseId();
      if (!(identity instanceof Uint8Array) || identity.length !== 16) fail('database identity');
      this.#state = { identity: identity.slice(), generation: 0, pages: 0, rows: 0, records: 0, root: 0, lastHash: 0n };
      finish(device.syncData());
      finish(device.writeMetadata(0, metadata(this.#state))); finish(device.syncMetadata(0));
    } else {
      if (valid.length === 2 && (!same(valid[0].identity, valid[1].identity)
        || (valid[0].generation === valid[1].generation && !same(raw[0], raw[1])))) fail('conflicting metadata decisions');
      this.#state = valid.reduce((left, right) => left.generation > right.generation ? left : right);
      if (this.#state.pages > maxPages) fail('committed pages exceed selected limit');
      const committedLength = this.#state.pages * PAGE;
      if (!Number.isSafeInteger(device.dataLength) || device.dataLength < committedLength) fail('truncated committed pages');
      this.#readCommitted(false);
      if (device.dataLength > committedLength) { finish(device.truncateData(committedLength)); finish(device.syncData()); }
    }
  }
  get supportsDurableFlush() { return this.#device.supportsDurableFlush === true; }
  #check() { if (this.#fenced) throw new Error('uncertain native page owner is fenced; reopen the log'); }
  read() {
    this.#check();
    try { return JSON.stringify([...this.#readCommitted(true), ...this.#pending.map((entry) => entry.record)]); }
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
      page = this.#device.readPage(index);
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
