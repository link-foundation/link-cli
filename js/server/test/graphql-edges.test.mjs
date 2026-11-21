// Rare forms of a `where`, of a table and of a guest: the cases of
// rust/tests/graphql_edges_tests.rs, over stores built from small packages.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import { Json } from '../src/graphql-json.js';
import { Position } from '../src/graphql-plan.js';
import { DeepStore } from '../src/graphql-store.js';
import { compare, passes } from '../src/graphql-tables.js';
import { ADMIN, GUEST, ask, deepStore, settingsOf } from './helpers/graphql.mjs';

/** A store holding `rows`, a package as text. */
function storeOf(rows) {
  const { store } = deepStore();
  store.loadPackage(Json.parse(rows));
  return store;
}

describe('values of different kinds', () => {
  it('never order', () => {
    assert.equal(compare(Json.bool(true), Json.bool(false)), 1);
    assert.equal(compare(Json.bool(true), Json.int(1)), null);
    assert.equal(compare(Json.int(2), Json.int(1)), 1);
  });

  it('are not unequal to null', () => {
    assert.ok(!passes(Json.int(1), '_neq', Json.NULL));
    assert.ok(!passes(Json.int(1), '_neq', Json.int(1)));
    assert.ok(passes(Json.int(1), '_neq', Json.int(2)));
  });
});

describe('a store', () => {
  it('joins a guest to the users of a database that has them', () => {
    const row = (id, from, to, name) =>
      `{"id":${id},"type_id":1,"from_id":${from},"to_id":${to},"string":"${name}"}`;
    const store = storeOf(
      `[${[
        row(1, 0, 0, '@deep-foundation/core'),
        row(2, 1, 3, 'Join'),
        row(3, 0, 0, 'the join type'),
        row(4, 1, 5, 'User'),
        row(5, 0, 0, 'the user type'),
        row(6, 0, 0, 'deep'),
        row(7, 6, 8, 'users'),
        row(8, 0, 0, 'the users'),
      ].join(',')}]`,
    );
    const settings = settingsOf();
    assert.equal(
      ask(store, settings, 'query { guest { linkId } }'),
      '{"data":{"guest":{"linkId":9}}}',
    );
    assert.equal(
      ask(
        store,
        settings,
        'query { links(where: { from_id: { _eq: 8 } }) { type_id to_id } }',
        ADMIN,
      ),
      '{"data":{"links":[{"type_id":3,"to_id":9}]}}',
    );
  });

  it('orders rows without a value as Postgres orders nulls', () => {
    const store = storeOf(`[{"id":1,"type_id":1,"from_id":0,"to_id":0},
      {"id":2,"type_id":1,"from_id":0,"to_id":0,"string":"b"},
      {"id":3,"type_id":1,"from_id":0,"to_id":0},
      {"id":4,"type_id":1,"from_id":0,"to_id":0,"string":"a"}]`);
    const settings = settingsOf();
    const ids = (order) =>
      ask(store, settings, `query { links(order_by: ${order}) { id } }`, ADMIN);
    for (const [direction, expected] of [
      ['asc', [4, 2, 1, 3]],
      ['asc_nulls_first', [1, 3, 4, 2]],
      ['desc', [1, 3, 2, 4]],
      ['desc_nulls_last', [2, 4, 1, 3]],
    ]) {
      const rows = expected.map((id) => `{"id":${id}}`).join(',');
      assert.equal(
        ids(`{ string: { value: ${direction} } }`),
        `{"data":{"links":[${rows}]}}`,
        direction,
      );
    }
    // An order that names a relation and nothing in it keeps the rows in place.
    assert.equal(
      ids('{ string: { } }'),
      '{"data":{"links":[{"id":1},{"id":2},{"id":3},{"id":4}]}}',
    );
  });

  it('changes nothing for a mutation whose where cannot match', () => {
    const store = storeOf('[{"id":1,"type_id":1,"from_id":0,"to_id":0}]');
    const settings = settingsOf();
    assert.equal(
      ask(
        store,
        settings,
        'mutation { delete_links(where: { id: { _in: [] } }) { affected_rows } }',
        ADMIN,
      ),
      '{"data":{"delete_links":{"affected_rows":0}}}',
    );
    assert.equal(
      ask(store, settings, 'query { links { id } }', ADMIN),
      '{"data":{"links":[{"id":1}]}}',
    );
  });

  it('reads a row without a type as type zero, and refuses absent value rows', () => {
    const { store } = deepStore();
    const id = store.createLink(0, null, 0n, 0n, 0n);
    const address = store.tripletOf(id);
    const triplet = store.view(0, (view) => view.getTriplet(address));
    const row = store.rowOf(triplet);
    assert.deepEqual(row, { id, typeId: 0n, fromId: 0n, toId: 0n });
    // Type zero has an address of its own, other positions and ids do not.
    assert.equal(store.address(Position.Type, 0n), triplet.typeId);
    assert.equal(store.address(Position.Other, 0n), null);
    assert.throws(() => store.updateValue(0, 999999, Json.NULL), /No value row/);
    assert.throws(() => store.deleteValue(0, 999999), /No value row/);
  });

  it('reads a stored value that is not JSON as null', () => {
    const store = storeOf('[{"id":1,"type_id":1,"from_id":0,"to_id":0,"number":5}]');
    const [row] = store.valueOfOwner('numbers', 1n);
    store.triplets.setName(row, `${row}:five`);
    store.reload();
    assert.equal(
      ask(store, settingsOf(), '{numbers{link_id value}}', ADMIN),
      '{"data":{"numbers":[{"link_id":1,"value":null}]}}',
    );
    assert.equal(ask(store, settingsOf(), '{links{id}}', GUEST), '{"data":{"links":[{"id":1}]}}');
  });

  it('opened again reads its rows back from the links, whatever their names say', () => {
    const { store, links } = deepStore();
    // A row may leave out its columns: they are 0.
    store.loadPackage(
      Json.parse(
        '[{"id":1,"type_id":1,"string":"a"},{"id":2,"type_id":1,"string":"b"},{"id":3,"number":1},{"id":4,"type_id":1},{"id":5,"type_id":1},{"id":6,"type_id":1}]',
      ),
    );
    const { triplets } = store;
    triplets.removeName(store.valueOfOwner('strings', 1n)[0]);
    triplets.setName(store.valueOfOwner('strings', 2n)[0], 'no colon');
    // Links with a part that is not a row of links are not rows of links.
    const outside = triplets.createTripletPoint();
    const type = store.tripletOf(1n);
    triplets.updateTriplet(store.tripletOf(4n), outside, 0, 0);
    triplets.updateTriplet(store.tripletOf(5n), type, outside, 0);
    triplets.updateTriplet(store.tripletOf(6n), type, 0, outside);
    triplets.save();
    const again = DeepStore.open(links, null);
    assert.equal(
      ask(again, settingsOf(), '{strings{link_id value} links{id type_id}}', ADMIN),
      '{"data":{"strings":[{"link_id":1,"value":""},{"link_id":2,"value":""}],"links":[{"id":1,"type_id":1},{"id":2,"type_id":1},{"id":3,"type_id":0}]}}',
    );
    assert.throws(() => again.deleteLink(0, 0n), { message: 'Link 0 is not a triplet.' });
  });
});
