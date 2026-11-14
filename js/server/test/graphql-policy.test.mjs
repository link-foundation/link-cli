// The GraphQL executor under a permission policy: the cases of
// rust/tests/graphql_policy.rs, over Deep's core (deep-core.json) and rules
// written as `clink --auto-create-missing-references` writes them.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import { signToken } from '../src/graphql-crypto.js';
import { Settings, execute } from '../src/graphql-execute.js';
import { Json } from '../src/graphql-json.js';
import { ADMIN, GUEST, ask, deepStore, settingsOf, user } from './helpers/graphql.mjs';

function withStore(rule, body) {
  const { store } = deepStore({ rule, core: true });
  body(store, settingsOf());
}

function guest(store, settings) {
  const [answer] = execute(store, settings, '{guest{linkId}}', null, Json.NULL, GUEST);
  return answer.get('data').get('guest').get('linkId').asI64();
}

describe('a store under a policy', () => {
  it('what a subject created is only its own', () => {
    withStore('(AllowWrite (Anything Doc))', (store, settings) => {
      const first = guest(store, settings);
      const second = guest(store, settings);
      assert.notEqual(first, second);
      assert.equal(
        ask(
          store,
          settings,
          'mutation{insert_links_one(object:{id:9000,type_id:9000,string:{data:{value:"mine"}}}){id}}',
          user(first),
        ),
        '{"data":{"insert_links_one":{"id":9000}}}',
      );
      const links = '{links(where:{id:{_eq:9000}}){id string{value}}}';
      const seen = '{"data":{"links":[{"id":9000,"string":{"value":"mine"}}]}}';
      const unseen = '{"data":{"links":[]}}';
      assert.equal(ask(store, settings, links, user(first)), seen);
      assert.equal(ask(store, settings, links, user(second)), unseen);
      assert.equal(ask(store, settings, links, GUEST), unseen);
      assert.equal(ask(store, settings, links, ADMIN), seen);

      // A value is seen with its owner.
      const values = '{strings(where:{value:{_eq:"mine"}}){value}}';
      assert.equal(
        ask(store, settings, values, user(first)),
        '{"data":{"strings":[{"value":"mine"}]}}',
      );
      assert.equal(ask(store, settings, values, user(second)), '{"data":{"strings":[]}}');
    });
  });

  it('writing what is not yours is forbidden', () => {
    withStore('(AllowRead (Anything Anything))', (store, settings) => {
      ask(
        store,
        settings,
        'mutation{insert_links_one(object:{id:9000,type_id:1,number:{data:{value:1}}}){id}}',
        ADMIN,
      );
      const visitor = guest(store, settings);
      for (const mutation of [
        'mutation{insert_links_one(object:{type_id:9000}){id}}',
        'mutation{update_links_by_pk(id:9000,_set:{to_id:1}){id}}',
        'mutation{delete_links_by_pk(id:9000){id}}',
        'mutation{update_numbers(where:{link_id:{_eq:9000}},_set:{value:2}){affected_rows}}',
        'mutation{delete_numbers(where:{link_id:{_eq:9000}}){affected_rows}}',
        'mutation{insert_strings_one(object:{link_id:9000,value:"x"}){id}}',
      ]) {
        const answer = ask(store, settings, mutation, user(visitor));
        assert.ok(answer.includes('"code":"permission-error"'), `${mutation}: ${answer}`);
      }
      // Nothing changed, and nothing was left half done.
      assert.equal(
        ask(store, settings, '{links(where:{id:{_eq:9000}}){to_id number{value}}}', user(visitor)),
        '{"data":{"links":[{"to_id":0,"number":{"value":1}}]}}',
      );
    });
  });

  it('a token for another link needs permission', () => {
    withStore('(AllowRead (Anything Anything))', (store, settings) => {
      const first = guest(store, settings);
      const second = guest(store, settings);
      const of = (id) => `{jwt(input:{linkId:${id}}){linkId error}}`;
      const forbidden = '{"data":{"jwt":{"linkId":null,"error":"Forbidden."}}}';
      assert.equal(ask(store, settings, of(second), user(first)), forbidden);
      assert.equal(ask(store, settings, of(second), GUEST), forbidden);
      const own = `{"data":{"jwt":{"linkId":${first},"error":null}}}`;
      assert.equal(ask(store, settings, of(first), user(first)), own);
      assert.equal(ask(store, settings, of(first), ADMIN), own);
      // No link names the asker.
      assert.equal(ask(store, settings, '{jwt{linkId error}}', user(first)), own);
    });
  });

  it('can lists the grants in Deep terms', () => {
    withStore('(AllowRead (Anything Anything))', (store, settings) => {
      // AllowSelect of the loaded core, and 0 for `Anything` on both sides.
      assert.equal(
        ask(store, settings, '{can{rule_id action_id object_id subject_id action{id}}}', ADMIN),
        '{"data":{"can":[{"rule_id":0,"action_id":28,"object_id":0,"subject_id":0,"action":{"id":28}}]}}',
      );
      assert.equal(
        ask(
          store,
          settings,
          '{links(where:{id:{_eq:28}}){can_action{subject_id} can_object{rule_id} can_rule{rule_id} can_subject{rule_id}}}',
          ADMIN,
        ),
        '{"data":{"links":[{"can_action":[{"subject_id":0}],"can_object":[],"can_rule":[],"can_subject":[]}]}}',
      );
    });
  });

  it('a write grant is AllowUpdate, and a rule on a link without a Deep id is not shown', () => {
    withStore('(AllowWrite (Anything Anything)) (AllowRead (Anything Doc))', (store, settings) => {
      assert.equal(
        ask(store, settings, '{can{action_id object_id subject_id}}', ADMIN),
        '{"data":{"can":[{"action_id":26,"object_id":0,"subject_id":0}]}}',
      );
    });
  });
});

describe('authorisation', () => {
  it('reads the secret and the token', () => {
    const settings = new Settings(Buffer.from('secret'), 'root');
    const open = new Settings(Buffer.from('secret'), null);
    const token = signToken(Buffer.from('secret'), 7n, 0n);
    const bearer = `Bearer ${token}`;
    assert.deepEqual(settings.authorize(null, null), GUEST);
    assert.deepEqual(settings.authorize('  ', null), GUEST);
    assert.deepEqual(settings.authorize('Bearer ', null), GUEST);
    assert.deepEqual(settings.authorize(bearer, null), user(7));
    assert.deepEqual(settings.authorize(token, null), user(7));
    assert.deepEqual(settings.authorize(bearer, 'root'), { user: 7n, admin: true });
    assert.throws(() => settings.authorize(null, 'wrong'), {
      message: 'invalid x-hasura-admin-secret',
    });
    assert.throws(() => open.authorize(null, 'root'), { message: 'invalid x-hasura-admin-secret' });
    assert.throws(() => settings.authorize('a.b.c', null), {
      message: 'Could not verify JWT: Invalid JSON at byte 0.',
    });
    assert.throws(() => settings.authorize('Bearer a.b', null), {
      message: 'Could not verify JWT: Could not verify JWT: malformed token.',
    });
    const [a, b] = [Settings.randomSecret(), Settings.randomSecret()];
    assert.equal(a.length, 32);
    assert.notDeepEqual(a, b);
  });
});
