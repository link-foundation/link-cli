// Ports the parse errors of src/lib.rs of the Rust links-notation 0.31.6
// crate: `ParseError` and its `Display`, `SyntaxError` and `NestingTooDeep`
// with their summaries and snippets, and `locate`, which turns the byte offset
// a parse stopped at into a line, a column and the character found there.
import { CR, LF, decodeUtf8, escapeDebug } from './protocol-lino-parser-text.js';

const QUOTED_LINE_WIDTH = 80;
const ELLIPSIS = '...';

/**
 * `ParseError`. `message` is its `Display`; `kind` is `'empty'`, `'syntax'`,
 * `'depth'` or `'internal'`. A syntax or depth error says where: `offset` is
 * a byte offset into the UTF-8 document, `line` and `column` count from one,
 * the column in characters. A syntax error also says what was `expected` there
 * and what was `found`, a character or null at the end of input; a depth error
 * says the `maxDepth` it went past.
 */
export class LinoParseError extends Error {
  constructor(kind, message, fields = {}) {
    super(message);
    this.name = 'LinoParseError';
    this.kind = kind;
    this.line = fields.line ?? null;
    this.column = fields.column ?? null;
    this.offset = fields.offset ?? null;
    this.expected = fields.expected ?? [];
    this.found = fields.found ?? null;
    this.lineText = fields.lineText ?? null;
    this.maxDepth = fields.maxDepth ?? null;
  }

  /** `ParseError::EmptyInput`. */
  static empty() {
    return new LinoParseError('empty', 'Empty input');
  }

  /** `ParseError::InternalError`. */
  static internal(detail) {
    return new LinoParseError('internal', `Internal error: ${detail}`);
  }
}

/** `parse_error`: the error for a failure at a byte offset of `document`. */
export function parseError(document, failure) {
  const located = locate(document, failure);
  const snippet = quoteSnippet(located.line, located.lineText, located.column);
  const at = `line ${located.line}, column ${located.column}`;
  if (failure.maxDepth !== null) {
    const message = `Nesting too deep at ${at}: nesting depth exceeds the maximum of ${failure.maxDepth}`;
    return new LinoParseError('depth', `${message}\n${snippet}`, {
      ...located,
      maxDepth: failure.maxDepth,
    });
  }
  const found = located.found === null ? 'end of input' : `"${escapeDebug(located.found)}"`;
  const expected = joinAlternatives(failure.expected);
  const summary =
    expected === null
      ? `${at}: unexpected ${found}`
      : `${at}: expected ${expected}, found ${found}`;
  return new LinoParseError('syntax', `Syntax error at ${summary}\n${snippet}`, {
    ...located,
    expected: failure.expected,
  });
}

/** `locate`: the line, column, line text and character at `failure.offset`. */
function locate(document, failure) {
  const offset = Math.min(failure.offset, document.length);
  let line = 1;
  let lineStart = 0;
  let cursor = 0;
  while (cursor < offset) {
    const byte = document[cursor];
    cursor += 1;
    if (byte === CR) {
      line += 1;
      if (cursor < offset && document[cursor] === LF) cursor += 1;
      lineStart = cursor;
    } else if (byte === LF) {
      line += 1;
      lineStart = cursor;
    }
  }
  let lineEnd = lineStart;
  while (lineEnd < document.length && document[lineEnd] !== CR && document[lineEnd] !== LF) {
    lineEnd += 1;
  }
  const column = [...decodeUtf8(document, lineStart, offset)].length + 1;
  const rest = decodeUtf8(document, offset, offset + 4);
  const found = offset < document.length ? String.fromCodePoint(rest.codePointAt(0)) : null;
  return { offset, line, column, found, lineText: decodeUtf8(document, lineStart, lineEnd) };
}

/** `join_alternatives`: `a`, `a or b`, `a, b or c`; null for none. */
function joinAlternatives(alternatives) {
  if (alternatives.length === 0) return null;
  const last = alternatives[alternatives.length - 1];
  if (alternatives.length === 1) return last;
  return `${alternatives.slice(0, -1).join(', ')} or ${last}`;
}

/** `snippet`: the numbered line and a caret under the column. */
function quoteSnippet(number, lineText, column) {
  const [quoted, caret] = quoteLine(lineText, column);
  const gutter = ' '.repeat(String(number).length);
  return `${number} | ${quoted}\n${gutter} | ${' '.repeat(caret - 1)}^`;
}

/** `quote_line`: a long line is cut to an 80-character window around the column. */
function quoteLine(line, column) {
  const characters = [...line];
  if (characters.length <= QUOTED_LINE_WIDTH) return [line, column];
  const target = column - 1;
  const lastStart = characters.length - QUOTED_LINE_WIDTH;
  const start = Math.min(Math.max(target - QUOTED_LINE_WIDTH / 2, 0), lastStart);
  const end = start + QUOTED_LINE_WIDTH;
  const before = start > 0 ? ELLIPSIS : '';
  const after = end < characters.length ? ELLIPSIS : '';
  const quoted = `${before}${characters.slice(start, end).join('')}${after}`;
  return [quoted, target - start + before.length + 1];
}
