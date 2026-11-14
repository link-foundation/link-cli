// The tables of the Deep schema and how rows compare: the tests of
// rust/src/graphql/tables.rs, and every operator of `comparison_exp` with
// the answers the Rust `passes` and `compare` give.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import { Json } from '../src/graphql-json.js';
import { TABLES, columns, compare, like, passes, regex, relation } from '../src/graphql-tables.js';

const check = (value, operator, argument) =>
  passes(Json.parse(value), operator, Json.parse(argument));

describe('like', () => {
  it('handles wildcards and case', () => {
    assert.ok(like('Hello', 'H%o', false));
    assert.ok(like('Hello', '_ello', false));
    assert.ok(!like('Hello', 'h%', false));
    assert.ok(like('Hello', 'h%', true));
    assert.ok(like('abc', '%c%', false));
    assert.ok(!like('abc', 'a%%b%x', false));
    assert.ok(like('abcbc', 'a%bc', false));
    assert.ok(!like('ab', 'a', false));
  });
});

describe('regex', () => {
  it('matches nothing for what regex_lite cannot build', () => {
    assert.ok(regex('abc', '^a.c$', false));
    assert.ok(regex('ABC', '^a', true));
    assert.ok(!regex('abc', '(?=a)', false));
    assert.ok(!regex('aa', '(a)\\1', false));
    assert.ok(!regex('a', '\\pL', false));
    assert.ok(!regex('a', '(', false));
    assert.ok(regex('a\\1', '\\\\1', false));
  });
});

describe('passes', () => {
  it('lets null pass only _is_null', () => {
    assert.ok(passes(Json.NULL, '_is_null', Json.bool(true)));
    assert.ok(!passes(Json.NULL, '_neq', Json.int(1)));
    assert.ok(!passes(Json.int(1), '_is_null', Json.bool(true)));
    assert.ok(!passes(Json.NULL, '_is_null', Json.bool(false)));
  });

  it('answers every operator as the Rust passes does', () => {
    // Recorded from the Rust `tables::passes` of each case.
    for (const [value, operator, argument, expected] of [
      ['1', '_neq', 'null', false],
      ['1', '_neq', '2', true],
      ['1', '_eq', '1.0', true],
      ['[1]', '_eq', '[1]', true],
      ['true', '_eq', 'true', true],
      ['2', '_gt', '1', true],
      ['true', '_gt', 'false', true],
      ['2', '_gte', '2', true],
      ['2', '_gte', 'null', false],
      ['1', '_lt', '2', true],
      ['2', '_lte', '2', true],
      ['"b"', '_lte', '"a"', false],
      ['1', '_in', '[2,1.0]', true],
      ['1', '_in', '1', true],
      ['1', '_nin', '[2]', true],
      ['"Hello"', '_like', '"H%o"', true],
      ['"x"', '_like', '1', false],
      ['"Hello"', '_nlike', '"h%"', true],
      ['"Hello"', '_ilike', '"h%"', true],
      ['"İ"', '_ilike', '"i̇"', true],
      ['"Hello"', '_nilike', '"h%"', false],
      ['"abc"', '_regex', '"^a.c$"', true],
      ['"ABC"', '_nregex', '"^a"', true],
      ['"ABC"', '_iregex', '"^a"', true],
      ['"ABC"', '_niregex', '"^a"', false],
      ['{"a":{"b":1,"c":2}}', '_contains', '{"a":{"b":1}}', true],
      ['{"a":1}', '_contains', '{"b":1}', false],
      ['[1,[2,3]]', '_contains', '[[3]]', true],
      ['"x"', '_contains', '"x"', true],
      ['[1]', '_contained_in', '[1,2]', true],
      ['{"a":null}', '_has_key', '"a"', true],
      ['{"a":1}', '_has_key', '"b"', false],
      ['1', '_has_key', '1', false],
    ]) {
      assert.equal(check(value, operator, argument), expected, `${value} ${operator} ${argument}`);
    }
  });

  it('refuses an unknown operator', () => {
    assert.throws(() => check('1', '_foo', '1'), {
      message: "field '_foo' not found in type: 'comparison_exp'",
    });
    assert.throws(() => check('1', 'constructor', '1'), {
      message: "field 'constructor' not found in type: 'comparison_exp'",
    });
  });
});

describe('compare', () => {
  it('orders as the Rust compare does', () => {
    const order = (left, right) => compare(Json.parse(left), Json.parse(right));
    assert.equal(order('"a"', '1'), null);
    assert.equal(order('true', 'false'), 1);
    assert.equal(order('false', 'true'), -1);
    assert.equal(order('1', '1.0'), 0);
    assert.equal(order('"é"', '"z"'), 1);
    // Code points, not UTF-16 units.
    assert.equal(order('"￿"', '"😀"'), -1);
    assert.equal(order('[1]', '[1]'), null);
    assert.equal(order('1e400', '1'), 1);
    assert.equal(compare(Json.float(NaN), Json.int(1)), null);
  });
});

describe('the schema', () => {
  it('names its tables, columns and relations', () => {
    assert.equal(TABLES.length, 10);
    assert.deepEqual(columns('healthz'), ['status']);
    assert.deepEqual(columns('toString'), []);
    assert.deepEqual(relation('links', 'from'), {
      local: 'from_id',
      target: 'links',
      remote: 'id',
      many: false,
    });
    assert.equal(relation('strings', 'link').many, false);
    assert.equal(relation('selectors', 'query').many, true);
    assert.equal(relation('links', 'nothing'), null);
    assert.equal(relation('toString', 'x'), null);
  });
});
