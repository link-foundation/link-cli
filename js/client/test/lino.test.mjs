import assert from 'node:assert/strict';
import test from 'node:test';
import { decodeReferenceLiteral, encodeReferenceLiteral, formatDocument, formatReference, group, named, parseDocument } from '../src/lino.js';

test('references stay bare unless the notation needs them quoted', () => {
  assert.equal(formatReference('42'), '42');
  assert.equal(formatReference('café'), 'café');
  assert.equal(formatReference('a pair'), "'a pair'");
  assert.equal(formatReference("it's"), '"it\'s"');
  assert.equal(formatReference('\'a\' "b"'), '`\'a\' "b"`');
  assert.equal(formatReference('#tag'), "'#tag'");
  assert.equal(formatReference('~1{41}'), "'~1{41}'");
  assert.equal(formatReference('a:b'), "'a:b'");
  assert.equal(formatReference('a b'), "'a b'");
});

test('empty text and control characters become ~1{hex} literals', () => {
  assert.equal(formatReference(''), '~1{}');
  assert.equal(formatReference('a\nb'), '~1{610a62}');
  assert.equal(encodeReferenceLiteral('\u0000'), '~1{00}');
  assert.equal(decodeReferenceLiteral('~1{e4bda0e5a5bd}'), '你好');
  assert.throws(() => decodeReferenceLiteral('~1{6}'), /even number/);
  assert.throws(() => decodeReferenceLiteral('~1{ff}'), /UTF-8/);
});

test('the formats the servers write parse into the model', () => {
  assert.deepEqual(parseDocument('() ((1: 1 1))'), [group([group([]), group([named('1', ['1', '1'])])])]);
  assert.deepEqual(parseDocument('(count: 2)'), [named('count', ['2'])]);
  assert.deepEqual(parseDocument("(name: 'a pair')"), [named('name', ['a pair'])]);
  assert.deepEqual(parseDocument('(name: ~1{})'), [named('name', [''])]);
  assert.deepEqual(parseDocument('(1: 1 1)\n(2: 1 2)\n'), [named('1', ['1', '1']), named('2', ['1', '2'])]);
  assert.deepEqual(parseDocument(''), []);
  assert.deepEqual(parseDocument('((1: 1 1)) ()'), [group([group([named('1', ['1', '1'])]), group([])])]);
});

test('formatting matches the canonical text of links-notation', () => {
  assert.equal(formatDocument([named('create', [group(['1', '1'])])]), '(create: (1 1))');
  assert.equal(formatDocument([named('delete', ['7'])]), '(delete: 7)');
  assert.equal(formatDocument([group([group([]), group([named('1', ['1', '1'])])])]), '() ((1: 1 1))');
  assert.equal(formatDocument([named('count', [group(['*', '*', '2'])])]), '(count: (* * 2))');
  assert.equal(formatDocument([named('remove-name', [])]), '(remove-name:)');
});

test('every reference round-trips, Unicode and quotes included', () => {
  const samples = [
    'plain',
    'with space',
    "it's",
    '"double"',
    '`back`',
    "'''",
    'mixed \'"` quotes',
    'línea española',
    'Привет мир',
    '中文 名字',
    'emoji 🧪🔗',
    '',
    'tab\there',
    'new\nline',
    '~1{41}',
    '#hash',
    'a:b',
    '(paren)',
    '﻿bom',
    '18446744073709551615',
  ];
  for (const text of samples) {
    const document = [named('set-name', [group(['1', text])])];
    assert.deepEqual(parseDocument(formatDocument(document)), document, JSON.stringify(text));
  }
});

test('malformed text is refused', () => {
  assert.throws(() => parseDocument('(a'), { name: 'MalformedMessageError' });
  assert.throws(() => parseDocument("'unterminated"), { name: 'MalformedMessageError' });
});

test('comments follow the upstream token boundaries and preserve quoted content', () => {
  assert.deepEqual(parseDocument('# unmatched ) and quote "\r\n'), []);
  assert.deepEqual(parseDocument('a b # ignored )\nc d#kept\r# end'), [group(['a', 'b']), group(['c', 'd#kept'])]);
  assert.deepEqual(parseDocument('(a # ignored (\n b)\n(x:#literal)\n(#literal)'), [group(['a', 'b']), named('x', ['#literal']), '#literal']);
  assert.deepEqual(parseDocument("'#kept'\n(a: '# kept )') # ignored\n"), ['#kept', named('a', ['# kept )'])]);
  assert.deepEqual(parseDocument('\t# comment at EOF'), []);
});

test('optional LiNo limits admit bytes, grammar depth and syntax nodes before recursive work', () => {
  // UTF-8 cannot be shorter than the UTF-16 code-unit count. Reject this
  // finite oversized input before allocating a second encoded copy.
  const encode = TextEncoder.prototype.encode;
  TextEncoder.prototype.encode = () => { throw new Error('encoded an already oversized request'); };
  try {
    assert.throws(() => parseDocument('x'.repeat(9), { maxBytes: 8 }), { name: 'LimitExceededError' });
  } finally { TextEncoder.prototype.encode = encode; }
  assert.deepEqual(parseDocument('', { maxBytes: 0, maxDepth: 0, maxNodes: 0 }), []);
  assert.deepEqual(parseDocument('(x: y)', { maxBytes: 8, maxDepth: 1, maxNodes: 3 }), [named('x', ['y'])]);
  assert.deepEqual(parseDocument('a b', { maxBytes: 3, maxNodes: 3 }), [group(['a', 'b'])]);
  for (const limits of [{ maxBytes: -1 }, { maxDepth: NaN }, { maxNodes: Infinity }, { maxDepth: 0.5 }]) {
    assert.throws(() => parseDocument('', limits), TypeError);
  }
  for (const [text, limits] of [
    ['雪', { maxBytes: 2 }], ['(x)', { maxDepth: 0 }], ['a b', { maxNodes: 2 }],
    ['(x: y)', { maxNodes: 2 }], ['('.repeat(70) + 'x' + ')'.repeat(70), { maxDepth: 64 }],
  ]) {
    assert.throws(() => parseDocument(text, limits), error => error.name === 'LimitExceededError' && error.outcome === 'not-sent');
  }
  // Parentheses and colons inside quoted references do not change grammar depth.
  assert.deepEqual(parseDocument("'(((:)))'", { maxDepth: 0, maxNodes: 1 }), ['(((:)))']);
});
