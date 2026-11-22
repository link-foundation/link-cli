// The `.lenv` and `.env` loading that `Cli::parse` runs first
// (rust/src/cli.rs, `lino_arguments::init()`), ported from the crates the
// Rust `clink` links: `init`, `load_lenv_file` and `load_env_file` of
// lino-arguments 0.5.0, `LinoEnv::read` of lino-env 0.1.0, and
// `from_path_iter`, `Iter`, `QuotedLines`, `eval_end_state`, `parse_line`,
// `LineParser`, `parse_value` and `apply_substitution` of dotenvy 0.15.7.
//
// `.lenv` is loaded first, then `.env`, both from the current directory; a
// variable that is already set is never overwritten, and every error (a
// missing or unreadable file, invalid UTF-8, a line that does not parse) is
// ignored. Rust's `str::trim*` and `char::is_whitespace` are the Unicode
// `White_Space` property, which differs from JavaScript's `trim` (U+FEFF is
// not white space, U+0085 is), hence the regular expressions below.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import process from 'node:process';

const LEADING_WHITE_SPACE = /^\p{White_Space}+/u;
const TRAILING_WHITE_SPACE = /\p{White_Space}+$/u;
const WHITE_SPACE = /^\p{White_Space}$/u;
// `char::is_alphanumeric`: `Alphabetic` or a `Numeric` general category.
const ALPHANUMERIC = /^[\p{Alphabetic}\p{N}]$/u;

/** Rust's `str::trim_start`. */
const trimStart = (text) => text.replace(LEADING_WHITE_SPACE, '');
/** Rust's `str::trim_end`. */
const trimEnd = (text) => text.replace(TRAILING_WHITE_SPACE, '');
/** Rust's `str::trim`. */
const trim = (text) => trimEnd(trimStart(text));
/** Rust's `char::is_whitespace`. */
const isWhitespace = (c) => WHITE_SPACE.test(c);

const encoder = new TextEncoder();
/** `str::len`: the length in UTF-8 bytes, which dotenvy's error positions count. */
const utf8Length = (text) => encoder.encode(text).length;

/** Decodes UTF-8 as `String::from_utf8` does: `undefined` when invalid, a BOM kept. */
function decodeUtf8(bytes) {
  try {
    return new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes);
  } catch {
    return undefined;
  }
}

/** `env::var(key).is_ok()` over `env` (a variable set to a string). */
const isSet = (env, key) => typeof env[key] === 'string';

/**
 * Whether `env::set_var(key, value)` would succeed. Rust panics on an empty
 * key, a key with `=` or NUL, or a value with NUL (an abort, since `init`
 * also runs from a `ctor` before `main`); here such a variable is skipped.
 */
const canSet = (key, value) => key !== '' && !/[=\0]/.test(key) && !value.includes('\0');

/** Reads `path` as bytes, or `undefined` on any error (missing, unreadable, a directory). */
function readBytes(path) {
  try {
    return readFileSync(path);
  } catch {
    return undefined;
  }
}

/**
 * `lino_arguments::init()`: loads `.lenv`, then `.env`, from `cwd` into
 * `env`, never overwriting a variable that is already set and ignoring every
 * error.
 */
export function loadEnvironmentFiles({ cwd = process.cwd(), env = process.env } = {}) {
  loadLenvFile(join(cwd, '.lenv'), env);
  loadEnvFile(join(cwd, '.env'), env);
}

/**
 * `load_lenv_file`: `read_lino_env`, then every key that `env` lacks is set
 * to its last value. Returns the number of variables set.
 */
export function loadLenvFile(path, env) {
  const bytes = readBytes(path);
  if (bytes === undefined) return 0;
  // `BufRead::lines` fails on the first line that is not UTF-8, which fails
  // `LinoEnv::read` and so the whole file.
  const text = decodeUtf8(bytes);
  if (text === undefined) return 0;
  let loaded = 0;
  for (const [key, value] of parseLinoEnv(text)) {
    if (!isSet(env, key) && canSet(key, value)) {
      env[key] = value;
      loaded += 1;
    }
  }
  return loaded;
}

/**
 * `LinoEnv::read` over `text`: lines `KEY: value`, split at the first `": "`,
 * the key trimmed and the value kept as is. Blank lines and lines starting
 * with `#` once trimmed are skipped, and so are lines without `": "`. Returns
 * a `Map` from each key to its last value (`LinoEnv::get`), in the order of
 * the keys' first appearance.
 */
export function parseLinoEnv(text) {
  const values = new Map();
  for (const line of bufReadLines(text)) {
    const trimmed = trim(line);
    if (trimmed === '' || trimmed.startsWith('#')) continue;
    const separator = line.indexOf(': ');
    if (separator === -1) continue;
    values.set(trim(line.slice(0, separator)), line.slice(separator + 2));
  }
  return values;
}

/**
 * `BufRead::lines`: splits at `\n`, dropping it and a `\r` before it; a last
 * line without `\n` keeps its `\r`, and a final `\n` adds no empty line.
 */
function bufReadLines(text) {
  const lines = text.split('\n');
  const last = lines.pop();
  const complete = lines.map((line) => (line.endsWith('\r') ? line.slice(0, -1) : line));
  if (last !== '') complete.push(last);
  return complete;
}

/**
 * `load_env_file`: `dotenvy::from_path_iter`, setting every parsed variable
 * that `env` lacks as soon as it is parsed (so later substitutions see it)
 * and skipping the items that are errors. Returns the number of variables set.
 */
export function loadEnvFile(path, env) {
  const bytes = readBytes(path);
  if (bytes === undefined) return 0;
  let loaded = 0;
  for (const item of parseDotenv(bytes, env)) {
    if (item.error !== undefined) continue;
    if (!isSet(env, item.key) && canSet(item.key, item.value)) {
      env[item.key] = item.value;
      loaded += 1;
    }
  }
  return loaded;
}

/**
 * dotenvy's `Iter` over `input` (a string, or the bytes of a file, whose
 * lines that are not UTF-8 become `Io` errors): yields `{ key, value }` per
 * variable and `{ error: 'LineParse', line, index }` or `{ error: 'Io' }` per
 * line that fails, then goes on with the next line. It is lazy, as in Rust:
 * `$VAR` and `${VAR}` read `env` when their line is parsed, so the variables
 * a caller sets between items are visible, ahead of the file's earlier
 * values. dotenvy's `from_path_iter` removes no byte order mark.
 */
export function* parseDotenv(input, env = process.env) {
  const substitutionData = new Map();
  for (const line of quotedLines(readLines(input))) {
    if (typeof line !== 'string') {
      yield line;
      continue;
    }
    const parsed = parseLine(line, substitutionData, env);
    if (parsed !== null) yield parsed;
  }
}

/**
 * `BufRead::read_line` repeated: the lines of `input` with their `\n`. A
 * byte line that is not UTF-8 is consumed and reported as an `Io` error.
 */
function* readLines(input) {
  if (typeof input === 'string') {
    yield* input.match(/[^\n]*\n|[^\n]+$/g) ?? [];
    return;
  }
  let start = 0;
  while (start < input.length) {
    const newline = input.indexOf(0x0a, start);
    const end = newline === -1 ? input.length : newline + 1;
    const line = decodeUtf8(input.subarray(start, end));
    yield line === undefined ? { error: 'Io' } : line;
    start = end;
  }
}

// The states of dotenvy's `ParseState`.
const COMPLETE = 'Complete';
const ESCAPE = 'Escape';
const STRONG_OPEN = 'StrongOpen';
const STRONG_OPEN_ESCAPE = 'StrongOpenEscape';
const WEAK_OPEN = 'WeakOpen';
const WEAK_OPEN_ESCAPE = 'WeakOpenEscape';
const COMMENT = 'Comment';
const WHITESPACE = 'WhiteSpace';

/**
 * `eval_end_state`: runs the quote state machine over `text` from `state`;
 * returns the index of the last character (of the `#` that starts a
 * comment) and the state at the end.
 */
function evalEndState(state, text) {
  let position = 0;
  let index = 0;
  for (const c of text) {
    position = index;
    index += c.length;
    switch (state) {
      case WHITESPACE:
        if (c === '#') return [position, COMMENT];
        state = { '\\': ESCAPE, '"': WEAK_OPEN, "'": STRONG_OPEN }[c] ?? COMPLETE;
        break;
      case ESCAPE:
        state = COMPLETE;
        break;
      case COMPLETE:
        if (isWhitespace(c) && c !== '\n' && c !== '\r') state = WHITESPACE;
        else state = { '\\': ESCAPE, '"': WEAK_OPEN, "'": STRONG_OPEN }[c] ?? COMPLETE;
        break;
      case WEAK_OPEN:
        state = { '\\': WEAK_OPEN_ESCAPE, '"': COMPLETE }[c] ?? WEAK_OPEN;
        break;
      case WEAK_OPEN_ESCAPE:
        state = WEAK_OPEN;
        break;
      case STRONG_OPEN:
        state = { '\\': STRONG_OPEN_ESCAPE, "'": COMPLETE }[c] ?? STRONG_OPEN;
        break;
      default: // STRONG_OPEN_ESCAPE; `Comment` returned above.
        state = STRONG_OPEN;
    }
  }
  return [position, state];
}

/**
 * `QuotedLines`: joins the lines of an open quote or a trailing escape into
 * one logical line, cuts a ` #comment`, turns a comment line into `''` and
 * drops the `\n` (and a `\r` before it). An `Io` error drops the lines joined
 * so far; a quote still open at the end yields a `LineParse` error.
 */
function* quotedLines(lines) {
  let buffer = '';
  let state = COMPLETE;
  for (const line of lines) {
    if (typeof line !== 'string') {
      yield line;
      buffer = '';
      state = COMPLETE;
      continue;
    }
    const start = buffer.length;
    buffer += line;
    let complete;
    if (trimStart(buffer).startsWith('#')) {
      complete = '';
    } else {
      const [position, end] = evalEndState(state, line);
      state = end;
      if (state === COMMENT) complete = buffer.slice(0, start + position);
      else if (state === COMPLETE) complete = buffer.replace(/\r?\n$/, '');
      else continue;
    }
    yield complete;
    buffer = '';
    state = COMPLETE;
  }
  if (state !== COMPLETE) yield { error: 'LineParse', line: buffer, index: utf8Length(buffer) };
}

/**
 * `parse_line` (`LineParser::parse_line`): `KEY=value`, with an optional
 * `export ` prefix; `null` for a blank or comment line, `{ key, value }`, or
 * a `LineParse` error at the byte position where parsing stopped.
 */
function parseLine(originalLine, substitutionData, env) {
  let line = trimEnd(originalLine);
  let position = 0;
  const error = () => ({ error: 'LineParse', line: originalLine, index: position });
  const skipWhitespace = () => {
    const rest = trimStart(line);
    position += utf8Length(line.slice(0, line.length - rest.length));
    line = rest;
  };
  // `LineParser::parse_key`: an ASCII letter or `_`, then letters, digits, `_` and `.`.
  const parseKey = () => {
    const key = /^[A-Za-z_][A-Za-z0-9_.]*/.exec(line)?.[0];
    if (key === undefined) return undefined;
    position += key.length;
    line = line.slice(key.length);
    return key;
  };
  // `LineParser::expect_equal`.
  const expectEqual = () => {
    if (!line.startsWith('=')) return false;
    line = line.slice(1);
    position += 1;
    return true;
  };

  skipWhitespace();
  if (line === '' || line.startsWith('#')) return null;
  let key = parseKey();
  if (key === undefined) return error();
  skipWhitespace();
  // `export` is either an optional prefix or a key itself.
  if (key === 'export') {
    if (!expectEqual()) {
      key = parseKey();
      if (key === undefined) return error();
      skipWhitespace();
      if (!expectEqual()) return error();
    }
  } else if (!expectEqual()) {
    return error();
  }
  skipWhitespace();
  if (line === '' || line.startsWith('#')) {
    substitutionData.set(key, undefined);
    return { key, value: '' };
  }
  const value = parseValue(line, substitutionData, env);
  if (typeof value !== 'string') return value;
  substitutionData.set(key, value);
  return { key, value };
}

/**
 * `parse_value`: quotes (`'` literal, `"` with escapes and substitution),
 * the escapes `\\ \' \" \$ \ ` and `\n`, `$NAME` / `${NAME}` substitution and
 * a ` #comment` after the value. Returns the value or a `LineParse` error at
 * the character index (at the last byte for an unterminated quote or `${`).
 */
function parseValue(input, substitutionData, env) {
  let strongQuote = false;
  let weakQuote = false;
  let escaped = false;
  let expectingEnd = false;
  let output = '';
  let substitutionMode = 'None';
  let substitutionName = '';
  const error = (index) => ({ error: 'LineParse', line: input, index });
  const substitute = () => {
    output += applySubstitution(substitutionData, substitutionName, env);
    substitutionName = '';
  };

  let index = -1;
  for (const c of input) {
    index += 1;
    if (expectingEnd) {
      if (c === ' ' || c === '\t') continue;
      if (c === '#') break;
      return error(index);
    } else if (escaped) {
      if (c === 'n') output += '\n';
      else if ('\\\'"$ '.includes(c)) output += c;
      else return error(index);
      escaped = false;
    } else if (strongQuote) {
      if (c === "'") strongQuote = false;
      else output += c;
    } else if (substitutionMode !== 'None') {
      if (ALPHANUMERIC.test(c)) {
        substitutionName += c;
      } else if (substitutionMode === 'Block') {
        if (c === '{' && substitutionName === '') {
          substitutionMode = 'EscapedBlock';
        } else {
          substitute();
          // Inside the substitution branch neither quote nor escape is
          // active, so a `$` always starts the next block.
          if (c === '$') {
            substitutionMode = 'Block';
          } else {
            substitutionMode = 'None';
            output += c;
          }
        }
      } else if (c === '}') {
        substitutionMode = 'None';
        substitute();
      } else {
        substitutionName += c;
      }
    } else if (c === '$') {
      substitutionMode = 'Block';
    } else if (weakQuote) {
      if (c === '"') weakQuote = false;
      else if (c === '\\') escaped = true;
      else output += c;
    } else if (c === "'") {
      strongQuote = true;
    } else if (c === '"') {
      weakQuote = true;
    } else if (c === '\\') {
      escaped = true;
    } else if (c === ' ' || c === '\t') {
      expectingEnd = true;
    } else {
      output += c;
    }
  }

  if (substitutionMode === 'EscapedBlock' || strongQuote || weakQuote) {
    // `value_length - 1`, or 0 for an empty value.
    return error(Math.max(utf8Length(input) - 1, 0));
  }
  substitute();
  return output;
}

/**
 * `apply_substitution`: the variable's value in `env`, else its value from
 * an earlier line of the file, else nothing. A name `getenv` cannot look up
 * (empty, or with `=` or NUL) counts as unset in `env`.
 */
function applySubstitution(substitutionData, name, env) {
  if (name !== '' && !/[=\0]/.test(name) && isSet(env, name)) return env[name];
  return substitutionData.get(name) ?? '';
}
