// Ports src/parser.rs of the Rust links-notation 0.31.5 crate: the `nom`
// grammar of the text notation, its `ParserState` (indentation, nesting depth,
// the furthest failure and the cache of unreadable lines) and
// `ParserState::failure`, which turns a failed parse into a position.
//
// Each parser takes a byte offset into the document with its comments blanked
// and returns `{ pos, value }`, or a `nom` error: `{ err: ERROR, at }`, which
// the alternatives around it recover from, or `{ err: FAILURE, at }`, which
// they pass on. As in `nom`, `alt` reports the error of its last alternative,
// and `many0`/`many1` stop at an error but pass a failure on.
import {
  CLOSE,
  COLON,
  CR,
  LF,
  OPEN,
  SPACE,
  TAB,
  decodeReferenceLiteral,
  decodeUtf8,
  isQuote,
  isReferenceByte,
  isWhitespaceByte,
  prefixEnd,
} from './protocol-lino-parser-text.js';

const ERROR = 1;
const FAILURE = 2;
const RIGHT_BRACE = 0x7d;

function ok(pos, value) {
  return { pos, value };
}

function error(at) {
  return { err: ERROR, at };
}

/** Whether an alternative may be tried after `result`: it is a recoverable error. */
function recoverable(result) {
  return result.err === ERROR;
}

/** A parsed line: an ID, values, indented children, and the body of a group. */
function link(id, values, { indented = false, nested = null } = {}) {
  return { id, values, children: [], indented, nested };
}

/** `Link::new_singlet`. */
function singlet(id) {
  return link(id, []);
}

export class LinoGrammar {
  /**
   * @param {Uint8Array} bytes The document with its comments blanked.
   * @param {import('./protocol-lino-parser-text.js').DelimitedReferences} references
   *   The delimited references of the document as written.
   * @param {number} maxDepth How deep groups and indentation may nest.
   */
  constructor(bytes, references, maxDepth) {
    this.bytes = bytes;
    this.references = references;
    this.maxDepth = maxDepth;
    this.indentationStack = [0];
    this.baseIndentation = null;
    this.nestedDepth = 0;
    this.contextDepth = 0;
    this.tooDeep = null;
    this.furthest = null;
    this.furthestExpected = [];
    this.unreadableLines = new Map();
  }

  /** `document`: the whole document, or the error it stops at. */
  document() {
    const start = this.skipEmptyLines(0);
    if (this.whitespace(start) === this.bytes.length) return ok(this.bytes.length, []);
    const parsed = this.links(start);
    if (parsed.err) return parsed;
    const rest = this.whitespace(parsed.pos);
    if (rest !== this.bytes.length) return this.expected(rest, 'end of input');
    return parsed;
  }

  /**
   * `ParserState::failure`: where the parse stopped, as `{ offset, expected,
   * maxDepthExceeded }`. The furthest tracked position wins over `nom`'s own,
   * which is the floor; a document nested too deep is reported where it did.
   */
  failure(stopped) {
    const length = this.bytes.length;
    if (this.tooDeep !== null) {
      return { offset: Math.min(this.tooDeep, length), expected: [], maxDepth: this.maxDepth };
    }
    const tracked = this.furthest ?? 0;
    const offset = Math.min(Math.max(tracked, stopped.at), length);
    const expected = tracked === offset ? this.furthestExpected.slice() : [];
    return { offset, expected, maxDepth: null };
  }

  depth() {
    return this.contextDepth + this.indentationStack.length - 1;
  }

  currentIndentation() {
    return this.indentationStack[this.indentationStack.length - 1];
  }

  normalizeIndentation(spaces) {
    return Math.max(spaces - (this.baseIndentation ?? 0), 0);
  }

  popIndentation() {
    if (this.indentationStack.length > 1) this.indentationStack.pop();
  }

  /** A group body starts fresh at indentation level zero, one level deeper. */
  enterNestedContext() {
    const depth = this.depth() + 1;
    const saved = {
      indentationStack: this.indentationStack,
      baseIndentation: this.baseIndentation,
      contextDepth: this.contextDepth,
    };
    this.indentationStack = [0];
    this.baseIndentation = null;
    this.contextDepth = depth;
    this.nestedDepth += 1;
    return saved;
  }

  exitNestedContext(saved) {
    this.indentationStack = saved.indentationStack;
    this.baseIndentation = saved.baseIndentation;
    this.contextDepth = saved.contextDepth;
    this.nestedDepth -= 1;
  }

  /** `check_depth`: a failure at `at` when `depth` is past the limit, else null. */
  checkDepth(at, depth) {
    if (depth <= this.maxDepth) return null;
    if (this.tooDeep === null) this.tooDeep = at;
    return { err: FAILURE, at };
  }

  /** `expected_at`: keeps the furthest position and everything expected there. */
  expectedAt(at, what) {
    if (this.furthest !== null && this.furthest > at) return;
    if (this.furthest === at) {
      if (!this.furthestExpected.includes(what)) this.furthestExpected.push(what);
      return;
    }
    this.furthest = at;
    this.furthestExpected = [what];
  }

  expected(at, what) {
    this.expectedAt(at, what);
    return error(at);
  }

  /** `horizontal_whitespace`. */
  hws(pos) {
    while (this.bytes[pos] === SPACE || this.bytes[pos] === TAB) pos += 1;
    return pos;
  }

  /** `whitespace`. */
  whitespace(pos) {
    while (pos < this.bytes.length && isWhitespaceByte(this.bytes[pos])) pos += 1;
    return pos;
  }

  /** `strip_line_ending`: past a `\r\n`, `\n` or `\r` at `pos`, or -1. */
  lineEnding(pos) {
    if (this.bytes[pos] === CR) return this.bytes[pos + 1] === LF ? pos + 2 : pos + 1;
    return this.bytes[pos] === LF ? pos + 1 : -1;
  }

  /** `skip_empty_lines`: past every blank line, up to the line that is not. */
  skipEmptyLines(pos) {
    for (;;) {
      const next = this.lineEnding(this.hws(pos));
      if (next < 0) return pos;
      pos = next;
    }
  }

  /** `reference`: a reference literal, a delimited reference or a plain one. */
  reference(pos) {
    const bytes = this.bytes;
    const prefix = prefixEnd(bytes, pos);
    if (prefix >= 0) {
      const closing = bytes.indexOf(RIGHT_BRACE, prefix);
      if (closing >= 0) {
        const value = decodeReferenceLiteral(bytes, pos, prefix, closing + 1);
        if (value !== null) return ok(closing + 1, value);
      }
      this.expectedAt(pos, 'a valid version 1 reference literal');
      return { err: FAILURE, at: pos };
    }
    // A delimiter is never blanked, so the document as written opens the same
    // delimited reference here.
    const reading = isQuote(bytes[pos]) ? this.references.read(pos) : null;
    if (reading !== null) return ok(pos + reading.length, reading.value);
    let end = pos;
    while (end < bytes.length && isReferenceByte(bytes[end])) end += 1;
    if (end > pos) return ok(end, decodeUtf8(bytes, pos, end));
    return this.expected(pos, 'a reference');
  }

  /** `eol`: a line break, the end of input, or the `)` that closes a group. */
  eol(pos) {
    const blank = this.hws(pos);
    const next = this.lineEnding(blank);
    if (next >= 0) return ok(next);
    if (blank === this.bytes.length) return ok(blank);
    const end = this.nestedGroupEnd(pos);
    if (end.err) this.expectedAt(pos, 'end of line');
    return end;
  }

  /** `nested_group_end`: inside a group, `)` ends a line without being consumed. */
  nestedGroupEnd(pos) {
    if (this.nestedDepth === 0) return error(pos);
    const rest = this.hws(pos);
    return this.bytes[rest] === CLOSE ? ok(rest) : this.expected(rest, '")"');
  }

  character(code, pos, what) {
    return this.bytes[pos] === code ? ok(pos + 1) : this.expected(pos, what);
  }

  /** `reference_or_link`: a group, or a reference. */
  referenceOrLink(pos) {
    const group = this.nestedGroup(pos);
    if (!recoverable(group)) return group;
    const reference = this.reference(pos);
    return reference.err ? reference : ok(reference.pos, singlet(reference.value));
  }

  /** `single_line_value_and_whitespace`. */
  value(pos) {
    return this.referenceOrLink(this.hws(pos));
  }

  /** `many0(single_line_value_and_whitespace)`, after `values`. */
  moreValues(pos, values) {
    for (;;) {
      const next = this.value(pos);
      if (next.err) return recoverable(next) ? ok(pos, values) : next;
      values.push(next.value);
      pos = next.pos;
    }
  }

  /** `single_line_values`: `many1` of values. */
  values(pos) {
    const first = this.value(pos);
    return first.err ? first : this.moreValues(first.pos, [first.value]);
  }

  /** `single_line_link`: `id: values`. */
  singleLineLink(pos) {
    const id = this.reference(this.hws(pos));
    if (id.err) return id;
    const colon = this.character(COLON, this.hws(id.pos), '":"');
    if (colon.err) return colon;
    const values = this.values(colon.pos);
    return values.err ? values : ok(values.pos, link(id.value, values.value));
  }

  /** `single_line_value_link`: values without an ID. */
  singleLineValueLink(pos) {
    const values = this.values(pos);
    return values.err ? values : ok(values.pos, link(null, values.value));
  }

  /** `indented_id_link`: `id:` alone on its line, its values indented below. */
  indentedIdLink(pos) {
    const id = this.reference(pos);
    if (id.err) return id;
    const colon = this.character(COLON, this.hws(id.pos), '":"');
    if (colon.err) return colon;
    const end = this.eol(colon.pos);
    return end.err ? end : ok(end.pos, link(id.value, [], { indented: true }));
  }

  /** `nested_group`: a group opens a nested context, one level deeper. */
  nestedGroup(pos) {
    const open = this.character(OPEN, pos, '"("');
    if (open.err) return open;
    const tooDeep = this.checkDepth(pos, this.depth() + 1);
    if (tooDeep !== null) return tooDeep;
    const saved = this.enterNestedContext();
    const result = this.nestedGroupBody(open.pos);
    this.exitNestedContext(saved);
    return result;
  }

  nestedGroupBody(pos) {
    const body = this.links(this.skipEmptyLines(pos));
    if (!body.err) {
      const close = this.character(CLOSE, this.whitespace(body.pos), '")"');
      return close.err ? close : ok(close.pos, link(null, [], { nested: body.value }));
    }
    if (!recoverable(body)) return body;
    const close = this.character(CLOSE, this.whitespace(pos), '")"');
    return close.err ? close : ok(close.pos, link(null, [], { nested: [] }));
  }

  /** `terminated(parsed, eol)`. */
  endOfLine(parsed) {
    if (parsed.err) return parsed;
    const end = this.eol(parsed.pos);
    return end.err ? end : ok(end.pos, parsed.value);
  }

  /** `single_line_any_link`. */
  singleLineAnyLink(pos) {
    const named = this.endOfLine(this.singleLineLink(pos));
    if (!recoverable(named)) return named;
    return this.endOfLine(this.singleLineValueLink(pos));
  }

  /** `any_link`: one line, or at once the error a line already failed with. */
  anyLink(pos) {
    const key = this.nestedDepth > 0 ? -1 - pos : pos;
    const unreadable = this.unreadableLines.get(key);
    if (unreadable !== undefined) return error(unreadable);
    const parsed = this.readAnyLink(pos);
    if (recoverable(parsed)) this.unreadableLines.set(key, parsed.at);
    return parsed;
  }

  /** `read_any_link`: a line that opens a group reads it once, then branches. */
  readAnyLink(pos) {
    const group = this.nestedGroup(pos);
    if (group.err) {
      // No reference starts with `(`, so this reports the missing reference.
      if (this.bytes[pos] === OPEN) return this.reference(pos);
      const indented = this.indentedIdLink(pos);
      if (!recoverable(indented)) return indented;
      return this.singleLineAnyLink(pos);
    }
    const end = this.eol(group.pos);
    if (!end.err) return ok(end.pos, group.value);
    const values = this.moreValues(group.pos, [group.value]);
    return this.endOfLine(values.err ? values : ok(values.pos, link(null, values.value)));
  }

  countIndentation(pos) {
    let spaces = 0;
    while (this.bytes[pos + spaces] === SPACE) spaces += 1;
    return spaces;
  }

  /** `push_indentation`: a line indented past the current level opens a new one. */
  pushIndentation(pos) {
    const start = this.skipEmptyLines(pos);
    const spaces = this.countIndentation(start);
    const indentation = this.normalizeIndentation(spaces);
    if (indentation <= this.currentIndentation()) return error(start + spaces);
    this.indentationStack.push(indentation);
    return ok(start + spaces);
  }

  /** `check_indentation`: a line indented at least as far as the current level. */
  checkIndentation(pos) {
    const spaces = this.countIndentation(pos);
    const indentation = this.normalizeIndentation(spaces);
    if (indentation >= this.currentIndentation()) return ok(pos + spaces);
    return error(pos + spaces);
  }

  /** `element`: a line and the lines indented below it. */
  element(pos) {
    const line = this.anyLink(pos);
    if (line.err) return line;
    const tooDeep = this.checkDepth(pos, this.depth());
    if (tooDeep !== null) return tooDeep;
    const indentation = this.indentationStack.slice();
    const pushed = this.pushIndentation(line.pos);
    if (!pushed.err) {
      const children = this.links(pushed.pos);
      if (!children.err) {
        line.value.children = children.value;
        return ok(children.pos, line.value);
      }
      if (!recoverable(children)) return children;
      // No child line followed the indentation: backtrack to the line.
      this.indentationStack = indentation;
    }
    return line;
  }

  /** `first_line`: its indentation is the base the context's lines count from. */
  firstLine(pos) {
    const spaces = this.countIndentation(pos);
    if (this.baseIndentation === null) this.baseIndentation = spaces;
    return this.element(pos + spaces);
  }

  /** `line`: blank lines are skipped. */
  line(pos) {
    const start = this.checkIndentation(this.skipEmptyLines(pos));
    return start.err ? start : this.element(start.pos);
  }

  /** `links`: a first line, then every line that follows at its level or deeper. */
  links(pos) {
    const first = this.firstLine(pos);
    if (first.err) return first;
    const lines = [first.value];
    let rest = first.pos;
    for (;;) {
      const next = this.line(rest);
      if (next.err) {
        if (!recoverable(next)) return next;
        break;
      }
      lines.push(next.value);
      rest = next.pos;
    }
    this.popIndentation();
    return ok(rest, lines);
  }
}
