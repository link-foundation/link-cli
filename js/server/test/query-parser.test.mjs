// The query parser: names in their quote envelope, addresses, variables and
// wildcards kept bare, and the admission limits of a checked store. Port of
// the behaviour of rust/src/query_parser.rs.
import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { ParseError, QueryError } from '../src/errors.js';
import { QueryLimits } from '../src/query-options.js';
import { formatIdentifier, name, parse, parseFor, rustTrim } from '../src/query-parser.js';

const leaf = (id) => ({ id, values: null });
const node = (id, ...values) => ({ id, values });

const limits = (overrides) =>
  new QueryLimits({
    maxAddress: 10,
    maxBytes: 100,
    maxDepth: 2,
    maxNodes: 5,
    maxResults: 1,
    maxBindings: 1,
    maxBindingBytes: 100,
    ...overrides,
  });

describe('rustTrim', () => {
  it('strips Unicode White_Space only', () => {
    assert.equal(rustTrim('　 a b\u0085\n '), 'a b');
    assert.equal(rustTrim('​ a'), '​ a', 'a zero-width space is no White_Space');
    assert.equal(rustTrim('\t\v\f\r '), '');
  });
});

describe('name', () => {
  it('removes one quote envelope', () => {
    assert.equal(name('"a"'), 'a');
    assert.equal(name('""a""'), '"a"');
    assert.equal(name('""'), '');
    assert.equal(name('"'), '"');
    assert.equal(name('a"'), 'a"');
    assert.equal(name('"a'), '"a');
    assert.equal(name('12'), '12');
  });
});

describe('formatIdentifier', () => {
  it('keeps syntax and writes safe names bare', () => {
    assert.equal(formatIdentifier('abc'), 'abc');
    assert.equal(formatIdentifier('$x'), '$x');
    assert.equal(formatIdentifier('"abc"'), 'abc');
    assert.equal(formatIdentifier('"é"'), 'é');
    assert.equal(formatIdentifier('"4294967296"'), '4294967296');
  });

  it('quotes names that would read as something else', () => {
    assert.equal(formatIdentifier('"5"'), "'5'");
    assert.equal(formatIdentifier('"+5"'), "'+5'");
    assert.equal(formatIdentifier('"a b"'), "'a b'");
    assert.equal(formatIdentifier('"a b"'), "'a b'");
    assert.equal(formatIdentifier('"a(b"'), "'a(b'");
    assert.equal(formatIdentifier('"a:b"'), "'a:b'");
    assert.equal(formatIdentifier('"it\'s"'), '"it\'s"');
    assert.equal(formatIdentifier('"say "hi""'), '\'say "hi"\'');
    assert.equal(formatIdentifier('"""'), "'\"'");
  });

  it('writes reference literals for what quoting cannot carry', () => {
    assert.equal(formatIdentifier('""'), '~1{}');
    assert.equal(formatIdentifier('"   "'), '~1{202020}');
    assert.equal(formatIdentifier('"*"'), '~1{2a}');
    assert.equal(formatIdentifier('"$x"'), '~1{2478}');
    assert.equal(formatIdentifier('"#x"'), '~1{2378}');
    assert.equal(formatIdentifier('"~x"'), '~1{7e78}');
    assert.equal(formatIdentifier('"a`b"'), '~1{616062}');
    assert.equal(formatIdentifier('"a\u0001"'), '~1{6101}');
    assert.equal(formatIdentifier('"x\'""'), '~1{782722}');
  });
});

describe('parse', () => {
  it('puts every name in a quote envelope', () => {
    assert.deepEqual(parse('(a: 1 $x) (* "q r")'), [
      node(null, node('"a"', leaf('1'), leaf('$x')), node(null, leaf('*'), leaf('"q r"'))),
    ]);
  });

  it('reads quoted numbers and reference literals as names', () => {
    assert.deepEqual(parse("('1' ~1{6162} \"~1{6162}\" ~1x 'odd)"), [
      node(null, leaf('"1"'), leaf('"ab"'), leaf('"~1{6162}"'), leaf('"~1x"'), leaf('"\'odd"')),
    ]);
  });

  it('returns nothing for an empty query', () => {
    assert.deepEqual(parse(''), []);
  });

  it('reports syntax errors without the notation prefix twice', () => {
    for (const query of ['((', '(a: b', '(a b))']) {
      assert.throws(
        () => parse(query),
        (error) =>
          error instanceof ParseError &&
          error.cause === undefined &&
          !error.message.startsWith('Parse error: Parse error:'),
        query,
      );
    }
  });

  it('reports a malformed reference literal', () => {
    assert.throws(() => parse('(~3{abc} 1)'), ParseError);
  });
});

describe('parseFor with limits', () => {
  it('accepts what fits', () => {
    assert.deepEqual(parseFor('(1 +3)', limits()), [node(null, leaf('1'), leaf('+3'))]);
    assert.deepEqual(parseFor('(1 01)', limits()), [node(null, leaf('1'), leaf('01'))]);
    assert.deepEqual(parseFor('(1 +)', limits()), [node(null, leaf('1'), leaf('"+"'))]);
    assert.deepEqual(parseFor('("11" ~1{3131})', limits()), [
      node(null, leaf('"11"'), leaf('"11"')),
    ]);
    assert.deepEqual(parseFor('', limits()), []);
  });

  it('limits the nesting depth', () => {
    assert.throws(() => parseFor('((((a))))', limits()), ParseError);
    assert.equal(parseFor('((((a))))', limits({ maxDepth: 8 })).length, 1);
  });

  it('limits the node count', () => {
    assert.throws(() => parseFor('(a b c d e f)', limits()), {
      name: 'QueryError',
      message: 'Query error: query node limit exceeded',
    });
    assert.equal(parseFor('(a b c d)', limits()).length, 1);
  });

  it('limits addresses', () => {
    for (const token of ['11', '-1', '+11', '4294967296']) {
      assert.throws(
        () => parseFor(`(1 ${token})`, limits()),
        (error) =>
          error instanceof QueryError &&
          error.message === `Query error: query address out of range: ${token}`,
        token,
      );
    }
    assert.equal(parseFor('(1 4294967295)', limits({ maxAddress: 0xffffffffffffffffn })).length, 1);
  });
});
