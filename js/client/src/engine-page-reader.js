// Shared validated native doublets cursor, including sealed-table point reads.
import { PAGE, CELLS, PAGE_MAGIC, EXTERNAL, PAYLOAD, decoder, fail, same, view, pageHash, number } from './engine-page-format.js';
export class PageCursor {
  constructor(device, state, expected = state.lastHash) {
    this.device = device; this.state = state; this.index = state.pages - 1;
    this.expected = expected; this.generation = state.generation; this.consumed = 0; this.count = 0;
  }
  current() { return this.index < 0 ? 0 : this.index * CELLS + this.count; }
  load() {
    const { device, index, expected, state } = this;
    const page = device.readCheckedPage === undefined ? device.readPage(index) : device.readCheckedPage(index, expected);
    if (!(page instanceof Uint8Array) || page.length !== PAGE) fail('short page');
    const data = view(page), actualHash = pageHash(page);
    if (!same(page.subarray(0, 8), PAGE_MAGIC) || !same(page.subarray(8, 24), state.identity)
      || number(data.getBigUint64(24, true)) !== index || actualHash !== expected || actualHash !== data.getBigUint64(56, true))
      fail('page checksum or identity');
    const created = number(data.getBigUint64(32, true));
    if (created === 0 || created > this.generation || created < this.generation - 1 || (index === state.pages - 1 && created !== this.generation)
      || (index === 0 && created !== 1)) fail('page generation');
    this.generation = created; this.count = data.getUint32(40, true); this.expected = data.getBigUint64(48, true);
    if (this.count === 0 || this.count > CELLS || data.getUint32(44, true) !== 0
      || page.subarray(64 + this.count * 16).some(Boolean) || (index === 0 && this.expected !== 0n)) fail('page count or padding');
    this.page = page;
  }
  startAt(root) { this.index = Math.floor((root - 1) / CELLS); this.load(); this.count = (root - 1) % CELLS + 1; }
  take(wanted) {
    if (wanted === 0 || wanted !== this.current()) fail('noncanonical doublet reference');
    const data = view(this.page), offset = 64 + (this.count - 1) * 16;
    const result = [data.getBigUint64(offset, true), number(data.getBigUint64(offset + 8, true))];
    this.consumed++; this.count--;
    if (this.count === 0) { this.index--; if (this.index >= 0) this.load(); }
    return result;
  }
  record(root) {
    const [source, previous] = this.take(root);
    let string = number(source); const bytes = [];
    while (string !== 0) {
      const [external, next] = this.take(string), length = Number((external >> 56n) & 7n), payload = external & PAYLOAD;
      if ((external & EXTERNAL) === 0n || (external & 0x7800000000000000n) !== 0n || length === 0
        || (payload >> BigInt(length * 8)) !== 0n || (next !== 0 && length !== 7)) fail('string doublet');
      for (let i = 0; i < length; i++) bytes.push(Number((payload >> BigInt(i * 8)) & 255n));
      string = next;
    }
    const text = decoder.decode(new Uint8Array(bytes));
    if (/[\r\n]/.test(text)) fail('record delimiter');
    if (previous !== this.current()) fail('record predecessor');
    return [text, previous];
  }
}
export function readPageRecords(device, state, retain) {
  const output = [];
  if (state.pages === 0) return output;
  const cursor = new PageCursor(device, state); cursor.load();
  let root = state.root, count = 0;
  while (root !== 0) {
    const [text, previous] = cursor.record(root);
    if (retain) output.push(text);
    root = previous; count++;
  }
  if (cursor.current() !== 0 || cursor.consumed !== state.rows || count !== state.records) fail('record counts');
  return output.reverse();
}
