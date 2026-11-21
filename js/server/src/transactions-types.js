// The value types and wire codecs of the transactions layer: transitions
// (rust/src/transactions/types.rs), complete journal records with their name
// changes (rust/src/transactions/record.rs) and write-ahead undo images
// (rust/src/transactions/undo_image.rs), plus the Rust number parsing,
// string ordering and sidecar file names they rely on.
//
// Addresses are numbers in `0..=0xffffffff`. Transaction ids (`u128`),
// sequences and timestamps (`i64`) are BigInts, so a log reads and writes
// exactly the values the Rust clink does.
import { AddressOverflowError, InvalidFormatError } from './errors.js';
import { link, MAX_ADDRESS } from './link.js';
import { trim } from './text-storage-format.js';

export const I64_MAX = (1n << 63n) - 1n;
const I64_MIN = -(1n << 63n);
const U128_MAX = (1n << 128n) - 1n;
const U64_MAX = (1n << 64n) - 1n;

/** The timestamps a complete record may hold: .NET's representable range. */
export const MIN_RECORD_MS = -62_135_596_800_000n;
export const MAX_RECORD_MS = 253_402_300_799_999n;

/** Sidecar-store name prefixes used by the recovery protocol. */
export const COMMIT_MARKER_PREFIX = '__transactions:commit:';
export const ROLLBACK_MARKER_PREFIX = '__transactions:rollback:';
export const APPLIED_MARKER_PREFIX = '__transactions:applied:';
export const TRANSITION_NAME_PREFIX = '__transactions:transition:';

/**
 * `str::parse` of a Rust integer type, or `from_str_radix(text, 16)`: an
 * optional `+` (and `-` for signed types) before at least one digit, and no
 * overflow. The value as a BigInt, or `null` when Rust would fail.
 */
function parseInteger(text, min, max, hex = false) {
  const match = (hex ? /^([+-]?)([0-9a-fA-F]+)$/ : /^([+-]?)([0-9]+)$/).exec(text);
  if (match === null || (match[1] === '-' && min === 0n)) return null;
  const magnitude = BigInt(hex ? `0x${match[2]}` : match[2]);
  const value = match[1] === '-' ? -magnitude : magnitude;
  return value < min || value > max ? null : value;
}

export const parseI64 = (text) => parseInteger(text, I64_MIN, I64_MAX);
export const parseU64 = (text) => parseInteger(text, 0n, U64_MAX);
export const parseU128 = (text) => parseInteger(text, 0n, U128_MAX);
export const parseU128Hex = (text) => parseInteger(text, 0n, U128_MAX, true);
export const parseU64Hex = (text) => parseInteger(text, 0n, U64_MAX, true);
const parseU8 = (text) => parseInteger(text, 0n, 255n);

/** `{:032x}` of a transaction id. */
export const hex32 = (id) => id.toString(16).padStart(32, '0');

/** Ascending order of BigInts. */
export const compareBig = (left, right) => (left < right ? -1 : left > right ? 1 : 0);

/** Rust `String` order: by code point (UTF-8 bytes), not by UTF-16 unit. */
export function compareStrings(left, right) {
  const a = [...left];
  const b = [...right];
  for (let i = 0; i < Math.min(a.length, b.length); i++) {
    const difference = a[i].codePointAt(0) - b[i].codePointAt(0);
    if (difference !== 0) return difference;
  }
  return a.length - b.length;
}

/** An address parsed from decimal text: `null` when it is not a `u128`. */
function parseAddress(text) {
  const value = parseU128(text);
  if (value === null) return null;
  if (value > BigInt(MAX_ADDRESS)) throw new AddressOverflowError(value);
  return Number(value);
}

export const TransitionKind = Object.freeze({ Create: 0, Update: 1, Delete: 2 });

/** The `{:?}` text of a transition kind. */
export const KIND_NAMES = Object.freeze(['Create', 'Update', 'Delete']);

export const CommitMode = Object.freeze({ Sync: 'Sync', Async: 'Async' });

export const RowUndoPolicy = Object.freeze({
  Observed: 'Observed',
  WriteAheadImage: 'WriteAheadImage',
});

export const INFINITE_RETENTION = Object.freeze({ kind: 'infinite' });

/** `LogRetentionPolicy::parse`: `infinite`, `sized:<n>` or `chunked:<n>:<dir>`. */
export function parseRetentionPolicy(spec) {
  const trimmed = trim(spec);
  const lowered = trimmed.replace(/[A-Z]+/g, (letters) => letters.toLowerCase());
  if (trimmed === '' || lowered === 'infinite') return INFINITE_RETENTION;
  if (lowered.startsWith('sized:')) {
    const max = parseU64(trimmed.slice('sized:'.length));
    if (max === null) throw new Error(`invalid sized retention spec '${spec}'`);
    return Object.freeze({ kind: 'sized', maxTransitions: max });
  }
  if (lowered.startsWith('chunked:')) {
    const rest = trimmed.slice('chunked:'.length);
    const colon = rest.indexOf(':');
    if (colon === -1) throw new Error(`invalid chunked retention spec '${spec}'`);
    const chunkSize = parseU64(rest.slice(0, colon));
    if (chunkSize === null || chunkSize === 0n) {
      throw new Error(`invalid chunked size in '${spec}'`);
    }
    const archiveDirectory = rest.slice(colon + 1);
    if (archiveDirectory === '') throw new Error(`invalid chunked retention spec '${spec}'`);
    return Object.freeze({ kind: 'chunked', chunkSize, archiveDirectory });
  }
  throw new Error(`unknown retention spec '${spec}'`);
}

/** A doublet state of a transition. */
export const doublet = (index, source, target) => link(index, source, target);

/** The all-zero doublet of the missing side of a create or delete. */
export const EMPTY_DOUBLET = doublet(0, 0, 0);

export const sameDoublet = (left, right) =>
  left.index === right.index && left.source === right.source && left.target === right.target;

const isEmptyDoublet = (value) => sameDoublet(value, EMPTY_DOUBLET);

const serializeDoublet = (value) => `${value.index},${value.source},${value.target}`;

/** A complete journal record: a row transition plus optional name changes. */
export function journalRecord(transition, nameOnly = false, names = []) {
  return {
    transactionId: transition.transactionId,
    sequence: transition.sequence,
    timestampMs: transition.timestampMs,
    kind: transition.kind,
    before: transition.before,
    after: transition.after,
    nameOnly,
    names,
  };
}

/** The row transition of a record. */
export const transitionOf = (record) => ({
  transactionId: record.transactionId,
  sequence: record.sequence,
  timestampMs: record.timestampMs,
  kind: record.kind,
  before: record.before,
  after: record.after,
});

/** The v1 entry of a transition (`GenericTransition::serialize`). */
export function serializeTransition(transition) {
  return [
    'v1',
    hex32(transition.transactionId),
    transition.sequence,
    transition.timestampMs,
    transition.kind,
    serializeDoublet(transition.before),
    serializeDoublet(transition.after),
  ].join('|');
}

/**
 * `GenericTransition::parse`: a malformed entry is an invalid format, a
 * well-formed one whose addresses do not fit is an address overflow.
 */
export function parseTransition(text) {
  const invalid = () => new InvalidFormatError(`malformed transition entry '${text}'`);
  const parts = text.split('|');
  if (parts.length < 7 || parts[0] !== 'v1') throw invalid();
  const transactionId = parseU128Hex(parts[1]);
  const sequence = parseI64(parts[2]);
  const timestampMs = parseI64(parts[3]);
  const kind = parseU8(parts[4]);
  if (transactionId === null || sequence === null || timestampMs === null || kind === null) {
    throw invalid();
  }
  if (kind > 2n) throw invalid();
  const parseDoublet = (value) => {
    const fields = value.split(',');
    if (fields.length !== 3) throw invalid();
    const addresses = fields.map((field) => {
      const address = parseAddress(field);
      if (address === null) throw invalid();
      return address;
    });
    return doublet(...addresses);
  };
  return {
    transactionId,
    sequence,
    timestampMs,
    kind: Number(kind),
    before: parseDoublet(parts[5]),
    after: parseDoublet(parts[6]),
  };
}

/** A name as UTF-16 units in upper-case hex; `-` when absent. */
export function encodeName(name) {
  if (name === null) return '-';
  let text = '';
  for (let i = 0; i < name.length; i++) {
    text += name.charCodeAt(i).toString(16).toUpperCase().padStart(4, '0');
  }
  return text;
}

/** Whether `text` holds no lone surrogate (`String::from_utf16` succeeds). */
function isWellFormed(text) {
  for (let i = 0; i < text.length; i++) {
    const unit = text.charCodeAt(i);
    if (unit >= 0xdc00 && unit <= 0xdfff) return false;
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = text.charCodeAt(i + 1);
      if (!(next >= 0xdc00 && next <= 0xdfff)) return false;
      i++;
    }
  }
  return true;
}

/** UTF-16 hex to text, `undefined` when the text is not a valid encoding. */
export function decodeUtf16Hex(text) {
  if (text.length % 4 !== 0 || !/^[0-9a-fA-F]*$/.test(text)) return undefined;
  let decoded = '';
  for (let i = 0; i < text.length; i += 4) {
    decoded += String.fromCharCode(Number.parseInt(text.slice(i, i + 4), 16));
  }
  return isWellFormed(decoded) ? decoded : undefined;
}

/** `decode_name`: `null` for `-`, the name, or `undefined` when invalid. */
export const decodeName = (text) => (text === '-' ? null : decodeUtf16Hex(text));

/** `JournalRecord::serialize`: v1 for a row-only record, v2 with names. */
export function serializeRecord(record) {
  const row = serializeTransition(record);
  if (!record.nameOnly && record.names.length === 0) return row;
  const names = record.names
    .map((name) => `${name.index},${encodeName(name.before)},${encodeName(name.after)}`)
    .join(';');
  return `v2${row.slice(2)}|${record.nameOnly ? 'n' : 'r'}|${names}`;
}

/** `JournalRecord::validate`: the name state a record may hold. */
export function validateRecord(record) {
  const invalid = () => new InvalidFormatError('invalid name state in transaction record');
  const names = record.names;
  for (let i = 1; i < names.length; i++) {
    if (names[i - 1].index >= names[i].index) throw invalid();
  }
  if (record.nameOnly) {
    if (
      names.length === 0 ||
      record.kind !== TransitionKind.Update ||
      !isEmptyDoublet(record.before) ||
      !isEmptyDoublet(record.after)
    ) {
      throw invalid();
    }
  } else if (names.length > 0) {
    const index = record.after.index !== 0 ? record.after.index : record.before.index;
    const shape = [
      isEmptyDoublet(record.before) && record.after.index !== 0,
      record.before.index !== 0 && record.before.index === record.after.index,
      record.before.index !== 0 && isEmptyDoublet(record.after),
    ][record.kind];
    if (!shape || names.length !== 1 || names[0].index !== index || index === 0) throw invalid();
  }
}

/** `JournalRecord::parse`: a v1 transition, or a canonical v2 record. */
export function parseRecord(text) {
  if (!text.startsWith('v2|')) return journalRecord(parseTransition(text));
  const invalid = () => new InvalidFormatError('malformed complete transaction record');
  const parts = text.split('|');
  if (parts.length !== 9) throw invalid();
  const transition = parseTransition(`v1|${parts.slice(1, 7).join('|')}`);
  if (transition.timestampMs < MIN_RECORD_MS || transition.timestampMs > MAX_RECORD_MS) {
    throw invalid();
  }
  if (parts[7] !== 'n' && parts[7] !== 'r') throw invalid();
  const names = parts[8].split(';').map((change) => {
    const fields = change.split(',');
    if (fields.length !== 3) throw invalid();
    const index = parseAddress(fields[0]);
    if (index === null) throw invalid();
    const before = decodeName(fields[1]);
    const after = decodeName(fields[2]);
    if (before === undefined || after === undefined) throw invalid();
    return { index, before, after };
  });
  const record = journalRecord(transition, parts[7] === 'n', names);
  validateRecord(record);
  if (serializeRecord(record) !== text) throw invalid();
  return record;
}

/** Whether two records are equal (the encoding is canonical). */
export const sameRecord = (left, right) => serializeRecord(left) === serializeRecord(right);

/** `UndoImage::serialize`: rows and names, `-` for an image without names. */
export function serializeImage(image) {
  const rows = image.rows.map(serializeDoublet).join(';');
  const names =
    image.names === null
      ? '-'
      : [...image.names].map(([index, name]) => `${index},${encodeName(name)}`).join(';');
  return `v3|undo|${hex32(image.owner)}|${image.base}|${rows}|${names}`;
}

/** `UndoImage::parse`: a canonical write-ahead undo image. */
export function parseImage(text) {
  const invalid = () => new InvalidFormatError('invalid write-ahead undo image');
  const parts = text.split('|');
  if (parts.length !== 6 || parts[0] !== 'v3' || parts[1] !== 'undo') throw invalid();
  const owner = parseU128Hex(parts[2]);
  const base = parseI64(parts[3]);
  if (owner === null || base === null || owner === 0n || base < 0n) throw invalid();
  const address = (value) => {
    const parsed = parseAddress(value);
    if (parsed === null) throw invalid();
    return parsed;
  };
  const rows = [];
  if (parts[4] !== '') {
    for (const value of parts[4].split(';')) {
      const fields = value.split(',');
      if (fields.length !== 3) throw invalid();
      const row = doublet(address(fields[0]), address(fields[1]), address(fields[2]));
      if (row.index === 0 || (rows.length > 0 && rows.at(-1).index >= row.index)) {
        throw invalid();
      }
      rows.push(row);
    }
  }
  let names = null;
  if (parts[5] !== '-') {
    names = new Map();
    let last = null;
    if (parts[5] !== '') {
      for (const value of parts[5].split(';')) {
        const comma = value.indexOf(',');
        if (comma === -1) throw invalid();
        const index = address(value.slice(0, comma));
        const name = decodeName(value.slice(comma + 1));
        if (name == null || (last !== null && last >= index)) throw invalid();
        names.set(index, name);
        last = index;
      }
    }
  }
  const image = { owner, base, rows, names };
  if (serializeImage(image) !== text) throw invalid();
  return image;
}

/**
 * `Path::file_stem` and `Path::parent` of a Unix path, as Rust computes
 * them: `{ stem, parent }`, each `null` when Rust has none.
 */
function stemAndParent(path) {
  const rooted = path.startsWith('/');
  const components = [];
  let start = 0;
  for (const segment of path.split('/')) {
    const leadingCurrent = segment === '.' && components.length === 0 && !rooted;
    if (segment !== '' && (segment !== '.' || leadingCurrent)) {
      components.push({ segment, end: start + segment.length });
    }
    start += segment.length + 1;
  }
  if (components.length === 0) return { stem: null, parent: null };
  const last = components.pop().segment;
  const parent = components.length > 0 ? path.slice(0, components.at(-1).end) : rooted ? '/' : '';
  if (last === '.' || last === '..') return { stem: null, parent };
  const dot = last.lastIndexOf('.');
  return { stem: dot <= 0 ? last : last.slice(0, dot), parent };
}

/** `<parent>/<stem><suffix>` beside the database, or just the name. */
export function sidecarPath(databasePath, suffix) {
  const { stem, parent } = stemAndParent(databasePath);
  const name = `${stem ?? ''}${suffix}`;
  if (parent === null || parent === '') return name;
  return parent.endsWith('/') ? `${parent}${name}` : `${parent}/${name}`;
}

/** `make_transitions_database_filename`: `<stem>.transitions.links`. */
export const makeTransitionsDatabaseFilename = (databasePath) =>
  sidecarPath(databasePath, '.transitions.links');
