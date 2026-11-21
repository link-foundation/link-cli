// The JSON of GraphQL over HTTP and WebSocket: a value keeps the order of its
// object keys, so a response lists its fields in the order they were asked.
// Port of rust/src/graphql/json.rs.
//
// A value is a `Json` of one kind, as the Rust enum: `null`, `bool`, `int`
// (an i64, held as a BigInt so no digit is lost), `float`, `string`, `array`
// and `object` (a list of `[key, value]` pairs, duplicates and order kept).
// Floats print as Rust's `Display` of `f64` does, never in exponent form.

const MIN_I64 = -(2n ** 63n);
const MAX_I64 = 2n ** 63n - 1n;
const MAX_U64 = 2n ** 64n - 1n;

/** Deep enough for any request a client means; a deeper one is refused. */
const MAX_DEPTH = 128;

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/** Rust's `str::parse::<i64>()`: a BigInt, or `null` when it does not parse. */
export function parseI64(text) {
  if (!/^[+-]?[0-9]+$/.test(text)) return null;
  const value = BigInt(text);
  return value < MIN_I64 || value > MAX_I64 ? null : value;
}

/** Rust's `str::parse::<u64>()`: a BigInt, or `null` when it does not parse. */
export function parseU64(text) {
  if (!/^[+]?[0-9]+$/.test(text)) return null;
  const value = BigInt(text);
  return value > MAX_U64 ? null : value;
}

/** Rust's `str::parse::<f64>()` over the characters of a JSON or GraphQL number. */
export function parseF64(text) {
  if (!/^[+-]?([0-9]+\.?[0-9]*|\.[0-9]+)([eE][+-]?[0-9]+)?$/.test(text)) return null;
  return Number(text);
}

/** Rust's `f64 as i64`: truncated and saturated, NaN as 0. */
export function f64ToI64(value) {
  if (Number.isNaN(value)) return 0n;
  if (value >= 2 ** 63) return MAX_I64;
  if (value <= -(2 ** 63)) return MIN_I64;
  return BigInt(Math.trunc(value));
}

/** The shortest digits that read back as `value` (finite, not zero), and the exponent. */
function shortestDigits(value) {
  const [mantissa, exponent] = Math.abs(value).toExponential().split('e');
  return { digits: mantissa.replace('.', ''), exponent: Number(exponent) };
}

function plainDecimal(digits, exponent) {
  if (exponent < 0) return `0.${'0'.repeat(-exponent - 1)}${digits}`;
  if (exponent >= digits.length - 1) return digits + '0'.repeat(exponent - digits.length + 1);
  return `${digits.slice(0, exponent + 1)}.${digits.slice(exponent + 1)}`;
}

/** Rust's `Display` of an `f64`: `1`, `0.1`, `1000000000000000000000`, `-0`. */
export function formatF64(value) {
  if (Number.isNaN(value)) return 'NaN';
  const sign = value < 0 || Object.is(value, -0) ? '-' : '';
  if (!Number.isFinite(value)) return `${sign}inf`;
  if (value === 0) return `${sign}0`;
  const { digits, exponent } = shortestDigits(value);
  return sign + plainDecimal(digits, exponent);
}

/** Rust's `Debug` of an `f64`: `1.0`, `1e16`, `1.5e-5`. */
export function debugF64(value) {
  if (!Number.isFinite(value) || value === 0) {
    const text = formatF64(value);
    return value === 0 ? `${text}.0` : text;
  }
  const absolute = Math.abs(value);
  if (absolute < 1e-4 || absolute >= 1e16) {
    const { digits, exponent } = shortestDigits(value);
    const mantissa = digits.length > 1 ? `${digits[0]}.${digits.slice(1)}` : digits;
    return `${value < 0 ? '-' : ''}${mantissa}e${exponent}`;
  }
  const text = formatF64(value);
  return text.includes('.') ? text : `${text}.0`;
}

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

export class Json {
  constructor(kind, value) {
    this.kind = kind;
    this.value = value;
    Object.freeze(this);
  }

  static bool(value) {
    return value ? TRUE : FALSE;
  }

  /** An `Int` of a number or BigInt. */
  static int(value) {
    return new Json('int', BigInt(value));
  }

  static float(value) {
    return new Json('float', value);
  }

  static str(text) {
    return new Json('string', text);
  }

  static array(items) {
    return new Json('array', items);
  }

  /** An object of `[key, value]` pairs. */
  static object(pairs) {
    return new Json('object', pairs);
  }

  static parse(text) {
    const reader = new Reader(encoder.encode(text));
    const value = reader.value(0);
    reader.space();
    if (reader.at !== reader.bytes.length) {
      throw new Error(`Unexpected text after JSON at byte ${reader.at}.`);
    }
    return value;
  }

  get(key) {
    if (this.kind !== 'object') return null;
    const found = this.value.find(([name]) => name === key);
    return found ? found[1] : null;
  }

  asStr() {
    return this.kind === 'string' ? this.value : null;
  }

  /** The i64 of an `Int`, or of a `Float` without a fraction, as a BigInt. */
  asI64() {
    if (this.kind === 'int') return this.value;
    if (this.kind === 'float' && Number.isFinite(this.value) && Number.isInteger(this.value)) {
      return f64ToI64(this.value);
    }
    return null;
  }

  asF64() {
    if (this.kind === 'int') return Number(this.value);
    if (this.kind === 'float') return this.value;
    return null;
  }

  isNull() {
    return this.kind === 'null';
  }

  /**
   * One object or a list of them, as a list: GraphQL coerces a single value
   * to a list of one, and DeepClient relies on it.
   */
  asList() {
    if (this.kind === 'array') return this.value;
    if (this.kind === 'null') return [];
    return [this];
  }

  /** The derived `PartialEq` of the Rust enum. */
  equals(other) {
    if (this.kind !== other.kind) return false;
    switch (this.kind) {
      case 'array':
        return (
          this.value.length === other.value.length &&
          this.value.every((item, at) => item.equals(other.value[at]))
        );
      case 'object':
        return (
          this.value.length === other.value.length &&
          this.value.every(
            ([key, item], at) => key === other.value[at][0] && item.equals(other.value[at][1]),
          )
        );
      default:
        return this.value === other.value;
    }
  }

  write() {
    switch (this.kind) {
      case 'null':
        return 'null';
      case 'bool':
        return this.value ? 'true' : 'false';
      case 'int':
        return this.value.toString();
      case 'float':
        return Number.isFinite(this.value) ? formatF64(this.value) : 'null';
      case 'string':
        return writeString(this.value);
      case 'array':
        return `[${this.value.map((item) => item.write()).join(',')}]`;
      default:
        return `{${this.value.map(([key, item]) => `${writeString(key)}:${item.write()}`).join(',')}}`;
    }
  }
}

const TRUE = new Json('bool', true);
const FALSE = new Json('bool', false);

/** The null value. */
Json.NULL = new Json('null', null);

const isSpace = (byte) => byte === 0x20 || byte === 0x0a || byte === 0x0d || byte === 0x09;
const isDigit = (byte) => byte >= 0x30 && byte <= 0x39;
const isNumberByte = (byte) =>
  byte === 0x2d ||
  byte === 0x2b ||
  byte === 0x2e ||
  byte === 0x65 ||
  byte === 0x45 ||
  isDigit(byte);

const ESCAPES = { n: '\n', r: '\r', t: '\t', b: '\b', f: '\f', '/': '/', '\\': '\\', '"': '"' };

class Reader {
  constructor(bytes) {
    this.bytes = bytes;
    this.at = 0;
  }

  space() {
    while (this.at < this.bytes.length && isSpace(this.bytes[this.at])) this.at += 1;
  }

  startsWith(text) {
    for (let position = 0; position < text.length; position += 1) {
      if (this.bytes[this.at + position] !== text.charCodeAt(position)) return false;
    }
    return true;
  }

  literal(text, value) {
    if (!this.startsWith(text)) throw new Error(`Invalid JSON at byte ${this.at}.`);
    this.at += text.length;
    return value;
  }

  value(depth) {
    if (depth > MAX_DEPTH) throw new Error('JSON is nested too deeply.');
    this.space();
    const byte = this.bytes[this.at];
    if (byte === 0x6e) return this.literal('null', Json.NULL);
    if (byte === 0x74) return this.literal('true', TRUE);
    if (byte === 0x66) return this.literal('false', FALSE);
    if (byte === 0x22) return Json.str(this.string());
    if (byte === 0x5b) return this.array(depth);
    if (byte === 0x7b) return this.object(depth);
    if (byte === 0x2d || isDigit(byte)) return this.number();
    throw new Error(`Invalid JSON at byte ${this.at}.`);
  }

  array(depth) {
    this.at += 1;
    const items = [];
    this.space();
    if (this.bytes[this.at] === 0x5d) {
      this.at += 1;
      return Json.array(items);
    }
    for (;;) {
      items.push(this.value(depth + 1));
      this.space();
      const byte = this.bytes[this.at];
      if (byte === 0x2c) this.at += 1;
      else if (byte === 0x5d) {
        this.at += 1;
        return Json.array(items);
      } else throw new Error(`Invalid JSON array at byte ${this.at}.`);
    }
  }

  object(depth) {
    this.at += 1;
    const pairs = [];
    this.space();
    if (this.bytes[this.at] === 0x7d) {
      this.at += 1;
      return Json.object(pairs);
    }
    for (;;) {
      this.space();
      if (this.bytes[this.at] !== 0x22) throw new Error(`Invalid JSON object at byte ${this.at}.`);
      const key = this.string();
      this.space();
      if (this.bytes[this.at] !== 0x3a) throw new Error(`Invalid JSON object at byte ${this.at}.`);
      this.at += 1;
      pairs.push([key, this.value(depth + 1)]);
      this.space();
      const byte = this.bytes[this.at];
      if (byte === 0x2c) this.at += 1;
      else if (byte === 0x7d) {
        this.at += 1;
        return Json.object(pairs);
      } else throw new Error(`Invalid JSON object at byte ${this.at}.`);
    }
  }

  number() {
    const start = this.at;
    while (this.at < this.bytes.length && isNumberByte(this.bytes[this.at])) this.at += 1;
    const text = decoder.decode(this.bytes.subarray(start, this.at));
    const integer = parseI64(text);
    if (integer !== null) return new Json('int', integer);
    const value = parseF64(text);
    if (value === null) throw new Error(`Invalid JSON number at byte ${start}.`);
    return Json.float(value);
  }

  hex4() {
    const digits = decoder.decode(this.bytes.subarray(this.at, this.at + 4));
    if (!/^[+]?[0-9a-fA-F]+$/.test(digits) || this.at + 4 > this.bytes.length) {
      throw new Error(`Invalid JSON escape at byte ${this.at}.`);
    }
    this.at += 4;
    return parseInt(digits, 16);
  }

  escape() {
    const escape = this.bytes[this.at];
    if (escape === undefined) throw new Error('Unterminated JSON string.');
    this.at += 1;
    const simple = ESCAPES[String.fromCharCode(escape)];
    if (simple !== undefined) return simple;
    if (escape !== 0x75) throw new Error(`Invalid JSON escape at byte ${this.at}.`);
    let code = this.hex4();
    if (code >= 0xd800 && code < 0xdc00 && this.startsWith('\\u')) {
      this.at += 2;
      const low = this.hex4();
      code = 0x10000 + ((code - 0xd800) << 10) + ((low - 0xdc00) & 0x3ff);
    }
    return code >= 0xd800 && code < 0xe000 ? '�' : String.fromCodePoint(code);
  }

  string() {
    this.at += 1;
    let out = '';
    let start = this.at;
    for (;;) {
      const byte = this.bytes[this.at];
      if (byte === undefined) throw new Error('Unterminated JSON string.');
      if (byte === 0x22 || byte === 0x5c) {
        out += decoder.decode(this.bytes.subarray(start, this.at));
        this.at += 1;
        if (byte === 0x22) return out;
        out += this.escape();
        start = this.at;
      } else this.at += 1;
    }
  }
}

// Rust's `is_printable` leaves these out, and `Debug` of a `str` escapes
// them as `\u{..}`, as it does any grapheme extender.
const NOT_PRINTABLE = /^[\p{Cc}\p{Cf}\p{Cs}\p{Co}\p{Cn}\p{Zl}\p{Zp}\p{Zs}\p{Grapheme_Extend}]$/u;
const DEBUG_ESCAPES = {
  '\0': '\\0',
  '\t': '\\t',
  '\r': '\\r',
  '\n': '\\n',
  '\\': '\\\\',
  '"': '\\"',
};

/** Rust's `Debug` of a `str`: quoted, with what does not print escaped. */
export function debugStr(text) {
  let out = '"';
  for (const character of text) {
    const escape = DEBUG_ESCAPES[character];
    if (escape !== undefined) out += escape;
    else if (character !== ' ' && NOT_PRINTABLE.test(character)) {
      out += `\\u{${character.codePointAt(0).toString(16)}}`;
    } else out += character;
  }
  return `${out}"`;
}

const strict = new TextDecoder('utf-8', { fatal: true });

const isContinuation = (byte) => byte >= 0x80 && byte <= 0xbf;

/** The second byte a lead byte of three or four allows, as Rust's validation reads it. */
function secondAllowed(first, second) {
  if (first === 0xe0) return second >= 0xa0 && second <= 0xbf;
  if (first === 0xed) return second >= 0x80 && second <= 0x9f;
  if (first === 0xf0) return second >= 0x90 && second <= 0xbf;
  if (first === 0xf4) return second >= 0x80 && second <= 0x8f;
  return isContinuation(second);
}

function charWidth(first) {
  if (first >= 0xc2 && first <= 0xdf) return 2;
  if (first >= 0xe0 && first <= 0xef) return 3;
  if (first >= 0xf0 && first <= 0xf4) return 4;
  return 0;
}

/** Rust's `Utf8Error` of bytes that are not UTF-8: where, and how long the bad sequence is. */
function utf8Error(bytes) {
  let at = 0;
  for (;;) {
    const first = bytes[at];
    if (first < 0x80) {
      at += 1;
      continue;
    }
    const width = charWidth(first);
    let length = 1;
    if (width > 0) {
      for (; length < width; length += 1) {
        const next = bytes[at + length];
        if (next === undefined) return `incomplete utf-8 byte sequence from index ${at}`;
        if (length === 1 ? !secondAllowed(first, next) : !isContinuation(next)) break;
      }
      if (length === width) {
        at += width;
        continue;
      }
    }
    return `invalid utf-8 sequence of ${length} bytes from index ${at}`;
  }
}

/** Rust's `String::from_utf8`: the text of UTF-8 bytes, or its error. */
export function utf8Text(bytes) {
  try {
    return strict.decode(bytes);
  } catch {
    throw new Error(utf8Error(bytes));
  }
}

/** Rust's `str::trim`: Unicode white space off both ends. */
export function trimRust(text) {
  return text.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, '');
}
