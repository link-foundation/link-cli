// Tests of the `LinoLink` helpers of src/parser.js, the port of
// rust/src/lino_link.rs and rust/src/parser.rs (cases of
// rust/tests/lino_link_tests.rs and rust/tests/parser_tests.rs).
import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { ParseError } from '../src/errors.js';
import {
  hasValues,
  isEmptyLino,
  isVariable,
  isWildcard,
  lino,
  parseLino,
  valuesCount,
} from '../src/parser.js';

describe('LinoLink helpers', () => {
  it('isVariable holds for an id that starts with $', () => {
    assert.equal(isVariable(lino('$x')), true);
    assert.equal(isVariable(lino('$')), true);
    assert.equal(isVariable(lino('x')), false);
    assert.equal(isVariable(lino('a$')), false);
    assert.equal(isVariable(lino(null)), false);
  });

  it('isWildcard holds for the id * only', () => {
    assert.equal(isWildcard(lino('*')), true);
    assert.equal(isWildcard(lino('**')), false);
    assert.equal(isWildcard(lino('$*')), false);
    assert.equal(isWildcard(lino(null)), false);
  });

  it('tells an empty link from one with values', () => {
    assert.equal(isEmptyLino(lino(null)), true);
    assert.equal(isEmptyLino(lino(null, [])), true);
    assert.equal(isEmptyLino(lino('a')), false);
    const parent = lino(null, [lino('a'), lino('b')]);
    assert.equal(hasValues(parent), true);
    assert.equal(valuesCount(parent), 2);
    assert.equal(valuesCount(lino('a')), 0);
  });
});

describe('parseLino', () => {
  it('marks variables and wildcards in a parsed query', () => {
    const [link] = parseLino('($i: $s *)');
    assert.equal(link.id, '$i');
    assert.ok(isVariable(link));
    assert.deepEqual(
      link.values.map((value) => [value.id, isVariable(value), isWildcard(value)]),
      [
        ['$s', true, false],
        ['*', false, true],
      ],
    );
  });

  it('reports a syntax error as a ParseError', () => {
    assert.throws(() => parseLino('(a b'), ParseError);
  });
});
