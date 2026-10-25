// Canonical LiNo text: the document model shared by the text protocol of
// `clink --serve` and the WebAssembly `Clink.request` entry point.
//
// This is a port of `links_notation::binary::{format_document, parse_document}`
// (links-notation 0.26.0, the version both link-cli ports pin), restricted to
// what the links protocol exchanges: one top-level link per line, nested
// groups in parentheses, `id:` names and references. A reference is a JS
// string, a link is `{ id: string | null, values: (string | link)[] }`.

import { MalformedMessageError } from './errors.js';

const QUOTES = ["'", '"', '`'];
// Rust's `char::is_whitespace` (Unicode White_Space) plus the characters the
// formatter refuses to leave bare.
const NEEDS_QUOTES = /[\t\n\v\f\r \u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000\ufeff()'"`:]/u;
// Control characters cannot be written as text at all.
const CONTROL = /[\u0000-\u001f\u007f]/u;
const LITERAL_PREFIX = /^~[0-9]+\{/;
const encoder = new TextEncoder();
const decoder = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });

/** A link without an id: `(a b)`. */
export function group(values) {
  return { id: null, values };
}

/** A link with an id: `(id: a b)`. */
export function named(id, values) {
  return { id, values };
}

/** `~1{hex}`: the lossless literal for any text, control characters included. */
export function encodeReferenceLiteral(text) {
  let hex = '';
  for (const byte of encoder.encode(text)) hex += byte.toString(16).padStart(2, '0');
  return `~1{${hex}}`;
}

/** The text of a `~1{hex}` literal. */
export function decodeReferenceLiteral(literal) {
  const match = /^~([0-9]+)\{([0-9a-fA-F]*)\}$/.exec(literal);
  if (!match || match[1] !== '1') {
    throw new MalformedMessageError(`invalid or unsupported reference literal: ${literal}`);
  }
  const hex = match[2];
  if (hex.length % 2 !== 0) {
    throw new MalformedMessageError('reference literal requires an even number of hex digits');
  }
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < bytes.length; i += 1) bytes[i] = Number.parseInt(hex.slice(2 * i, 2 * i + 2), 16);
  try {
    return decoder.decode(bytes);
  } catch {
    throw new MalformedMessageError('reference literal must contain valid UTF-8');
  }
}

/** Writes a reference so that it parses back as exactly `text`. */
export function formatReference(text) {
  if (typeof text !== 'string') throw new TypeError(`a reference is a string, got ${typeof text}`);
  if (text === '' || CONTROL.test(text)) return encodeReferenceLiteral(text);
  if (!LITERAL_PREFIX.test(text) && !text.startsWith('#') && !NEEDS_QUOTES.test(text)) return text;
  const first = text[0];
  let best = null;
  for (const quote of QUOTES) {
    if (quote === first) continue;
    let longest = 0;
    let current = 0;
    for (const c of text) {
      current = c === quote ? current + 1 : 0;
      if (current > longest) longest = current;
    }
    const count = (longest + 1) | 1;
    if (best === null || count < best.count) best = { quote, count };
  }
  const delimiter = best.quote.repeat(best.count);
  return `${delimiter}${text}${delimiter}`;
}

/** Formats one link as it appears nested inside another link. */
export function formatLink(link) {
  if (typeof link === 'string') return formatReference(link);
  const { id, values } = link;
  if (id === null || id === undefined) {
    if (values.length === 1 && typeof values[0] === 'string') return `((${formatReference(values[0])}))`;
    return `(${joinValues(values)})`;
  }
  return values.length === 0 ? `(${formatReference(id)}:)` : `(${formatReference(id)}: ${joinValues(values)})`;
}

function joinValues(values) {
  return values.map(formatLink).join(' ');
}

/**
 * Formats a document as canonical LiNo text, one top-level link per line. An
 * unnamed top-level link with at least two values loses its parentheses.
 */
export function formatDocument(document) {
  return document
    .map((link) =>
      typeof link !== 'string' && (link.id === null || link.id === undefined) && link.values.length >= 2
        ? joinValues(link.values)
        : formatLink(link),
    )
    .join('\n');
}

/**
 * An unnamed group holding exactly one reference is that reference, so `a`
 * and `(a)` both parse as `a`; applied from the outside in, like
 * `links_notation::binary::canonical`.
 */
function canonical(link) {
  if (typeof link === 'string') return link;
  if (link.id === null && link.values.length === 1 && typeof link.values[0] === 'string') return link.values[0];
  return { id: link.id, values: link.values.map(canonical) };
}

const isSpace = (c) => c === ' ' || c === '\t';
const isLineBreak = (c) => c === '\n' || c === '\r';
const endsBareWord = (c) => c === undefined || isSpace(c) || isLineBreak(c) || c === '(' || c === ')' || c === ':';

function runLength(text, start, quote) {
  let end = start;
  while (text[end] === quote) end += 1;
  return end - start;
}

/** Whether an even-run body holds something visible and balanced parentheses. */
function isSubstantiveBody(content) {
  let depth = 0;
  let visible = false;
  for (const c of content) {
    if (c === '(') depth += 1;
    if (c === ')') {
      depth -= 1;
      if (depth < 0) return false;
    }
    if (!isSpace(c) && !isLineBreak(c)) visible = true;
  }
  return visible && depth === 0;
}

/**
 * Reads the delimited reference opened at `start`: a run of N quotes opens it,
 * a run of R closes it when `R / N` is odd (its last N close it), and each 2N
 * inside are N literal quotes. An even opening run with no substantive body is
 * the empty reference.
 */
function readQuoted(text, start) {
  const quote = text[start];
  const count = runLength(text, start, quote);
  let position = start + count;
  let end = -1;
  for (;;) {
    const found = text.indexOf(quote, position);
    if (found === -1) break;
    const length = runLength(text, found, quote);
    position = found + length;
    if (length >= count && Math.floor(length / count) % 2 === 1) {
      end = position;
      break;
    }
  }
  const empty = count % 2 === 0 ? { value: '', end: start + count } : null;
  if (end === -1) {
    if (empty) return empty;
    throw new MalformedMessageError(`unterminated quoted reference at offset ${start}`);
  }
  const body = text.slice(start + count, end);
  let value = '';
  let at = 0;
  for (;;) {
    const found = body.indexOf(quote, at);
    if (found === -1) break;
    const length = runLength(body, found, quote);
    const escaped = Math.floor(length / (2 * count)) * count;
    const closes = found + length === body.length ? count : 0;
    value += body.slice(at, found) + quote.repeat(length - escaped - closes);
    at = found + length;
  }
  value += body.slice(at);
  if (empty && !isSubstantiveBody(value)) return empty;
  return { value, end };
}

class Reader {
  constructor(text) {
    this.text = text;
    this.at = 0;
    this.depth = 0;
  }

  fail(expected) {
    const found = this.at < this.text.length ? JSON.stringify(this.text.slice(this.at, this.at + 20)) : 'end of input';
    throw new MalformedMessageError(`invalid LiNo at offset ${this.at}: expected ${expected}, found ${found}`);
  }

  /** Skips blanks; inside parentheses line breaks are blanks too. */
  skipSpace() {
    for (;;) {
      const c = this.text[this.at];
      if (isSpace(c) || (this.depth > 0 && isLineBreak(c))) this.at += 1;
      else return;
    }
  }

  reference() {
    const { text } = this;
    const c = text[this.at];
    if (LITERAL_PREFIX.test(text.slice(this.at, this.at + 24))) {
      const close = text.indexOf('}', this.at);
      if (close === -1) this.fail('a reference literal closed by "}"');
      const literal = text.slice(this.at, close + 1);
      this.at = close + 1;
      return decodeReferenceLiteral(literal);
    }
    if (QUOTES.includes(c)) {
      const { value, end } = readQuoted(text, this.at);
      this.at = end;
      return value;
    }
    const start = this.at;
    while (!endsBareWord(text[this.at])) this.at += 1;
    if (this.at === start) this.fail('a reference');
    return text.slice(start, this.at);
  }

  /** `(id: values)` or `(values)`; the reader stands on `(`. */
  parenthesized() {
    this.at += 1;
    this.depth += 1;
    this.skipSpace();
    let id = null;
    const values = [];
    if (this.text[this.at] !== '(' && this.text[this.at] !== ')') {
      const first = this.reference();
      this.skipSpace();
      if (this.text[this.at] === ':') {
        this.at += 1;
        id = first;
      } else {
        values.push(first);
      }
    }
    for (;;) {
      this.skipSpace();
      const c = this.text[this.at];
      if (c === ')') break;
      if (c === undefined) this.fail('")"');
      values.push(this.value());
    }
    this.at += 1;
    this.depth -= 1;
    return { id, values };
  }

  value() {
    return this.text[this.at] === '(' ? this.parenthesized() : this.reference();
  }

  /** One top-level line: `link`, `a b c` or `id: a b`. */
  line() {
    const values = [];
    let id = null;
    for (;;) {
      this.skipSpace();
      const c = this.text[this.at];
      if (c === undefined || isLineBreak(c)) break;
      if (c === ')') this.fail('a link or a reference');
      if (c === ':' && id === null && values.length === 1 && typeof values[0] === 'string') {
        this.at += 1;
        id = values.pop();
        continue;
      }
      values.push(this.value());
    }
    if (id === null && values.length === 1) return values[0];
    return { id, values };
  }
}

/**
 * Parses LiNo text into a canonical document. Blank input is the empty
 * document, as in `links_notation::binary::parse_document`.
 */
export function parseDocument(text) {
  if (typeof text !== 'string') throw new TypeError('LiNo text must be a string');
  const reader = new Reader(text);
  const document = [];
  while (reader.at < text.length) {
    reader.skipSpace();
    const c = text[reader.at];
    if (c === undefined) break;
    if (isLineBreak(c)) {
      reader.at += 1;
      continue;
    }
    document.push(canonical(reader.line()));
  }
  return document;
}
