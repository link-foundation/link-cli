// Optional physical journal compression. Reopen the stream through this
// decorator, with one externally fenced writer and caller-owned resources.
const U32 = 0xffff_ffffn;
const U64 = (1n << 64n) - 1n;
const U128 = (1n << 128n) - 1n;
const checksum = (text) => {
  let hash = 0xcbf29ce484222325n;
  for (let i = 0; i < text.length; i++) hash = ((hash ^ BigInt(text.charCodeAt(i))) * 0x100000001b3n) & U64;
  return hash.toString(16).padStart(16, '0');
};
const decimal = (text, max) => {
  if (text.length > 39 || !/^(0|[1-9][0-9]*)$/.test(text)) throw Error('invalid delta journal decimal');
  const value = BigInt(text);
  if (value > max) throw Error('delta journal decimal out of range');
  return value;
};
const verified = (record) => {
  if (typeof record !== 'string' || !/^[\x20-\x7e]*$/.test(record)) throw Error('delta journal requires printable ASCII records');
  const separator = record.lastIndexOf('|');
  if (separator < 0 || record.slice(separator + 1) !== checksum(record.slice(0, separator))) throw Error('delta journal checksum mismatch');
  return record.split('|');
};
const frame = (record) => {
  const fields = verified(record);
  if (fields.length !== 5 || fields[0] !== 'engine1') throw Error('expected a complete engine1 revision');
  const revision = decimal(fields[1], U64);
  if (revision === 0n || decimal(fields[2], U128) === 0n) throw Error('invalid delta journal revision or allocator');
  return { revision, next: fields[2], rows: fields[3], hash: fields[4] };
};
const contiguous = (before, after) => {
  const expected = before === null ? 1n : before.revision + 1n;
  if (after.revision !== expected) throw Error('delta journal revisions are not contiguous');
};
const expand = (before, record) => {
  if (!record.startsWith('delta1|')) return record;
  const fields = verified(record);
  if (fields.length !== 9) throw Error('invalid delta journal envelope');
  if (before === null) throw Error('delta journal base is missing');
  const base = before;
  if (fields[1] !== base.hash) throw Error('delta journal base checksum mismatch');
  const prefix = Number(decimal(fields[4], U32));
  const suffix = Number(decimal(fields[5], U32));
  if (prefix > base.rows.length || suffix > base.rows.length - prefix) throw Error('delta journal span outside base');
  return `engine1|${fields[2]}|${fields[3]}|${base.rows.slice(0, prefix)}${fields[6]}${base.rows.slice(base.rows.length - suffix)}|${fields[7]}`;
};
const compress = (a, b, full) => {
  let prefix = 0; let suffix = 0;
  // Cap spans to the portable u32 format even in a host with larger strings.
  while (prefix < Math.min(a.rows.length, b.rows.length, Number(U32)) && a.rows[prefix] === b.rows[prefix]) prefix++;
  while (suffix < Math.min(Math.min(a.rows.length, b.rows.length) - prefix, Number(U32)) && a.rows[a.rows.length - suffix - 1] === b.rows[b.rows.length - suffix - 1]) suffix++;
  const text = `delta1|${a.hash}|${b.revision}|${b.next}|${prefix}|${suffix}|${b.rows.slice(prefix, b.rows.length - suffix)}|${b.hash}`;
  const delta = `${text}|${checksum(text)}`;
  return delta.length < full.length ? delta : full;
};
const complete = (value) => {
  if (value !== undefined) throw TypeError('log operations must complete synchronously and return undefined');
};

/** Single-span compression; engine cloning/replay remain O(N), without compaction. */
export class DeltaCommitLog {
  #inner;
  #interval;
  #previous = null;
  #streak = 0;
  #ready = false;
  #poisoned = false;
  constructor(inner, { checkpointInterval = 128 } = {}) {
    if (!Number.isInteger(checkpointInterval) || checkpointInterval < 1 || checkpointInterval > Number(U32))
      throw RangeError('checkpoint interval must be a positive u32');
    this.#inner = inner;
    this.#interval = checkpointInterval;
  }
  get supportsDurableFlush() { return this.#inner.supportsDurableFlush === true; }
  #check() { if (this.#poisoned) throw Error('indeterminate delta journal outcome; reopen the decorator'); }
  read() {
    this.#check();
    this.#ready = false;
    const records = JSON.parse(this.#inner.read());
    if (!Array.isArray(records) || records.some((record) => typeof record !== 'string')) throw TypeError('log read must return a JSON array of strings');
    const logical = []; let previous = null; let streak = 0;
    for (const record of records) {
      const full = expand(previous, record);
      const parsed = frame(full);
      contiguous(previous, parsed);
      logical.push(full); previous = parsed;
      streak = record.startsWith('delta1|') ? streak + 1 : 0;
    }
    this.#previous = previous; this.#streak = streak; this.#ready = true;
    return JSON.stringify(logical);
  }
  append(record) {
    this.#check();
    if (!this.#ready) this.read();
    const parsed = frame(record);
    contiguous(this.#previous, parsed);
    const physical = this.#previous !== null && this.#streak + 1 < this.#interval ? compress(this.#previous, parsed, record) : record;
    try { complete(this.#inner.append(physical)); }
    catch (error) { this.#poisoned = true; throw error; }
    this.#streak = physical.startsWith('delta1|') ? this.#streak + 1 : 0;
    this.#previous = parsed;
  }
  flush() {
    this.#check();
    try { complete(this.#inner.flush()); }
    catch (error) { this.#poisoned = true; throw error; }
  }
}
