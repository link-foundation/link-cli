// The LiNo text parser, a port of the Rust links-notation 0.31.5 crate. Every
// expectation below was produced by that crate: parse_lino_to_links_with_config
// for the links, and the Display of its ParseError for the messages.
import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { LinoParseError, parseError } from '../src/protocol-lino-parser-errors.js';
import { LinoGrammar } from '../src/protocol-lino-parser-grammar.js';
import {
  DEFAULT_MAX_DEPTH,
  LinoParseError as ExportedLinoParseError,
  parseLinoToLinks,
} from '../src/protocol-lino-parser.js';
import {
  DelimitedReferences,
  decodeReferenceLiteral,
  encodeUtf8,
  escapeDebug,
  isQuote,
  prefixEnd,
  referenceLiteralEnd,
  stripComments,
} from '../src/protocol-lino-parser-text.js';

// prettier-ignore
const SYNTAX = [
  ["( :", 64, {"message":"Syntax error at line 1, column 3: expected \"(\", a reference or \")\", found \":\"\n1 | ( :\n  |   ^","kind":"syntax","line":1,"column":3,"offset":2,"expected":["\"(\"","a reference","\")\""],"found":":"}],
  ["(\n", 64, {"message":"Syntax error at line 2, column 1: expected \"(\", a reference or \")\", found end of input\n2 | \n  | ^","kind":"syntax","line":2,"column":1,"offset":2,"expected":["\"(\"","a reference","\")\""],"found":null}],
  ["(a) b: c", 64, {"message":"Syntax error at line 1, column 6: expected \"(\", a reference or end of line, found \":\"\n1 | (a) b: c\n  |      ^","kind":"syntax","line":1,"column":6,"offset":5,"expected":["\"(\"","a reference","end of line"],"found":":"}],
  ["(a) b)", 64, {"message":"Syntax error at line 1, column 6: expected \"(\", a reference or end of line, found \")\"\n1 | (a) b)\n  |      ^","kind":"syntax","line":1,"column":6,"offset":5,"expected":["\"(\"","a reference","end of line"],"found":")"}],
  ["a\n  b: c: d\n", 64, {"message":"Syntax error at line 2, column 7: expected \"(\", a reference or end of line, found \":\"\n2 |   b: c: d\n  |       ^","kind":"syntax","line":2,"column":7,"offset":8,"expected":["\"(\"","a reference","end of line"],"found":":"}],
  ["(a\r", 64, {"message":"Syntax error at line 2, column 1: expected \"(\", a reference or \")\", found end of input\n2 | \n  | ^","kind":"syntax","line":2,"column":1,"offset":3,"expected":["\"(\"","a reference","\")\""],"found":null}],
  ["(a\r\n", 64, {"message":"Syntax error at line 2, column 1: expected \"(\", a reference or \")\", found end of input\n2 | \n  | ^","kind":"syntax","line":2,"column":1,"offset":4,"expected":["\"(\"","a reference","\")\""],"found":null}],
  ["a: b: c", 64, {"message":"Syntax error at line 1, column 5: expected \"(\", a reference or end of line, found \":\"\n1 | a: b: c\n  |     ^","kind":"syntax","line":1,"column":5,"offset":4,"expected":["\"(\"","a reference","end of line"],"found":":"}],
  [")", 64, {"message":"Syntax error at line 1, column 1: expected \"(\" or a reference, found \")\"\n1 | )\n  | ^","kind":"syntax","line":1,"column":1,"offset":0,"expected":["\"(\"","a reference"],"found":")"}],
  ["(a:)", 64, ["a"]],
  ["((a", 64, {"message":"Syntax error at line 1, column 4: expected \":\", \"(\", a reference or \")\", found end of input\n1 | ((a\n  |    ^","kind":"syntax","line":1,"column":4,"offset":3,"expected":["\":\"","\"(\"","a reference","\")\""],"found":null}],
  ["ci_gate x\nstage: rust: nextest\n", 64, {"message":"Syntax error at line 2, column 12: expected \"(\", a reference or end of line, found \":\"\n2 | stage: rust: nextest\n  |            ^","kind":"syntax","line":2,"column":12,"offset":21,"expected":["\"(\"","a reference","end of line"],"found":":"}],
  ["a\nb\nc\nd\ne: f: g\nh\n", 64, {"message":"Syntax error at line 5, column 5: expected \"(\", a reference or end of line, found \":\"\n5 | e: f: g\n  |     ^","kind":"syntax","line":5,"column":5,"offset":12,"expected":["\"(\"","a reference","end of line"],"found":":"}],
  ["a\rb: c: d", 64, {"message":"Syntax error at line 2, column 5: expected \"(\", a reference or end of line, found \":\"\n2 | b: c: d\n  |     ^","kind":"syntax","line":2,"column":5,"offset":6,"expected":["\"(\"","a reference","end of line"],"found":":"}],
  ["x\r\n  y: (z", 64, {"message":"Syntax error at line 2, column 8: expected \":\", \"(\", a reference or \")\", found end of input\n2 |   y: (z\n  |        ^","kind":"syntax","line":2,"column":8,"offset":10,"expected":["\":\"","\"(\"","a reference","\")\""],"found":null}],
  ["a (b\n", 64, {"message":"Syntax error at line 2, column 1: expected \"(\", a reference or \")\", found end of input\n2 | \n  | ^","kind":"syntax","line":2,"column":1,"offset":5,"expected":["\"(\"","a reference","\")\""],"found":null}],
  ["a b)\n", 64, {"message":"Syntax error at line 1, column 4: expected \"(\", a reference or end of line, found \")\"\n1 | a b)\n  |    ^","kind":"syntax","line":1,"column":4,"offset":3,"expected":["\"(\"","a reference","end of line"],"found":")"}],
  ["привет: b: c", 64, {"message":"Syntax error at line 1, column 10: expected \"(\", a reference or end of line, found \":\"\n1 | привет: b: c\n  |          ^","kind":"syntax","line":1,"column":10,"offset":15,"expected":["\"(\"","a reference","end of line"],"found":":"}],
  ["😀 x: y: z", 64, {"message":"Syntax error at line 1, column 4: expected \"(\", a reference or end of line, found \":\"\n1 | 😀 x: y: z\n  |    ^","kind":"syntax","line":1,"column":4,"offset":6,"expected":["\"(\"","a reference","end of line"],"found":":"}],
  ["\"\")(\"\" z", 64, {"message":"Syntax error at line 1, column 3: expected \":\", \"(\", a reference or end of line, found \")\"\n1 | \"\")(\"\" z\n  |   ^","kind":"syntax","line":1,"column":3,"offset":2,"expected":["\":\"","\"(\"","a reference","end of line"],"found":")"}],
  ["\u0001 x: y: z", 64, {"message":"Syntax error at line 1, column 4: expected \"(\", a reference or end of line, found \":\"\n1 | \u0001 x: y: z\n  |    ^","kind":"syntax","line":1,"column":4,"offset":3,"expected":["\"(\"","a reference","end of line"],"found":":"}],
  ["a: b: \"\\\"", 64, {"message":"Syntax error at line 1, column 5: expected \"(\", a reference or end of line, found \":\"\n1 | a: b: \"\\\"\n  |     ^","kind":"syntax","line":1,"column":5,"offset":4,"expected":["\"(\"","a reference","end of line"],"found":":"}],
  ["é x: y: z", 64, {"message":"Syntax error at line 1, column 4: expected \"(\", a reference or end of line, found \":\"\n1 | é x: y: z\n  |    ^","kind":"syntax","line":1,"column":4,"offset":4,"expected":["\"(\"","a reference","end of line"],"found":":"}],
  ["x: y:  ", 64, {"message":"Syntax error at line 1, column 5: expected \"(\", a reference or end of line, found \":\"\n1 | x: y:  \n  |     ^","kind":"syntax","line":1,"column":5,"offset":4,"expected":["\"(\"","a reference","end of line"],"found":":"}],
  ["a:b:c", 64, {"message":"Syntax error at line 1, column 4: expected \"(\", a reference or end of line, found \":\"\n1 | a:b:c\n  |    ^","kind":"syntax","line":1,"column":4,"offset":3,"expected":["\"(\"","a reference","end of line"],"found":":"}],
  ["a ̀: b: c", 64, {"message":"Syntax error at line 1, column 4: expected \"(\", a reference or end of line, found \":\"\n1 | a ̀: b: c\n  |    ^","kind":"syntax","line":1,"column":4,"offset":4,"expected":["\"(\"","a reference","end of line"],"found":":"}],
];

// prettier-ignore
const DEPTH = [
  ["(a) (b (c))", 1, {"message":"Nesting too deep at line 1, column 8: nesting depth exceeds the maximum of 1\n1 | (a) (b (c))\n  |        ^","kind":"depth","line":1,"column":8,"offset":7,"maxDepth":1}],
  ["a\nb (c)\n", 0, {"message":"Nesting too deep at line 2, column 3: nesting depth exceeds the maximum of 0\n2 | b (c)\n  |   ^","kind":"depth","line":2,"column":3,"offset":4,"maxDepth":0}],
  ["(a) (((b)))", 2, {"message":"Nesting too deep at line 1, column 7: nesting depth exceeds the maximum of 2\n1 | (a) (((b)))\n  |       ^","kind":"depth","line":1,"column":7,"offset":6,"maxDepth":2}],
  ["a\n  ((b))\n", 2, {"message":"Nesting too deep at line 2, column 4: nesting depth exceeds the maximum of 2\n2 |   ((b))\n  |    ^","kind":"depth","line":2,"column":4,"offset":5,"maxDepth":2}],
  ["((((a))))", 3, {"message":"Nesting too deep at line 1, column 4: nesting depth exceeds the maximum of 3\n1 | ((((a))))\n  |    ^","kind":"depth","line":1,"column":4,"offset":3,"maxDepth":3}],
  ["a b c d", 3, [{"id":null,"values":["a","b","c","d"]}]],
  ["(a (b (c (d))))", 3, {"message":"Nesting too deep at line 1, column 10: nesting depth exceeds the maximum of 3\n1 | (a (b (c (d))))\n  |          ^","kind":"depth","line":1,"column":10,"offset":9,"maxDepth":3}],
  ["a\n  b\n    c\n      d\n        e\n", 3, {"message":"Nesting too deep at line 5, column 9: nesting depth exceeds the maximum of 3\n5 |         e\n  |         ^","kind":"depth","line":5,"column":9,"offset":28,"maxDepth":3}],
  ["a\n  (b)\n", 1, {"message":"Nesting too deep at line 2, column 3: nesting depth exceeds the maximum of 1\n2 |   (b)\n  |   ^","kind":"depth","line":2,"column":3,"offset":4,"maxDepth":1}],
  ["a\n  (b)\n", 2, [{"id":null,"values":["a"]},{"id":null,"values":[{"id":null,"values":["a"]},{"id":null,"values":["b"]}]}]],
  ["a\n  b\n    c   \n", 2, [{"id":null,"values":["a"]},{"id":null,"values":[{"id":null,"values":["a"]},{"id":null,"values":["b"]}]},{"id":null,"values":[{"id":null,"values":[{"id":null,"values":["a"]},{"id":null,"values":["b"]}]},{"id":null,"values":["c"]}]}]],
  ["a", 0, [{"id":null,"values":["a"]}]],
  ["(a)", 0, {"message":"Nesting too deep at line 1, column 1: nesting depth exceeds the maximum of 0\n1 | (a)\n  | ^","kind":"depth","line":1,"column":1,"offset":0,"maxDepth":0}],
  ["a b", 0, [{"id":null,"values":["a","b"]}]],
];

// prettier-ignore
const COMMENTS = [
  ["a: b # why", 64, [{"id":"a","values":["b"]}]],
  ["# a: b", 64, []],
  ["issue#1047", 64, [{"id":null,"values":["issue#1047"]}]],
  ["\"# not a comment\" a", 64, [{"id":null,"values":["# not a comment","a"]}]],
  ["(a)\" #b\"", 64, [{"id":null,"values":[{"id":null,"values":["a"]}," #b"]}]],
  ["```q```\"\" #c\n\"\"\"\n", 64, [{"id":null,"values":["q"," #c\n\""]}]],
  ["\"\" a #)b\n\"\"\n", 64, [{"id":null,"values":["","a"]},{"id":null,"values":[""]}]],
  ["~1{62}\" #b\"", 64, [{"id":null,"values":["b"," #b"]}]],
];

// prettier-ignore
const QUOTES = [
  ["\"a b\" 'c d' `e f`", 64, [{"id":null,"values":["a b","c d","e f"]}]],
  ["\"\"\"x\"\"\"\" y", 64, [{"id":null,"values":["x\"","y"]}]],
  ["''it''s'' ok", 64, [{"id":null,"values":["it","s''","ok"]}]],
  ["\"\"(a b)\"\" c", 64, [{"id":null,"values":["(a b)","c"]}]],
  ["\"\" x", 64, [{"id":null,"values":["","x"]}]],
  ["\"unclosed", 64, [{"id":null,"values":["\"unclosed"]}]],
  ["\"\"  \"\" z", 64, [{"id":null,"values":["","","z"]}]],
  ["\"\"\"\"x\"\"\"\" y", 64, [{"id":null,"values":["x","y"]}]],
  ["`a``b` c", 64, [{"id":null,"values":["a`b","c"]}]],
];

// prettier-ignore
const LITERALS = [
  ["~1{68656c6c6f} x", 64, [{"id":null,"values":["hello","x"]}]],
  ["~1{68656C6C6F}", 64, [{"id":null,"values":["hello"]}]],
  ["~2{61}", 64, {"message":"Syntax error at line 1, column 1: expected \"(\" or a valid version 1 reference literal, found \"~\"\n1 | ~2{61}\n  | ^","kind":"syntax","line":1,"column":1,"offset":0,"expected":["\"(\"","a valid version 1 reference literal"],"found":"~"}],
  ["~12{61}", 64, {"message":"Syntax error at line 1, column 1: expected \"(\" or a valid version 1 reference literal, found \"~\"\n1 | ~12{61}\n  | ^","kind":"syntax","line":1,"column":1,"offset":0,"expected":["\"(\"","a valid version 1 reference literal"],"found":"~"}],
  ["~1{616}", 64, {"message":"Syntax error at line 1, column 1: expected \"(\" or a valid version 1 reference literal, found \"~\"\n1 | ~1{616}\n  | ^","kind":"syntax","line":1,"column":1,"offset":0,"expected":["\"(\"","a valid version 1 reference literal"],"found":"~"}],
  ["~1{zz}", 64, {"message":"Syntax error at line 1, column 1: expected \"(\" or a valid version 1 reference literal, found \"~\"\n1 | ~1{zz}\n  | ^","kind":"syntax","line":1,"column":1,"offset":0,"expected":["\"(\"","a valid version 1 reference literal"],"found":"~"}],
  ["~1{ff}", 64, {"message":"Syntax error at line 1, column 1: expected \"(\" or a valid version 1 reference literal, found \"~\"\n1 | ~1{ff}\n  | ^","kind":"syntax","line":1,"column":1,"offset":0,"expected":["\"(\"","a valid version 1 reference literal"],"found":"~"}],
  ["~{61}", 64, [{"id":null,"values":["~{61}"]}]],
  ["~1x", 64, [{"id":null,"values":["~1x"]}]],
  ["~1{61 62}", 64, {"message":"Syntax error at line 1, column 1: expected \"(\" or a valid version 1 reference literal, found \"~\"\n1 | ~1{61 62}\n  | ^","kind":"syntax","line":1,"column":1,"offset":0,"expected":["\"(\"","a valid version 1 reference literal"],"found":"~"}],
  ["(~1{f09f9880}: a)", 64, [{"id":"😀","values":["a"]}]],
];

// prettier-ignore
const DOCUMENTS = [
  ["()", 64, [{"id":null,"values":[]}]],
  ["( )", 64, [{"id":null,"values":[]}]],
  ["(a) b", 64, [{"id":null,"values":[{"id":null,"values":["a"]},"b"]}]],
  ["(a:)\n  b\n", 64, ["a",{"id":null,"values":[{"id":null,"values":["a"]},{"id":null,"values":["b"]}]}]],
  ["a\n  b:\n    c\n", 64, [{"id":null,"values":["a"]},{"id":null,"values":[{"id":null,"values":["a"]},{"id":"b","values":["c"]}]}]],
  ["(a)\n  b\n", 64, [{"id":null,"values":["a"]},{"id":null,"values":[{"id":null,"values":["a"]},{"id":null,"values":["b"]}]}]],
  ["x:\n  y:\n    z\n", 64, [{"id":"x","values":[{"id":"y","values":["z"]}]}]],
  ["x:\n  (y:)\n    z\n", 64, [{"id":"x","values":[{"id":"y","values":["z"]}]}]],
  ["a\r\nb c\r\n  d\r\n", 64, [{"id":null,"values":["a"]},{"id":null,"values":["b","c"]},{"id":null,"values":[{"id":null,"values":["b","c"]},{"id":null,"values":["d"]}]}]],
  ["papa (lovesMama: loves mama)\nson lovesMama\n", 64, [{"id":null,"values":["papa",{"id":"lovesMama","values":["loves","mama"]}]},{"id":null,"values":["son","lovesMama"]}]],
  ["id:\n  a\n  b c\n    d\n", 64, [{"id":"id","values":["a",{"id":null,"values":["b","c","d"]}]}]],
  ["(a b)\n  c\n", 64, [{"id":null,"values":["a","b"]},{"id":null,"values":[{"id":null,"values":["a","b"]},{"id":null,"values":["c"]}]}]],
  ["x\n  (y)\n    z\n", 64, [{"id":null,"values":["x"]},{"id":null,"values":[{"id":null,"values":["x"]},{"id":null,"values":["y"]}]},{"id":null,"values":[{"id":null,"values":[{"id":null,"values":["x"]},{"id":null,"values":["y"]}]},{"id":null,"values":["z"]}]}]],
  ["x:\n  (y)\n    z\n", 64, [{"id":"x","values":[{"id":null,"values":["y","z"]}]}]],
  ["x:\n  y: a b\n    z\n", 64, [{"id":"x","values":[{"id":"y","values":["a","b","z"]}]}]],
  ["x:\n  a b\n    c\n", 64, [{"id":"x","values":[{"id":null,"values":["a","b","c"]}]}]],
  ["x:\n  y\n    z\n", 64, [{"id":"x","values":[{"id":null,"values":["y","z"]}]}]],
  ["x:\n  (y: a)\n", 64, [{"id":"x","values":[{"id":"y","values":["a"]}]}]],
  ["x:\n  a\n", 64, [{"id":"x","values":["a"]}]],
  ["((a b))", 64, [{"id":null,"values":[{"id":null,"values":["a","b"]}]}]],
  ["(a b)", 64, [{"id":null,"values":["a","b"]}]],
  ["((a) (b))", 64, [{"id":null,"values":[{"id":null,"values":["a"]},{"id":null,"values":["b"]}]}]],
  ["(id: a b)\n", 64, [{"id":"id","values":["a","b"]}]],
  ["  a\n  b\n", 64, [{"id":null,"values":["a"]},{"id":null,"values":["b"]}]],
  ["a\n\n\n  b\n", 64, [{"id":null,"values":["a"]},{"id":null,"values":[{"id":null,"values":["a"]},{"id":null,"values":["b"]}]}]],
  ["a:\n", 64, ["a"]],
  ["\t a b", 64, [{"id":null,"values":["a","b"]}]],
  ["a\n b\n  c\n", 64, [{"id":null,"values":["a"]},{"id":null,"values":[{"id":null,"values":["a"]},{"id":null,"values":["b"]}]},{"id":null,"values":[{"id":null,"values":[{"id":null,"values":["a"]},{"id":null,"values":["b"]}]},{"id":null,"values":["c"]}]}]],
  ["a\n  b\n c\n", 64, [{"id":null,"values":["a"]},{"id":null,"values":[{"id":null,"values":["a"]},{"id":null,"values":["b"]}]},{"id":null,"values":["c"]}]],
  ["(a\n  b)", 64, [{"id":null,"values":[{"id":null,"values":["a"]},{"id":null,"values":[{"id":null,"values":["a"]},{"id":null,"values":["b"]}]}]}]],
  ["(\n  a\n  b\n)", 64, [{"id":null,"values":[{"id":null,"values":["a"]},{"id":null,"values":["b"]}]}]],
  ["", 64, []],
  ["  \n \r\n", 64, []],
  ["﻿a", 64, [{"id":null,"values":["﻿a"]}]],
];

// prettier-ignore
const LONG = [
  ["a".repeat(122) + ": b: c", 64, {"message":"Syntax error at line 1, column 126: expected \"(\", a reference or end of line, found \":\"\n1 | ...aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa: b: c\n  |                                                                                 ^","kind":"syntax","line":1,"column":126,"offset":125,"expected":["\"(\"","a reference","end of line"],"found":":"}],
  ["x: y: " + "z".repeat(109), 64, {"message":"Syntax error at line 1, column 5: expected \"(\", a reference or end of line, found \":\"\n1 | x: y: zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz...\n  |     ^","kind":"syntax","line":1,"column":5,"offset":4,"expected":["\"(\"","a reference","end of line"],"found":":"}],
  ["a".repeat(112) + " x: y: z", 64, {"message":"Syntax error at line 1, column 115: expected \"(\", a reference or end of line, found \":\"\n1 | ...aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa x: y: z\n  |                                                                              ^","kind":"syntax","line":1,"column":115,"offset":114,"expected":["\"(\"","a reference","end of line"],"found":":"}],
  ["a".repeat(100) + ": b: c dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd", 64, {"message":"Syntax error at line 1, column 104: expected \"(\", a reference or end of line, found \":\"\n1 | ...aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa: b: c dddddddddddddddddddddddddddddddddddd...\n  |                                            ^","kind":"syntax","line":1,"column":104,"offset":103,"expected":["\"(\"","a reference","end of line"],"found":":"}],
];

/** Parses `text` and checks the links, or the error, the Rust crate gives. */
function check([text, maxDepth, expected]) {
  if (Array.isArray(expected)) {
    assert.deepEqual(parseLinoToLinks(text, { maxDepth }), expected);
    return;
  }
  let thrown = null;
  try {
    parseLinoToLinks(text, { maxDepth });
  } catch (error) {
    thrown = error;
  }
  assert.ok(thrown instanceof LinoParseError, `${JSON.stringify(text)} should not parse`);
  const { message, ...fields } = expected;
  assert.equal(thrown.message, message);
  for (const [field, value] of Object.entries(fields)) assert.deepEqual(thrown[field], value);
}

function table(name, rows) {
  describe(name, () => {
    for (const row of rows) it(JSON.stringify(row[0]).slice(0, 60), () => check(row));
  });
}

table('syntax errors', SYNTAX);
table('nesting depth', DEPTH);
table('comments', COMMENTS);
table('delimited references', QUOTES);
table('reference literals', LITERALS);
table('multi-line and indented documents', DOCUMENTS);
table('long lines', LONG);

describe('parseLinoToLinks', () => {
  it('nests at most 64 levels unless told otherwise', () => {
    assert.equal(DEFAULT_MAX_DEPTH, 64);
    const parens = (depth) => `${'('.repeat(depth)}a${')'.repeat(depth)}`;
    // Each group around `(a)` wraps it once more.
    const nested = (depth) =>
      depth === 1 ? { id: null, values: ['a'] } : { id: null, values: [nested(depth - 1)] };
    assert.deepEqual(parseLinoToLinks(parens(64)), [nested(64)]);
    assert.throws(() => parseLinoToLinks(parens(65)), {
      kind: 'depth',
      maxDepth: 64,
      column: 65,
    });
    assert.deepEqual(parseLinoToLinks(parens(3), {}), [nested(3)]);
  });

  it('exports the error class it throws', () => {
    assert.equal(ExportedLinoParseError, LinoParseError);
    assert.throws(() => parseLinoToLinks(')'), ExportedLinoParseError);
  });

  it('keeps the line of a syntax error', () => {
    assert.throws(() => parseLinoToLinks('x\r\n  y: (z'), { lineText: '  y: (z', line: 2 });
  });
});

describe('LinoGrammar', () => {
  const grammar = (text) => {
    const bytes = encodeUtf8(text);
    const references = new DelimitedReferences(bytes);
    return new LinoGrammar(stripComments(bytes, references), references, 64);
  };

  it('reports where nom stopped when nothing was tracked', () => {
    assert.deepEqual(grammar('abc').failure({ err: 1, at: 2 }), {
      offset: 2,
      expected: [],
      maxDepth: null,
    });
    assert.deepEqual(grammar('abc').failure({ err: 1, at: 9 }).offset, 3);
  });

  it('counts indentation from zero before the first line', () => {
    assert.equal(grammar('a').normalizeIndentation(3), 3);
  });
});

describe('LinoParseError', () => {
  it('prints the empty input and internal errors as Rust does', () => {
    const empty = LinoParseError.empty();
    assert.equal(empty.message, 'Empty input');
    assert.equal(empty.kind, 'empty');
    assert.equal(empty.name, 'LinoParseError');
    assert.deepEqual(
      [empty.line, empty.column, empty.offset, empty.found, empty.lineText, empty.maxDepth],
      [null, null, null, null, null, null],
    );
    assert.deepEqual(empty.expected, []);
    const internal = LinoParseError.internal('lost');
    assert.equal(internal.message, 'Internal error: lost');
    assert.equal(internal.kind, 'internal');
    assert.ok(internal instanceof Error);
  });

  it('says what was unexpected when nothing was expected', () => {
    const error = parseError(encodeUtf8('aé'), { offset: 1, expected: [], maxDepth: null });
    assert.equal(error.message, 'Syntax error at line 1, column 2: unexpected "é"\n1 | aé\n  |  ^');
  });

  it('names one, two and more alternatives', () => {
    const at = (expected) =>
      parseError(encodeUtf8('a'), { offset: 1, expected, maxDepth: null }).message.split('\n')[0];
    assert.equal(at(['x']), 'Syntax error at line 1, column 2: expected x, found end of input');
    assert.equal(
      at(['x', 'y']),
      'Syntax error at line 1, column 2: expected x or y, found end of input',
    );
    assert.equal(
      at(['x', 'y', 'z']),
      'Syntax error at line 1, column 2: expected x, y or z, found end of input',
    );
  });

  it('finds the character at the offset, astral ones whole', () => {
    const error = parseError(encodeUtf8('a\n😀b'), {
      offset: 2,
      expected: ['x'],
      maxDepth: null,
    });
    assert.equal(error.found, '😀');
    assert.deepEqual([error.line, error.column, error.offset], [2, 1, 2]);
  });
});

describe('escapeDebug', () => {
  it('escapes as char::escape_debug does', () => {
    const cases = [
      ['\0', '\\0'],
      ['\t', '\\t'],
      ['\r', '\\r'],
      ['\n', '\\n'],
      ['\\', '\\\\'],
      ['"', '\\"'],
      ["'", "\\'"],
      ['\u0001', '\\u{1}'],
      ['\u007f', '\\u{7f}'],
      ['\u0085', '\\u{85}'],
      [' ', '\\u{a0}'],
      ['­', '\\u{ad}'],
      ['̀', '\\u{300}'],
      ['͸', '\\u{378}'],
      [' ', '\\u{1680}'],
      [' ', '\\u{2000}'],
      ['​', '\\u{200b}'],
      [' ', '\\u{2028}'],
      ['　', '\\u{3000}'],
      ['', '\\u{e000}'],
      ['﻿', '\\u{feff}'],
      ['\u{e0001}', '\\u{e0001}'],
      ['\u{e0100}', '\\u{e0100}'],
      ['\u{10ffff}', '\\u{10ffff}'],
      [' ', ' '],
      ['a', 'a'],
      [':', ':'],
      ['é', 'é'],
      ['\u{1f600}', '\u{1f600}'],
    ];
    for (const [character, escaped] of cases) assert.equal(escapeDebug(character), escaped);
  });
});

describe('reference literals', () => {
  const literal = (text) => {
    const bytes = encodeUtf8(text);
    const end = referenceLiteralEnd(bytes, 0);
    return end < 0 ? undefined : decodeReferenceLiteral(bytes, 0, prefixEnd(bytes, 0), end);
  };

  it('finds where a literal is written', () => {
    assert.equal(prefixEnd(encodeUtf8('~1{61}'), 0), 3);
    assert.equal(prefixEnd(encodeUtf8('~12{61}'), 0), 4);
    assert.equal(prefixEnd(encodeUtf8('~{61}'), 0), -1);
    assert.equal(prefixEnd(encodeUtf8('~1x'), 0), -1);
    assert.equal(prefixEnd(encodeUtf8('a~1{'), 0), -1);
    assert.equal(referenceLiteralEnd(encodeUtf8('~1{61} b'), 0), 6);
    assert.equal(referenceLiteralEnd(encodeUtf8('~1{61 62}'), 0), -1);
    assert.equal(referenceLiteralEnd(encodeUtf8('~1{61'), 0), -1);
    assert.equal(referenceLiteralEnd(encodeUtf8('~1x'), 0), -1);
  });

  it('decodes version 1 hex as UTF-8', () => {
    assert.equal(literal('~1{}'), '');
    assert.equal(literal('~1{61}'), 'a');
    assert.equal(literal('~1{6A6b}'), 'jk');
    assert.equal(literal('~1{f09f9880}'), '\u{1f600}');
    assert.equal(literal('~1{efbbbf}'), '﻿');
  });

  it('rejects other versions, odd or bad digits and invalid UTF-8', () => {
    assert.equal(literal('~2{61}'), null);
    assert.equal(literal('~12{61}'), null);
    assert.equal(literal('~01{61}'), null);
    assert.equal(literal('~1{616}'), null);
    assert.equal(literal('~1{zz}'), null);
    assert.equal(literal('~1{6g}'), null);
    assert.equal(literal('~1{ff}'), null);
    assert.equal(literal('~1{c3}'), null);
  });
});

describe('delimited references', () => {
  const read = (text, start = 0) => new DelimitedReferences(encodeUtf8(text)).read(start);

  it('knows the three delimiters', () => {
    assert.deepEqual(
      ['"', "'", '`', 'a', '#'].map((c) => isQuote(c.charCodeAt(0))),
      [true, true, true, false, false],
    );
  });

  it('reads nothing where no delimiter opens', () => {
    assert.equal(read('abc'), null);
  });

  it('closes on the first run of an odd multiple of the opening run', () => {
    assert.deepEqual(read('"a" b'), { value: 'a', length: 3 });
    assert.deepEqual(read('""a"b"" c'), { value: 'a"b', length: 7 });
    assert.deepEqual(read('"a""b" c'), { value: 'a"b', length: 6 });
    assert.deepEqual(read('``a```` b```'), { value: 'a`` b`', length: 12 });
    assert.deepEqual(read("'é\u{1f600}'"), { value: 'é\u{1f600}', length: 8 });
  });

  it('reads from inside a run as a shorter run', () => {
    assert.deepEqual(read('x""a"', 2), { value: 'a', length: 3 });
  });

  it('leaves an odd run unclosed and reads an even one as empty', () => {
    assert.equal(read('"abc'), null);
    assert.equal(read('"""abc'), null);
    assert.deepEqual(read('""abc'), { value: '', length: 2 });
  });

  it('reads an even run as empty unless its body is balanced and visible', () => {
    assert.deepEqual(read('""(a)"" x'), { value: '(a)', length: 7 });
    assert.deepEqual(read('""  "" x'), { value: '', length: 2 });
    assert.deepEqual(read('"")(""'), { value: '', length: 2 });
    assert.deepEqual(read('""(a""'), { value: '', length: 2 });
  });

  it('remembers what it read', () => {
    const references = new DelimitedReferences(encodeUtf8('"a"'));
    assert.equal(references.read(0), references.read(0));
  });
});

describe('stripComments', () => {
  const strip = (text) => {
    const bytes = encodeUtf8(text);
    return new TextDecoder().decode(stripComments(bytes, new DelimitedReferences(bytes)));
  };

  it('blanks comments byte for byte to the end of the line', () => {
    assert.equal(strip('# x\ny'), '   \ny');
    assert.equal(strip('a #é\r\nb'), 'a    \r\nb');
    assert.equal(strip('a\t# b'), 'a\t   ');
  });

  it('keeps a # inside a token, a delimited reference or a literal', () => {
    assert.equal(strip('a#b'), 'a#b');
    assert.equal(strip('"#" #c'), '"#"   ');
    assert.equal(strip('~1{23} #c'), '~1{23}   ');
    assert.equal(strip('x"#"'), 'x"#"');
    assert.equal(strip('(#a)'), '(#a)');
  });

  it('leaves the document it was given alone', () => {
    const bytes = encodeUtf8('a # b');
    stripComments(bytes, new DelimitedReferences(bytes));
    assert.equal(new TextDecoder().decode(bytes), 'a # b');
  });
});
