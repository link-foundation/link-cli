// The line grammar of a text links database. The error texts are the ones the
// Rust clink reports for the same name metadata.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import {
  decodeNameLiteral,
  NAME_HEADER,
  nameMetadata,
  needsNameMetadata,
  parseLinkLine,
  parseNameMetadata,
  splitWhitespace,
  trim,
  trimStart,
} from '../src/text-storage-format.js';

const row = (line) => {
  const parsed = parseLinkLine(line);
  return parsed === null
    ? null
    : [parsed.link.index, parsed.link.source, parsed.link.target, parsed.name];
};

describe('Rust whitespace', () => {
  it('trims and splits on char::is_whitespace', () => {
    assert.equal(trimStart(' \u0085\t a b '), 'a b ');
    assert.equal(trim('　a b '), 'a b');
    assert.equal(trim('﻿a'), '﻿a');
    assert.deepEqual(splitWhitespace('  a  b\n c '), ['a', 'b', 'c']);
    assert.deepEqual(splitWhitespace(' \t '), []);
  });
});

describe('name metadata', () => {
  it('is needed for names with control characters', () => {
    assert.equal(needsNameMetadata('plain name'), false);
    assert.equal(needsNameMetadata('a\tb'), true);
    assert.equal(needsNameMetadata('a\x7f'), true);
    assert.equal(nameMetadata(2, 'a\tb'), `${NAME_HEADER}1 2 ~1{610962}`);
  });

  it('reads back what it writes', () => {
    assert.deepEqual(parseNameMetadata('1 9 ~1{67686f7374}'), [9, 'ghost']);
    assert.deepEqual(parseNameMetadata(' 1  2 ~1{}'), [2, '']);
    assert.equal(decodeNameLiteral('~1{C3A9}'), 'é');
  });

  it('reports what the Rust clink reports', () => {
    for (const [metadata, message] of [
      ['1 2', 'invalid or unsupported name metadata'],
      ['2 2 ~1{61}', 'invalid or unsupported name metadata'],
      ['1 2 abc', 'invalid reference literal'],
      ['1 2 ~2{61}', 'invalid or unsupported reference literal'],
      ['1 2 ~1{61', 'invalid or unsupported reference literal'],
      ['1 2 ~1{6}', 'reference literal requires an even number of hex digits'],
      ['1 2 ~1{6g}', 'reference literal requires an even number of hex digits'],
      ['1 2 ~1{ff}', 'reference literal must contain valid UTF-8'],
      ['1 4294967296 ~1{61}', 'number too large to fit in target type'],
      ['1 +2 ~1{61}', 'invalid name metadata address'],
    ]) {
      assert.throws(() => parseNameMetadata(metadata), { message }, metadata);
    }
  });
});

describe('rows', () => {
  it('reads the rows the Rust clink reads', () => {
    assert.deepEqual(row('(1 2 3)'), [1, 2, 3, null]);
    assert.deepEqual(row('((9 1 1))'), [9, 1, 1, null]);
    assert.deepEqual(row('+5 1 1'), [5, 1, 1, null]);
    assert.deepEqual(row('14\t1  1   "tab name"'), [14, 1, 1, 'tab name']);
    assert.deepEqual(row('(13 1 1 ""q"")'), [13, 1, 1, '"q"']);
    assert.deepEqual(row('(6 1 1 "x'), [6, 1, 1, 'x']);
    assert.deepEqual(row('7 1 1 x y'), [7, 1, 1, 'x']);
    assert.deepEqual(row('8 1 1 "a" b'), [8, 1, 1, 'a']);
    assert.deepEqual(row('(12 1 1 ")'), [12, 1, 1, '']);
    assert.deepEqual(row('4294967295 0 4294967295'), [0xffffffff, 0, 0xffffffff, null]);
  });

  it('skips lines that are not rows', () => {
    for (const line of [
      '',
      '()',
      'junk',
      '(4294967296 1 1)',
      '(9)',
      '(10 1)',
      '(11 1 x)',
      '(-1 1 1)',
    ]) {
      assert.equal(row(line), null, line);
    }
  });
});
