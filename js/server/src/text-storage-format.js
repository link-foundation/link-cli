// The line grammar of a text links database (rust/src/text_storage_format.rs):
// rows `(index source target)` with an optional `"name"`, and the versioned
// `# link-cli:name:` comment for names that cannot sit on a row -- names of
// addresses without a row, and names holding control characters.
import { encodeReferenceLiteral } from './notation.js';
import { link } from './link.js';

export const NAME_HEADER = '# link-cli:name:';

// `char::is_whitespace`, which differs from the JavaScript `\s` class: U+0085
// is whitespace in Rust and U+FEFF is not.
const WHITESPACE =
  '\\t\\n\\v\\f\\r \\u0085\\u00a0\\u1680\\u2000-\\u200a\\u2028\\u2029\\u202f\\u205f\\u3000';
const LEADING = new RegExp(`^[${WHITESPACE}]+`);
const TRAILING = new RegExp(`[${WHITESPACE}]+$`);
const SEPARATOR = new RegExp(`[${WHITESPACE}]+`);
const FIRST_SPACE = new RegExp(`[${WHITESPACE}]`);

/** Rust `str::trim_start`. */
export const trimStart = (text) => text.replace(LEADING, '');

/** Rust `str::trim`. */
export const trim = (text) => trimStart(text).replace(TRAILING, '');

/** Rust `str::split_whitespace`. */
export function splitWhitespace(text) {
  const trimmed = trim(text);
  return trimmed === '' ? [] : trimmed.split(SEPARATOR);
}

/** Whether a name has to be written as metadata instead of on its row. */
export function needsNameMetadata(name) {
  for (const character of name) {
    if (character < ' ' || character === '\x7f') return true;
  }
  return false;
}

export function nameMetadata(id, name) {
  return `${NAME_HEADER}1 ${id} ${encodeReferenceLiteral(name)}`;
}

const utf8 = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });

/**
 * Rust `links_notation::decode_reference_literal`, with its error texts: the
 * links-notation package reports every failure as one message.
 */
export function decodeNameLiteral(literal) {
  const prefix = /^~[0-9]+\{/.exec(literal);
  if (prefix === null) throw new Error('invalid reference literal');
  if (prefix[0] !== '~1{' || !literal.endsWith('}')) {
    throw new Error('invalid or unsupported reference literal');
  }
  const hex = literal.slice(3, -1);
  if (hex.length % 2 !== 0 || !/^[0-9a-fA-F]*$/.test(hex)) {
    throw new Error('reference literal requires an even number of hex digits');
  }
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < bytes.length; i++) bytes[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  try {
    return utf8.decode(bytes);
  } catch {
    throw new Error('reference literal must contain valid UTF-8');
  }
}

/** `[id, name]` of the text after `NAME_HEADER`. */
export function parseNameMetadata(metadata) {
  const fields = splitWhitespace(metadata);
  if (fields.length !== 3 || fields[0] !== '1') {
    throw new Error('invalid or unsupported name metadata');
  }
  if (!/^[0-9]*$/.test(fields[1])) throw new Error('invalid name metadata address');
  const id = Number(fields[1]);
  if (id > 0xffffffff) throw new Error('number too large to fit in target type');
  return [id, decodeNameLiteral(fields[2])];
}

// Rust `str::parse::<u32>()` of a word that holds no whitespace.
function parseAddress(word) {
  if (!/^\+?[0-9]+$/.test(word)) return null;
  const value = Number(word);
  return value <= 0xffffffff ? value : null;
}

function takeWord(state) {
  state.rest = trimStart(state.rest);
  if (state.rest === '') return null;
  const end = state.rest.search(FIRST_SPACE);
  const word = end === -1 ? state.rest : state.rest.slice(0, end);
  state.rest = end === -1 ? '' : state.rest.slice(end);
  return word;
}

const takeAddress = (state) => {
  const word = takeWord(state);
  return word === null ? null : parseAddress(word);
};

/**
 * A row `(index source target)` or `(index source target "name")`:
 * `{ link, name }` with `name` null when the row has none, or `null` when the
 * line is not a row.
 */
export function parseLinkLine(line) {
  const state = { rest: line.replace(/^[()]+/, '').replace(/[()]+$/, '') };
  const index = takeAddress(state);
  if (index === null) return null;
  const source = takeAddress(state);
  if (source === null) return null;
  const target = takeAddress(state);
  if (target === null) return null;
  const tail = trim(state.rest);
  let name = null;
  if (tail.length >= 2 && tail.startsWith('"') && tail.endsWith('"')) {
    name = tail.slice(1, -1);
  } else if (tail !== '') {
    // The old reader's single-token unquoted spelling, without escapes.
    name = takeWord(state).replace(/^"+/, '').replace(/"+$/, '');
  }
  return { link: link(index, source, target), name };
}
