// The executor's own edges, past what the recorded vectors reach: a store that
// fails to commit or to roll back (`GqlError::write` of execute.rs), the
// directives `@skip` and `@include` with conditions that are not booleans, the
// recorded schema's fields and `guest` over a store without Deep's core.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import { Json } from '../src/graphql-json.js';
import { ValueKind } from '../src/graphql-store.js';
import { ADMIN, GUEST, ask, deepStore, settingsOf } from './helpers/graphql.mjs';

const unexpected = (message) =>
  `{"errors":[{"message":"${message}","extensions":{"path":"$","code":"unexpected"}}]}`;

const fail = (message) => () => {
  throw new Error(message);
};

describe('a store that fails', () => {
  it('to commit a mutation answers with its error', () => {
    const { store } = deepStore();
    store.commit = fail('disk full');
    assert.equal(
      ask(store, settingsOf(), 'mutation{insert_links_one(object:{type_id:1}){id}}', ADMIN),
      unexpected('disk full'),
    );
  });

  it('to roll a mutation back answers with that error', () => {
    const { store } = deepStore();
    store.rollback = fail('undo lost');
    assert.equal(ask(store, settingsOf(), 'mutation{nothing}', ADMIN), unexpected('undo lost'));
    // A query neither commits nor rolls back.
    assert.equal(ask(store, settingsOf(), '{nothing}', ADMIN).includes('validation-failed'), true);
  });

  it('while it makes a guest answers with the first error', () => {
    const { store } = deepStore();
    const settings = settingsOf();
    store.createLink = fail('violation of a key');
    assert.equal(
      ask(store, settings, '{guest{linkId}}'),
      '{"errors":[{"message":"violation of a key","extensions":{"path":"$","code":"constraint-violation"}}]}',
    );
    store.rollback = fail('undo lost');
    assert.equal(ask(store, settings, '{guest{linkId}}'), unexpected('undo lost'));
    delete store.createLink;
    delete store.rollback;
    store.commit = fail('disk full');
    assert.equal(ask(store, settings, '{guest{linkId}}'), unexpected('disk full'));
  });
});

describe('directives', () => {
  it('apply only a boolean condition of skip and include', () => {
    const { store } = deepStore();
    const settings = settingsOf();
    assert.equal(
      ask(
        store,
        settings,
        '{a:links @skip(if:1){id} b:links @foo(if:true){id} c:links @include(if:false){id} d:links @skip{id} e:links @skip(if:$s){id}}',
        ADMIN,
      ),
      '{"data":{"a":[],"b":[],"d":[],"e":[]}}',
    );
  });
});

describe('the recorded schema', () => {
  it('keeps the type of a field it does not list', () => {
    const { store } = deepStore();
    assert.equal(
      ask(store, settingsOf(), '{__schema{x:queryType{name} nothing{name} __typename}}'),
      '{"data":{"__schema":{"x":{"name":"query_root"},"nothing":null,"__typename":"__Schema"}}}',
    );
  });
});

describe('guest', () => {
  it('without a User type is a link of its own type', () => {
    const { store } = deepStore();
    const settings = settingsOf();
    assert.equal(ask(store, settings, '{guest{linkId}}'), '{"data":{"guest":{"linkId":1}}}');
    assert.equal(
      ask(store, settings, '{links{id type_id from_id to_id}}', ADMIN),
      '{"data":{"links":[{"id":1,"type_id":1,"from_id":0,"to_id":0}]}}',
    );
  });

  it('skips a name whose link is gone or is not a row of links', () => {
    const { store } = deepStore({ core: true });
    const settings = settingsOf();
    const [values, tripletOf] = [store.values.bind(store), store.tripletOf.bind(store)];
    // Names of `User` the store does not hold together: one without a link,
    // one whose link is not a triplet.
    const named = (owner) => [-1, { kind: ValueKind.String, owner, value: Json.str('User') }];
    store.values = () => [named(-1n), named(-2n), ...values()];
    store.tripletOf = (id) => (id === -2n ? 2 ** 40 : tripletOf(id));
    const answer = ask(store, settings, '{guest{linkId}}', GUEST);
    store.values = values;
    store.tripletOf = tripletOf;
    const id = answer.match(/^\{"data":\{"guest":\{"linkId":(\d+)\}\}\}$/)[1];
    // Still of the core's `User`, as the next guest is.
    const typeOf = (link) =>
      JSON.parse(ask(store, settings, `{links(where:{id:{_eq:${link}}}){type_id}}`, ADMIN)).data
        .links[0].type_id;
    const next = JSON.parse(ask(store, settings, '{guest{linkId}}')).data.guest.linkId;
    assert.equal(typeOf(id), typeOf(next));
    assert.notEqual(typeOf(id), Number(id));
  });
});
