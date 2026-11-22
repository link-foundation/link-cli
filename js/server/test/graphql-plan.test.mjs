// Hasura's `links_bool_exp` as substitution queries: the tests of
// rust/src/graphql/plan.rs, with the vectors both languages run,
// docs/protocol/graphql/where-vectors.json.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import { Json } from '../src/graphql-json.js';
import { Position, compile, idList, render } from '../src/graphql-plan.js';
import { fixture } from './helpers/graphql.mjs';

/** Deep id `n` is at address `n + 100`; type `0` at 99; id 404 is nowhere. */
const map = (position, id) => (id === 404n ? null : id === 0n ? 99 : Number(id) + 100);

const plan = (text) => compile(Json.parse(text), map);

describe('compile', () => {
  it('passes where-vectors.json', () => {
    for (const vector of Json.parse(fixture('where-vectors.json')).asList()) {
      const where = vector.get('where');
      const found = compile(where, map);
      assert.equal(render(found.plan), vector.get('plan').asStr(), where.write());
      const exact = vector.get('exact');
      assert.equal(found.exact, exact?.kind === 'bool' ? exact.value : undefined, where.write());
    }
  });

  it('joins variables for equalities and relations', () => {
    assert.deepEqual(plan('{"type_id":{"_eq":5}}').plan, {
      kind: 'match',
      text: '(($i: 105 * *))',
    });
    const joined = plan('{"from":{"type_id":{"_eq":5}}}');
    assert.ok(joined.exact);
    assert.deepEqual(joined.plan, { kind: 'match', text: '(($v1: 105 * *) ($i: * $v1 *))' });
  });

  it('leaves to the executor what a restriction cannot say', () => {
    // Recorded from the Rust `compile` and `render` with the same mapping.
    const cases = [
      ['null', 'all', true],
      ['{}', 'all', true],
      ['{"value":{"_gt":1}}', 'all', false],
      ['{"from":{"value":{"_gt":1}}}', 'all', false],
      ['{"from":{}}', 'all', false],
      ['{"from":3}', 'all', false],
      ['{"in":{"string":{}}}', 'all', false],
      ['{"_and":[1]}', '(intersect all)', true],
      [
        '{"_and":[{"type_id":{"_eq":1}},{"id":{"_gt":1}}],"to_id":{"_eq":2}}',
        '(intersect (($i: * * 102)) (intersect (($i: 101 * *)) all))',
        false,
      ],
      ['{"type_id":{"_eq":404}}', 'empty', true],
      ['{"type_id":{"_eq":1},"_and":{"type_id":{"_eq":2}}}', 'empty', true],
      ['{"type_id":{"_in":[1,"x"]}}', 'all', false],
      ['{"type_id":{"_gt":1}}', 'all', false],
      ['{"id":{"_eq":"x"}}', 'all', false],
      ['{"id":{"_in":3}}', 'all', false],
      ['{"_or":[]}', 'all', false],
      ['{"_not":{"value":{"_gt":1}}}', 'all', false],
      ['{"_not":{"id":{"_eq":1}}}', '(minus (ids 1))', true],
      ['{"from_id":{"_eq":0}}', '(($i: * 0 *))', true],
      ['{"from":{"from_id":{"_eq":1}},"typed":{"type_id":{"_eq":2}}}', 'empty', true],
      ['{"from_id":{"_eq":1},"from":{"type_id":{"_eq":2}}}', 'empty', true],
      [
        '{"in":{"type_id":{"_eq":3}},"id":{"_in":[1,2]},"_or":[{"to_id":{"_eq":1}},{"value":{}}]}',
        '(intersect (($v1: 103 * $i) ($i: * * *)) (ids 1 2) (union (($i: * * 101)) all))',
        false,
      ],
      ['{"from":{"to":{"type_id":{"_eq":3}}},"to_id":{"_eq":404}}', 'empty', true],
    ];
    for (const [where, rendered, exact] of cases) {
      const found = plan(where);
      assert.deepEqual([render(found.plan), found.exact], [rendered, exact], where);
    }
  });

  it('reads id lists', () => {
    assert.deepEqual(idList(Json.parse('{"_eq":1.0}')), [1n]);
    assert.deepEqual(idList(Json.parse('{"_in":[1,2]}')), [1n, 2n]);
    assert.equal(idList(Json.parse('{"_in":[1,2.5]}')), null);
    assert.equal(idList(Json.parse('{"_gt":1}')), null);
    assert.equal(idList(Json.parse('{"_eq":1,"_in":[]}')), null);
    assert.equal(idList(Json.parse('[1]')), null);
  });

  it('asks the mapping where a type and another id are', () => {
    const asked = [];
    compile(Json.parse('{"type_id":{"_eq":0},"to_id":{"_eq":0},"from_id":{"_eq":1}}'), (p, id) => {
      asked.push([p, id]);
      return 7;
    });
    assert.deepEqual(asked, [
      [Position.Type, 0n],
      [Position.Other, 1n],
    ]);
  });
});

describe('render', () => {
  it('writes every kind of plan', () => {
    assert.equal(
      render({ kind: 'intersect', items: [{ kind: 'all' }, { kind: 'empty' }] }),
      '(intersect all empty)',
    );
    assert.equal(
      render({
        kind: 'union',
        items: [
          { kind: 'ids', ids: [1n] },
          { kind: 'match', text: '(x)' },
        ],
      }),
      '(union (ids 1) (x))',
    );
    assert.equal(render({ kind: 'minus', item: { kind: 'all' } }), '(minus all)');
  });
});
