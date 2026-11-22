// Tests of src/protocol-lino.js. The expected texts come from links-notation
// 0.31.6 (src/binary/format.rs: format_reference, format_link,
// format_document, canonical, parse_document) and from
// rust/src/protocol/server.rs and server_query.rs (error_document,
// error_message, query_document, admit_document, parse_document_with_limits),
// checked against the Rust crates; the limit cases repeat
// rust/tests/engine_causal_protocol_tests.rs
// (bounded_protocol_parser_admits_grammar_before_canonical_conversion).
import { describe, test } from 'node:test';
import assert from 'node:assert/strict';

import { ProtocolError } from '../src/protocol-errors.js';
import { LinkError } from '../src/errors.js';
import { QueryLimits } from '../src/query-options.js';
import {
  admitDocument,
  byteLength,
  canonical,
  encodeReferenceLiteral,
  errorDocument,
  errorMessage,
  formatDocument,
  formatLink,
  formatReference,
  isRef,
  linoLink,
  parseDocument,
  parseDocumentWithLimits,
  queryDocument,
  rustTrim,
  sameNode,
} from '../src/protocol-lino.js';

const link = linoLink;
const limits = (overrides) => ({ maxBytes: 1000, maxDepth: 64, maxNodes: 100, ...overrides });

/** Asserts that `run` throws a ProtocolError of `kind` whose message is `message`. */
function throwsProtocol(run, kind, message) {
  assert.throws(run, (error) => {
    assert.ok(error instanceof ProtocolError);
    assert.equal(error.kind, kind);
    assert.equal(error.message, message);
    return true;
  });
}

describe('nodes', () => {
  test('linoLink, isRef, byteLength and encodeReferenceLiteral', () => {
    assert.deepEqual(linoLink('a', ['b']), { id: 'a', values: ['b'] });
    assert.equal(isRef('a'), true);
    assert.equal(isRef(link(null, [])), false);
    assert.equal(byteLength('aé€😀'), 1 + 2 + 3 + 4);
    assert.equal(encodeReferenceLiteral('a\r\nb'), '~1{610d0a62}');
    assert.equal(encodeReferenceLiteral(''), '~1{}');
  });

  test('rustTrim strips Unicode White_Space as str::trim does', () => {
    assert.equal(rustTrim(' \t　a b \n'), 'a b');
    assert.equal(rustTrim('　'), '');
    assert.equal(rustTrim('﻿a'), '﻿a');
  });

  test('sameNode compares as PartialEq', () => {
    assert.equal(sameNode('a', 'a'), true);
    assert.equal(sameNode('a', 'b'), false);
    assert.equal(sameNode('a', link(null, ['a'])), false);
    assert.equal(sameNode(link(null, ['a']), 'a'), false);
    assert.equal(sameNode([], []), true);
    assert.equal(sameNode(['a', link('x', ['b'])], ['a', link('x', ['b'])]), true);
    assert.equal(sameNode(['a'], ['a', 'b']), false);
    assert.equal(sameNode(['a'], ['b']), false);
    assert.equal(sameNode([], link(null, [])), false);
    assert.equal(sameNode(link(null, []), []), false);
    assert.equal(sameNode(link('a', ['b']), link('a', ['b'])), true);
    assert.equal(sameNode(link('a', ['b']), link(null, ['b'])), false);
    assert.equal(sameNode(link('a', ['b']), link('a', ['c'])), false);
  });
});

describe('formatReference', () => {
  test('bare, quoted or literal as format_reference', () => {
    const cases = [
      ['plain', 'plain'],
      ['$x', '$x'],
      ['~x', '~x'],
      ['a#', 'a#'],
      ['', '~1{}'],
      ['\n', '~1{0a}'],
      ['\u007f', '~1{7f}'],
      ['a\r\nb', '~1{610d0a62}'],
      ['a b', "'a b'"],
      ['(x)', "'(x)'"],
      ['a:b', "'a:b'"],
      ['~1{00}', "'~1{00}'"],
      ['#tag', "'#tag'"],
      ['﻿x', "'﻿x'"],
      ['　', "'　'"],
      ["it's", '"it\'s"'],
      ['\'"', '`\'"`'],
      ['\'"`', '"""\'"`"""'],
      ['``x', "'``x'"],
      ['"x', "'\"x'"],
      ["a''b", '"a\'\'b"'],
      ['x\'\'\'y"""z```', "'''''x'''y\"\"\"z```'''''"],
    ];
    for (const [text, expected] of cases) assert.equal(formatReference(text), expected, text);
  });

  test('every formatted reference parses back to itself', () => {
    const references = [
      ...['', 'a b', 'a:b', '(x)', "it's", '\'"', '\'"`', 'tab\there', 'a\nb', 'a\r\nb'],
      ...["'x", "x'", "'", "''", "'''", '"\'`x`\'"', "a''''b\"\"`", "`'\"\"\"'''``", "(a) ''"],
    ];
    for (const text of references) {
      assert.deepEqual(parseDocument(formatReference(text)), [text], JSON.stringify(text));
    }
  });

  test('short references over the delimiters round trip', () => {
    // every_short_reference_over_delimiters_round_trips, lengths 1..3 to keep it cheap.
    const alphabet = ["'", '"', '`', 'a', ' ', '(', ')', ':'];
    let texts = [''];
    for (let length = 1; length <= 3; length += 1) {
      texts = texts.flatMap((text) => alphabet.map((character) => text + character));
      for (const text of texts) {
        assert.deepEqual(parseDocument(formatReference(text)), [text], JSON.stringify(text));
      }
    }
  });

  test('legacy quoting parses to the same references', () => {
    assert.deepEqual(parseDocument("''"), parseDocument('~1{}'));
    assert.deepEqual(parseDocument("'a\nb'"), parseDocument('~1{610a62}'));
  });
});

describe('formatLink and formatDocument', () => {
  test('format_link nests links', () => {
    assert.equal(formatLink('a b'), "'a b'");
    assert.equal(formatLink(link(null, ['a'])), '((a))');
    assert.equal(formatLink(link(null, [])), '()');
    assert.equal(formatLink(link(null, [link(null, [])])), '(())');
    assert.equal(formatLink(link(null, ['a', 'b'])), '(a b)');
    assert.equal(formatLink(link('a', [])), '(a:)');
    assert.equal(formatLink(link('a b', ['c', link(null, ['d'])])), "('a b': c ((d)))");
  });

  test('format_document writes one top-level link per line', () => {
    const cases = [
      ['(()((1 1)))', '() ((1 1))'],
      ['((1: 1 1)) ((1: 1 2))', '((1: 1 1)) ((1: 1 2))'],
      ['(a:)', 'a'],
      ['((a))', '((a))'],
      ['(a: (b))', '(a: b)'],
      ['(a (b) c)', 'a b c'],
      ['a\nb c', 'a\nb c'],
      ['(a)', 'a'],
      ['(a: b c)', '(a: b c)'],
      ['((a: b))', '((a: b))'],
    ];
    for (const [text, expected] of cases) {
      const document = parseDocument(text);
      assert.equal(formatDocument(document), expected, text);
      assert.deepEqual(parseDocument(formatDocument(document)), document, text);
    }
    assert.equal(formatDocument([]), '');
    assert.equal(formatDocument([link(null, ['a'])]), '((a))');
    assert.equal(formatDocument([link('x', [])]), '(x:)');
  });

  test('parsed documents are canonical', () => {
    assert.deepEqual(parseDocument('(()((1 1)))'), [
      link(null, [link(null, []), link(null, [link(null, ['1', '1'])])]),
    ]);
    assert.deepEqual(parseDocument('(a:)'), ['a']);
    assert.deepEqual(parseDocument('(a: (b))'), [link('a', ['b'])]);
    assert.deepEqual(parseDocument('((a))'), [link(null, ['a'])]);
  });
});

describe('canonical', () => {
  test('an unnamed group of one reference is that reference', () => {
    assert.equal(canonical('a'), 'a');
    assert.equal(canonical(link(null, ['a'])), 'a');
    assert.deepEqual(canonical(link(null, [link(null, ['a'])])), link(null, ['a']));
    assert.deepEqual(canonical(link('x', ['a'])), link('x', ['a']));
    assert.deepEqual(
      canonical(link(null, ['a', link(null, ['b']), link(null, [])])),
      link(null, ['a', 'b', link(null, [])]),
    );
  });
});

describe('parseDocument', () => {
  test('blank text (spaces, tabs, line ends) is the empty document', () => {
    assert.deepEqual(parseDocument(''), []);
    assert.deepEqual(parseDocument(' \t\r\n'), []);
    // Not blank for binary::parse_document: other white space is a reference.
    assert.deepEqual(parseDocument('　'), ['　']);
    assert.deepEqual(parseDocument('　a'), ['　a']);
  });

  test('a syntax error is invalid LiNo', () => {
    throwsProtocol(
      () => parseDocument('('),
      'invalidLino',
      'invalid LiNo: Syntax error at line 1, column 2: expected "(", a reference or ")", found end of input\n1 | (\n  |  ^',
    );
    throwsProtocol(
      () => parseDocument(')'),
      'invalidLino',
      'invalid LiNo: Syntax error at line 1, column 1: expected "(" or a reference, found ")"\n1 | )\n  | ^',
    );
    throwsProtocol(
      () => parseDocument('(a: b'),
      'invalidLino',
      'invalid LiNo: Syntax error at line 1, column 6: expected "(", a reference or ")", found end of input\n1 | (a: b\n  |      ^',
    );
  });

  test('nesting is bounded by the default parser depth', () => {
    const nested = (depth) => `${'('.repeat(depth)}a${')'.repeat(depth)}`;
    let node = parseDocument(nested(40))[0];
    let depth = 1;
    while (!isRef(node.values[0])) {
      node = node.values[0];
      depth += 1;
    }
    assert.equal(depth, 39);
    assert.deepEqual(node, link(null, ['a']));
    assert.throws(
      () => parseDocument(nested(100000)),
      (error) =>
        error.kind === 'invalidLino' &&
        error.message.startsWith(
          'invalid LiNo: Nesting too deep at line 1, column 65: nesting depth exceeds the maximum of 64\n',
        ),
    );
  });
});

describe('admitDocument', () => {
  test('admits a document within the limits', () => {
    assert.equal(admitDocument([], limits({ maxNodes: 0 })), undefined);
    assert.equal(
      admitDocument([link('abc', ['d'])], limits({ maxBytes: 4, maxNodes: 2 })),
      undefined,
    );
  });

  test('rejects too many top-level or nested nodes', () => {
    const nodes = { name: 'LinkError', message: 'protocol node limit exceeded' };
    assert.throws(() => admitDocument(['a', 'b', 'c'], limits({ maxNodes: 2 })), nodes);
    assert.throws(() => admitDocument([link('a', ['b', 'c'])], limits({ maxNodes: 2 })), nodes);
    assert.throws(() => admitDocument(['a', 'b', 'c'], limits({ maxNodes: 2 })), LinkError);
  });

  test('rejects nodes deeper than maxDepth', () => {
    const depth = { name: 'LinkError', message: 'protocol depth limit exceeded' };
    assert.throws(() => admitDocument([link(null, ['a'])], limits({ maxDepth: 0 })), depth);
    assert.equal(admitDocument([link(null, ['a'])], limits({ maxDepth: 1 })), undefined);
  });

  test('counts the bytes of references and names', () => {
    const bytes = { name: 'LinkError', message: 'protocol byte limit exceeded' };
    assert.throws(() => admitDocument(['abcd'], limits({ maxBytes: 3 })), bytes);
    assert.throws(() => admitDocument([link('abcd', [])], limits({ maxBytes: 3 })), bytes);
    assert.throws(() => admitDocument([link('ab', ['cd'])], limits({ maxBytes: 3 })), bytes);
    assert.throws(() => admitDocument(['é€'], limits({ maxBytes: 4 })), bytes);
    assert.equal(admitDocument([link(null, ['ab', 'c'])], limits({ maxBytes: 3 })), undefined);
  });
});

describe('parseDocumentWithLimits', () => {
  test('the Rust causal limits cases', () => {
    const causal = QueryLimits.causal();
    assert.deepEqual(parseDocumentWithLimits(' \n', causal), []);
    const query = "(query: '(((:)))')";
    assert.deepEqual(parseDocumentWithLimits(query, causal), parseDocument(query));
    assert.deepEqual(parseDocumentWithLimits(query, causal), [link('query', ['(((:)))'])]);
    throwsProtocol(
      () => parseDocumentWithLimits('(', causal),
      'invalidLino',
      'invalid LiNo: Syntax error at line 1, column 2: expected "(", a reference or ")", found end of input\n1 | (\n  |  ^',
    );
    assert.throws(
      () => parseDocumentWithLimits(`${'('.repeat(70)}x${')'.repeat(70)}`, causal),
      (error) =>
        error.kind === 'invalidLino' &&
        error.message.startsWith(
          'invalid LiNo: Nesting too deep at line 1, column 65: nesting depth exceeds the maximum of 64\n',
        ),
    );
    throwsProtocol(
      () => parseDocumentWithLimits('x '.repeat(4097), causal),
      'limitExceeded',
      'limit exceeded: protocol node limit exceeded',
    );
    throwsProtocol(
      () => parseDocumentWithLimits('x'.repeat(1048577), causal),
      'limitExceeded',
      'limit exceeded: protocol byte limit exceeded',
    );
  });

  test('blank text after str::trim is the empty document', () => {
    assert.deepEqual(parseDocumentWithLimits('　\t', limits()), []);
    assert.deepEqual(parseDocumentWithLimits('', limits()), []);
  });

  test('the parser depth is maxDepth, the document depth is checked after', () => {
    const tooDeep = (column, max, text) =>
      `invalid LiNo: Nesting too deep at line 1, column ${column}: nesting depth exceeds the maximum of ${max}\n1 | ${text}\n  | ${' '.repeat(column - 1)}^`;
    throwsProtocol(
      () => parseDocumentWithLimits('(a b)', limits({ maxDepth: 0 })),
      'invalidLino',
      tooDeep(1, 0, '(a b)'),
    );
    throwsProtocol(
      () => parseDocumentWithLimits('((a b))', limits({ maxDepth: 1 })),
      'invalidLino',
      tooDeep(2, 1, '((a b))'),
    );
    assert.deepEqual(parseDocumentWithLimits('((a b))', limits({ maxDepth: 2 })), [
      link(null, [link(null, ['a', 'b'])]),
    ]);
    for (const text of ['a', 'a b']) {
      throwsProtocol(
        () => parseDocumentWithLimits(text, limits({ maxDepth: 0 })),
        'limitExceeded',
        'limit exceeded: protocol depth limit exceeded',
      );
    }
  });

  test('node and byte limits', () => {
    const exceeded = (what) => `limit exceeded: protocol ${what} limit exceeded`;
    throwsProtocol(
      () => parseDocumentWithLimits('(a: b c)', limits({ maxNodes: 2 })),
      'limitExceeded',
      exceeded('node'),
    );
    assert.deepEqual(parseDocumentWithLimits('(a: b c)', limits({ maxNodes: 3 })), [
      link('a', ['b', 'c']),
    ]);
    throwsProtocol(
      () => parseDocumentWithLimits('a\nb\nc', limits({ maxNodes: 2 })),
      'limitExceeded',
      exceeded('node'),
    );
    throwsProtocol(
      () => parseDocumentWithLimits('(abc: d)', limits({ maxBytes: 3 })),
      'limitExceeded',
      exceeded('byte'),
    );
    assert.deepEqual(parseDocumentWithLimits('(abc: d)', limits({ maxBytes: 8 })), [
      link('abc', ['d']),
    ]);
    // The text limit counts UTF-8 bytes: 2 characters, 7 bytes.
    throwsProtocol(
      () => parseDocumentWithLimits('€😀', limits({ maxBytes: 6 })),
      'limitExceeded',
      exceeded('byte'),
    );
  });
});

describe('error and query documents', () => {
  test('errorDocument and queryDocument', () => {
    assert.deepEqual(errorDocument('boom'), [link('error', ['boom'])]);
    assert.equal(formatDocument(errorDocument('it failed')), "(error: 'it failed')");
    assert.deepEqual(queryDocument('(a b)'), [link('query', ['(a b)'])]);
    assert.equal(formatDocument(queryDocument('(a b)')), "(query: '(a b)')");
  });

  test('errorMessage reads only a single (error: message)', () => {
    assert.equal(errorMessage(errorDocument('boom')), 'boom');
    assert.equal(errorMessage(parseDocument("(error: 'it failed')")), 'it failed');
    assert.equal(errorMessage([]), null);
    assert.equal(errorMessage([...errorDocument('a'), ...errorDocument('b')]), null);
    assert.equal(errorMessage(['error']), null);
    assert.equal(errorMessage([link('other', ['boom'])]), null);
    assert.equal(errorMessage([link(null, ['boom'])]), null);
    assert.equal(errorMessage([link('error', [])]), null);
    assert.equal(errorMessage([link('error', ['a', 'b'])]), null);
    assert.equal(errorMessage([link('error', [link(null, ['a'])])]), null);
  });
});
