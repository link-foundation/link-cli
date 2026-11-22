// Ports rust/src/protocol/ipc/json.rs: the flat JSON objects of the
// rendezvous files, with string and non-negative integer (u64) members;
// members of other types are skipped. The parser works on UTF-8 byte offsets,
// so its error texts name the same offsets as the Rust ones. Every error is an
// `InvalidData` IoError. Numbers are BigInts (u64).
//
// Rust's "a JSON string is not UTF-8" cannot happen here: the input is a
// JavaScript string, and `readObject` refuses a file that is not UTF-8 first,
// as Rust's `fs::read_to_string` does.
import fs from 'node:fs';
import { IoError } from './protocol-errors.js';

const invalid = (message) => new IoError('InvalidData', message);
const U64_MAX = (1n << 64n) - 1n;
const byteOrder = (a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b));
const SPACE = new Set([0x20, 0x09, 0x0a, 0x0d]);
const isDigit = (byte) => byte !== undefined && byte >= 0x30 && byte <= 0x39;
const SKIP = Symbol('skip');

/** Writes `text` as a JSON string, escaping as json.rs does. */
function writeString(text) {
  let out = '"';
  for (const character of text) {
    const code = character.codePointAt(0);
    if (character === '"') out += '\\"';
    else if (character === '\\') out += '\\\\';
    else if (character === '\n') out += '\\n';
    else if (character === '\r') out += '\\r';
    else if (character === '\t') out += '\\t';
    else if (code < 0x20) out += `\\u${code.toString(16).padStart(4, '0')}`;
    else out += character;
  }
  return `${out}"`;
}

/** A flat JSON object: a sorted map of strings and u64 BigInts. */
export class JsonObject {
  #members;

  constructor(members = new Map()) {
    this.#members = members;
  }

  /** A copy with `name` set to a string or a whole number. */
  with(name, value) {
    const members = new Map(this.#members);
    members.set(name, typeof value === 'string' ? value : BigInt(value));
    return new JsonObject(members);
  }

  /** The string member `name`, or `undefined`. */
  text(name) {
    const value = this.#members.get(name);
    return typeof value === 'string' ? value : undefined;
  }

  /** The number member `name` as a BigInt, or `undefined`. */
  number(name) {
    const value = this.#members.get(name);
    return typeof value === 'bigint' ? value : undefined;
  }

  /** The number member `name`; its absence is an error of the handshake. */
  requireNumber(name) {
    const value = this.number(name);
    if (value === undefined) throw invalid(`the handshake lacks the number \`${name}\``);
    return value;
  }

  /** The members, sorted by byte. */
  entries() {
    return [...this.#members].sort(([a], [b]) => byteOrder(a, b));
  }

  /** The JSON text, members sorted by byte as a Rust `BTreeMap` iterates. */
  toJson() {
    const members = this.entries().map(
      ([name, value]) =>
        `${writeString(name)}:${typeof value === 'string' ? writeString(value) : value}`,
    );
    return `{${members.join(',')}}`;
  }

  /** Parses a JSON object; members that are not strings or u64 numbers are skipped. */
  static parse(text) {
    const parser = new Parser(text);
    parser.space();
    const object = parser.object();
    parser.space();
    if (parser.at !== parser.bytes.length) {
      throw invalid('trailing characters after the JSON object');
    }
    return object;
  }
}

/** Parses a JSON array of strings (an `exec:` command line). */
export function parseStrings(text) {
  const parser = new Parser(text);
  parser.expect(0x5b);
  const strings = [];
  parser.space();
  if (parser.peek() === 0x5d) {
    parser.at += 1;
  } else {
    for (;;) {
      parser.space();
      strings.push(parser.string());
      parser.space();
      const next = parser.peek();
      parser.at += 1;
      if (next === 0x5d) break;
      if (next !== 0x2c) throw invalid('expected `,` or `]` in a JSON array');
    }
  }
  parser.space();
  if (parser.at !== parser.bytes.length) {
    throw invalid('trailing characters after the JSON array');
  }
  return strings;
}

/** Reads a JSON object file; a file that is not UTF-8 fails as `read_to_string` does. */
export function readObject(file) {
  const bytes = fs.readFileSync(file);
  let text;
  try {
    text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes);
  } catch {
    throw invalid('stream did not contain valid UTF-8');
  }
  return JsonObject.parse(text);
}

class Parser {
  constructor(text) {
    this.bytes = Buffer.from(text, 'utf8');
    this.at = 0;
  }

  peek() {
    return this.bytes[this.at];
  }

  space() {
    while (SPACE.has(this.peek())) this.at += 1;
  }

  expect(byte) {
    this.space();
    if (this.peek() !== byte) {
      throw invalid(
        `expected \`${String.fromCharCode(byte)}\` at offset ${this.at} of the JSON object`,
      );
    }
    this.at += 1;
  }

  /** Reads `}` or `,` after a member; true at the end of the object. */
  #objectEnd(message) {
    this.space();
    const next = this.peek();
    if (next !== 0x2c && next !== 0x7d) throw invalid(message);
    this.at += 1;
    return next === 0x7d;
  }

  object() {
    this.expect(0x7b);
    const members = new Map();
    this.space();
    if (this.peek() === 0x7d) {
      this.at += 1;
      return new JsonObject(members);
    }
    do {
      this.space();
      const name = this.string();
      this.expect(0x3a);
      const value = this.value(0);
      if (value !== SKIP) members.set(name, value);
    } while (!this.#objectEnd('expected `,` or `}` in the JSON object'));
    return new JsonObject(members);
  }

  objectSkip(depth) {
    this.expect(0x7b);
    this.space();
    if (this.peek() === 0x7d) {
      this.at += 1;
      return;
    }
    do {
      this.space();
      this.string();
      this.expect(0x3a);
      this.value(depth);
    } while (!this.#objectEnd('expected `,` or `}` in a JSON object'));
  }

  value(depth) {
    if (depth > 32) throw invalid('the JSON object nests too deeply');
    this.space();
    const next = this.peek();
    if (next === 0x22) return this.string();
    if (isDigit(next)) {
      const start = this.at;
      while (isDigit(this.peek())) this.at += 1;
      const after = this.peek();
      if (after === 0x2e || after === 0x65 || after === 0x45) {
        this.numberTail();
        return SKIP;
      }
      const number = BigInt(this.bytes.toString('latin1', start, this.at));
      return number <= U64_MAX ? number : SKIP;
    }
    if (next === 0x2d) {
      this.at += 1;
      this.numberTail();
      return SKIP;
    }
    if (next === 0x74) return this.word('true');
    if (next === 0x66) return this.word('false');
    if (next === 0x6e) return this.word('null');
    if (next === 0x5b) {
      this.at += 1;
      this.space();
      if (this.peek() === 0x5d) {
        this.at += 1;
        return SKIP;
      }
      for (;;) {
        this.value(depth + 1);
        this.space();
        const end = this.peek();
        this.at += 1;
        if (end === 0x5d) return SKIP;
        if (end !== 0x2c) throw invalid('expected `,` or `]` in a JSON array');
      }
    }
    if (next === 0x7b) {
      this.objectSkip(depth + 1);
      return SKIP;
    }
    throw invalid(`unexpected JSON at offset ${this.at}`);
  }

  numberTail() {
    while (/[0-9.eE+-]/.test(String.fromCharCode(this.peek() ?? 0))) this.at += 1;
  }

  word(word) {
    if (this.bytes.toString('latin1', this.at, this.at + word.length) !== word) {
      throw invalid(`unexpected JSON at offset ${this.at}`);
    }
    this.at += word.length;
    return SKIP;
  }

  string() {
    this.expect(0x22);
    let out = '';
    for (;;) {
      const start = this.at;
      while (this.at < this.bytes.length && this.peek() !== 0x22 && this.peek() !== 0x5c) {
        this.at += 1;
      }
      out += this.bytes.toString('utf8', start, this.at);
      const next = this.peek();
      if (next === undefined) throw invalid('unended JSON string');
      this.at += 1;
      if (next === 0x22) return out;
      const escape = this.peek();
      if (escape === undefined) throw invalid('unended escape');
      this.at += 1;
      out += this.#escape(escape);
    }
  }

  #escape(escape) {
    const simple = { 0x22: '"', 0x5c: '\\', 0x2f: '/', 0x62: '\b', 0x66: '\f' };
    Object.assign(simple, { 0x6e: '\n', 0x72: '\r', 0x74: '\t' });
    if (simple[escape] !== undefined) return simple[escape];
    if (escape !== 0x75) throw invalid('unknown JSON escape');
    const high = this.hex4();
    let code = high;
    if (
      high >= 0xd800 &&
      high < 0xdc00 &&
      this.bytes.toString('latin1', this.at, this.at + 2) === '\\u'
    ) {
      this.at += 2;
      const low = this.hex4();
      code = 0x10000 + ((high - 0xd800) << 10) + (((low - 0xdc00) >>> 0) & 0x3ff);
    }
    return code >= 0xd800 && code < 0xe000 ? '�' : String.fromCodePoint(code);
  }

  /** Four hexadecimal digits, as `u32::from_str_radix` reads them (a leading `+` is allowed). */
  hex4() {
    const digits = this.bytes.subarray(this.at, this.at + 4);
    const text = digits.toString('latin1');
    if (digits.length !== 4 || !/^\+?[0-9a-fA-F]+$/.test(text)) {
      throw invalid('bad `\\u` escape');
    }
    this.at += 4;
    return parseInt(text, 16);
  }
}
