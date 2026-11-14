// Ports the byte-level helpers of the Rust links-notation 0.31.5 crate that the
// text parser uses: src/quotes.rs (delimited references with any number of
// delimiters), src/reference_literal.rs (`~1{hex}` reference literals),
// src/comments.rs (blanking `#` line comments) and `char::escape_debug`, which
// the parse error message prints the offending character with.
//
// The Rust parser reads UTF-8 bytes, so this port does too: every position is a
// byte offset into the UTF-8 encoding of the document, as in the Rust crate.
// Every byte with a meaning to the grammar is ASCII, so no position ever falls
// inside a multi-byte character.

export const SPACE = 0x20;
export const TAB = 0x09;
export const LF = 0x0a;
export const CR = 0x0d;
export const OPEN = 0x28;
export const CLOSE = 0x29;
export const COLON = 0x3a;
const HASH = 0x23;
const TILDE = 0x7e;
const LEFT_BRACE = 0x7b;
const RIGHT_BRACE = 0x7d;

/** `QUOTES`: the three delimiters a reference can be written between. */
const QUOTES = [0x22, 0x27, 0x60];

const encoder = new TextEncoder();
// `ignoreBOM` keeps a leading U+FEFF as the character it is, as Rust does.
const decoder = new TextDecoder('utf-8', { ignoreBOM: true });
const strictDecoder = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });

/** The UTF-8 bytes of `text`. */
export function encodeUtf8(text) {
  return encoder.encode(text);
}

/** The text of `bytes[start..end]`. */
export function decodeUtf8(bytes, start, end) {
  return decoder.decode(bytes.subarray(start, end));
}

/** `is_whitespace_char`: the four characters the grammar treats as blank. */
export function isWhitespaceByte(byte) {
  return byte === SPACE || byte === TAB || byte === LF || byte === CR;
}

/** `is_reference_char`: anything but blanks, `(`, `:` and `)`. */
export function isReferenceByte(byte) {
  return !isWhitespaceByte(byte) && byte !== OPEN && byte !== COLON && byte !== CLOSE;
}

function isDigit(byte) {
  return byte >= 0x30 && byte <= 0x39;
}

function hexValue(byte) {
  if (isDigit(byte)) return byte - 0x30;
  const lower = byte | 0x20;
  return lower >= 0x61 && lower <= 0x66 ? lower - 0x61 + 10 : -1;
}

/**
 * `reference_literal::prefix_end`: the offset just past `~N{` when the bytes at
 * `start` open a reference literal, or -1.
 */
export function prefixEnd(bytes, start) {
  if (bytes[start] !== TILDE) return -1;
  let position = start + 1;
  while (isDigit(bytes[position])) position += 1;
  return position > start + 1 && bytes[position] === LEFT_BRACE ? position + 1 : -1;
}

/**
 * `reference_literal::reference_literal_end`: the offset just past the `}` of
 * the reference literal at `start`, or -1 when none is written there.
 */
export function referenceLiteralEnd(bytes, start) {
  const body = prefixEnd(bytes, start);
  if (body < 0) return -1;
  let end = body;
  while (end < bytes.length && !LITERAL_STOPS.includes(bytes[end])) end += 1;
  return bytes[end] === RIGHT_BRACE ? end + 1 : -1;
}

const LITERAL_STOPS = [RIGHT_BRACE, SPACE, TAB, CR, LF, OPEN, CLOSE];

/**
 * `decode_reference_literal` for the literal `bytes[start..end]`, whose `~N{`
 * prefix ends at `prefix` and whose last byte is its `}`: the text it encodes,
 * or null where Rust returns an error. Only version 1 is supported, and its
 * body is an even number of hex digits that encode UTF-8.
 */
export function decodeReferenceLiteral(bytes, start, prefix, end) {
  if (prefix - start !== 3 || bytes[start + 1] !== 0x31) return null;
  const digits = end - 1 - prefix;
  if (digits % 2 !== 0) return null;
  const decoded = new Uint8Array(digits / 2);
  for (let index = 0; index < decoded.length; index += 1) {
    const high = hexValue(bytes[prefix + 2 * index]);
    const low = hexValue(bytes[prefix + 2 * index + 1]);
    if (high < 0 || low < 0) return null;
    decoded[index] = high * 16 + low;
  }
  try {
    return strictDecoder.decode(decoded);
  } catch {
    return null;
  }
}

/**
 * `DelimiterRuns`: every run of one delimiter in the document, and for each
 * run the next run that is longer than it, so that finding where a reference
 * closes skips the runs too short to close it.
 */
class DelimiterRuns {
  constructor(bytes, quote) {
    this.starts = [];
    this.lengths = [];
    let position = bytes.indexOf(quote);
    while (position >= 0) {
      const length = runLength(bytes, position, quote, bytes.length);
      this.starts.push(position);
      this.lengths.push(length);
      position = bytes.indexOf(quote, position + length);
    }
    const count = this.starts.length;
    this.nextLonger = new Array(count).fill(count);
    const longer = [];
    for (let run = count - 1; run >= 0; run -= 1) {
      while (longer.length > 0 && this.lengths[longer[longer.length - 1]] <= this.lengths[run]) {
        longer.pop();
      }
      if (longer.length > 0) this.nextLonger[run] = longer[longer.length - 1];
      longer.push(run);
    }
  }

  get count() {
    return this.starts.length;
  }

  end(run) {
    return this.starts[run] + this.lengths[run];
  }

  /** The run that holds `position`: the last one starting at or before it. */
  holding(position) {
    let low = 0;
    let high = this.starts.length;
    while (low < high) {
      const middle = (low + high) >> 1;
      if (this.starts[middle] <= position) low = middle + 1;
      else high = middle;
    }
    return Math.max(low - 1, 0);
  }
}

function runLength(bytes, start, quote, end) {
  let position = start;
  while (position < end && bytes[position] === quote) position += 1;
  return position - start;
}

/**
 * `DelimitedReferences`: reads the delimited reference that opens at any
 * offset of the document as written, remembering every reading.
 */
export class DelimitedReferences {
  constructor(bytes) {
    this.bytes = bytes;
    this.runs = new Map(QUOTES.map((quote) => [quote, new DelimiterRuns(bytes, quote)]));
    this.readings = new Map();
  }

  /** The `{ value, length }` read at `start`, or null when none opens there. */
  read(start) {
    let reading = this.readings.get(start);
    if (reading === undefined) {
      reading = this.readAt(start);
      this.readings.set(start, reading);
    }
    return reading;
  }

  /**
   * A run of N delimiters opens a reference that the first later run of R
   * delimiters closes, where R >= N and R / N is odd.
   */
  readAt(start) {
    const quote = this.bytes[start];
    const runs = this.runs.get(quote);
    if (runs === undefined) return null;
    const opening = runs.holding(start);
    const count = runs.end(opening) - start;
    let closing = opening + 1;
    while (closing < runs.count) {
      const length = runs.lengths[closing];
      if (length < count) closing = runs.nextLonger[closing];
      else if (Math.floor(length / count) % 2 === 1) break;
      else closing += 1;
    }
    const end = closing < runs.count ? runs.end(closing) - start : null;
    return reading(this.bytes, start, quote, count, end);
  }
}

/** `quotes::reading`: decodes the body of a reference opened by `count` delimiters. */
function reading(bytes, start, quote, count, end) {
  const empty = count % 2 === 0 ? { value: '', length: count } : null;
  if (end === null) return empty;
  const body = decodeUtf8(bytes, start + count, start + end);
  const delimiter = String.fromCharCode(quote);
  let value = '';
  let position = 0;
  let found = body.indexOf(delimiter);
  while (found >= 0) {
    let length = 1;
    while (body[found + length] === delimiter) length += 1;
    const escaped = Math.floor(length / (2 * count)) * count;
    const closes = found + length === body.length ? count : 0;
    value += body.slice(position, found) + delimiter.repeat(length - escaped - closes);
    position = found + length;
    found = body.indexOf(delimiter, position);
  }
  if (count % 2 === 0 && !isSubstantiveBody(value)) return empty;
  return { value, length: end };
}

/** Balanced parentheses that never close more than they opened, and something visible. */
function isSubstantiveBody(content) {
  let depth = 0;
  let visible = false;
  for (const character of content) {
    if (character === '(') depth += 1;
    else if (character === ')') {
      depth -= 1;
      if (depth < 0) return false;
    }
    if (!isWhitespaceByte(character.charCodeAt(0))) visible = true;
  }
  return visible && depth === 0;
}

const SEPARATORS = [SPACE, TAB, LF, CR, OPEN, COLON, CLOSE];
const BEFORE_COMMENT = [SPACE, TAB, LF, CR];

/**
 * `comments::strip_comments`: a copy of the document with every `#` comment
 * blanked to spaces, byte for byte. A `#` starts a comment at the start of the
 * document or after a blank; a delimited reference or reference literal that
 * starts a token is skipped whole, so a `#` inside one stays content.
 */
export function stripComments(document, references) {
  const bytes = document.slice();
  let position = 0;
  let tokenStart = true;
  while (position < bytes.length) {
    const byte = bytes[position];
    if (byte === HASH && (position === 0 || BEFORE_COMMENT.includes(bytes[position - 1]))) {
      while (position < bytes.length && bytes[position] !== LF && bytes[position] !== CR) {
        bytes[position] = SPACE;
        position += 1;
      }
      continue;
    }
    if (SEPARATORS.includes(byte)) {
      tokenStart = true;
      position += 1;
      continue;
    }
    if (tokenStart) {
      const end = delimitedTokenEnd(document, position, references);
      if (end >= 0) {
        position = end;
        continue;
      }
    }
    tokenStart = false;
    position += 1;
  }
  return bytes;
}

function delimitedTokenEnd(document, start, references) {
  const literal = referenceLiteralEnd(document, start);
  if (literal >= 0) return literal;
  const quoted = QUOTES.includes(document[start]) ? references.read(start) : null;
  return quoted === null ? -1 : start + quoted.length;
}

/** Whether `bytes[position]` opens a delimited reference. */
export function isQuote(byte) {
  return QUOTES.includes(byte);
}

const ESCAPED = new Map([
  ['\0', '\\0'],
  ['\t', '\\t'],
  ['\r', '\\r'],
  ['\n', '\\n'],
  ['\\', '\\\\'],
  ['"', '\\"'],
  ["'", "\\'"],
]);

// Rust escapes what it does not consider printable and every grapheme extender.
// Unicode general categories approximate its printable table: control, format,
// private use, unassigned and surrogate code points and every separator but the
// space.
const UNPRINTABLE = /^(?:[\p{Cc}\p{Cf}\p{Co}\p{Cn}\p{Cs}\p{Grapheme_Extend}]|(?! )\p{Z})$/u;

/** `char::escape_debug` of the one character `character`. */
export function escapeDebug(character) {
  const escaped = ESCAPED.get(character);
  if (escaped !== undefined) return escaped;
  if (UNPRINTABLE.test(character)) return `\\u{${character.codePointAt(0).toString(16)}}`;
  return character;
}
