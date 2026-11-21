// Reading and writing tables past what the recorded vectors reach: the keys
// of the virtual tables, aggregates over strings, ordering by relations, by
// columns that are not there and by values that do not compare, plans that
// find nothing, and mutations with their arguments left out. Each answer was
// recorded from the Rust clink serving the same requests in this order.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import { ADMIN, GUEST, ask, deepStore, settingsOf } from './helpers/graphql.mjs';

const STEPS = [
  [
    'mutation{insert_links(objects:[{id:1,type_id:1},{id:2,type_id:1,from_id:1,to_id:1,string:{data:{value:"b"}}},{id:3,type_id:1,string:{data:{value:"a"}}},{id:4,type_id:1,number:{data:{value:2}}}]){affected_rows}}',
    ADMIN,
    '{"data":{"insert_links":{"affected_rows":4}}}',
  ],
  [
    '{handlers_by_pk(handler_id:1){handler_id} can_by_pk(rule_id:0){rule_id} selectors_by_pk(item_id:1){item_id}}',
    GUEST,
    '{"data":{"handlers_by_pk":null,"can_by_pk":null,"selectors_by_pk":null}}',
  ],
  [
    '{strings_aggregate{aggregate{sum{value} avg{value} max{value} min{value}}}}',
    GUEST,
    '{"data":{"strings_aggregate":{"aggregate":{"sum":{"value":0},"avg":{"value":-0},"max":{"value":"b"},"min":{"value":"a"}}}}}',
  ],
  [
    '{a:links(order_by:{from:asc}){id} c:links(order_by:{nothing:asc}){id} d:links(distinct_on:[nothing]){id} e:links(where:{type_id:{_eq:99}}){id} g:links(where:{type_id:{_eq:1,_neq:2}}){id} h:links(order_by:{string:{value:asc}}){id} i:links(order_by:{string:{value:desc}}){id} k:links(order_by:{string:{value:asc_nulls_first}}){id} l:links(order_by:{string:{value:desc_nulls_last}}){id} j:links(order_by:{value:asc}){id}}',
    GUEST,
    '{"data":{"a":[{"id":1},{"id":2},{"id":3},{"id":4}],"c":[{"id":1},{"id":2},{"id":3},{"id":4}],"d":[{"id":1}],"e":[],"g":[{"id":1},{"id":2},{"id":3},{"id":4}],"h":[{"id":3},{"id":2},{"id":1},{"id":4}],"i":[{"id":1},{"id":4},{"id":2},{"id":3}],"k":[{"id":1},{"id":4},{"id":3},{"id":2}],"l":[{"id":2},{"id":3},{"id":1},{"id":4}],"j":[{"id":2},{"id":3},{"id":4},{"id":1}]}}',
  ],
  [
    '{links(where:{type_id:1}){id}}',
    GUEST,
    '{"errors":[{"message":"a comparison is an object","extensions":{"path":"$.where.type_id","code":"validation-failed"}}]}',
  ],
  [
    '{links(where:{_and:[{type_id:{_eq:1}},{from_id:{_eq:1}}]}){id} x:links(where:{type:{id:{_eq:1}}}){id}}',
    GUEST,
    '{"data":{"links":[{"id":2}],"x":[{"id":1},{"id":2},{"id":3},{"id":4}]}}',
  ],
  ['mutation{insert_links{affected_rows}}', ADMIN, '{"data":{"insert_links":{"affected_rows":0}}}'],
  [
    'mutation{insert_links(objects:{type_id:1}){returning{id}}}',
    ADMIN,
    '{"data":{"insert_links":{"returning":[{"id":5}]}}}',
  ],
  [
    'mutation{update_links_by_pk(pk_columns:{id:1},_set:{to_id:2}){id to_id}}',
    ADMIN,
    '{"data":{"update_links_by_pk":{"id":1,"to_id":2}}}',
  ],
  ['mutation{delete_links_by_pk{id}}', ADMIN, '{"data":{"delete_links_by_pk":null}}'],
  [
    'mutation{update_numbers(where:{},_inc:{value:"x"}){returning{value}}}',
    ADMIN,
    '{"data":{"update_numbers":{"returning":[{"value":2}]}}}',
  ],
  [
    'mutation{update_strings(where:{value:{_eq:"a"}},_inc:{value:1}){returning{value}}}',
    ADMIN,
    '{"data":{"update_strings":{"returning":[{"value":1}]}}}',
  ],
  [
    '{strings{value} numbers{value}}',
    GUEST,
    '{"data":{"strings":[{"value":"b"},{"value":1}],"numbers":[{"value":2}]}}',
  ],
];

// Over deep-core.json: 15 is Selector, 16 SelectorInclude, 70 SelectorTree,
// 73 SelectorFilter and 183 typesTree; 1010 puts the include 1009 in a tree
// that is not one.
const CORE_STEPS = [
  [
    'mutation{insert_links(objects:[{id:1000,type_id:1},{id:1001,type_id:1000},{id:1002,type_id:15},{id:1003,type_id:16,from_id:1002,to_id:1000},{id:1004,type_id:70,from_id:1003,to_id:183},{id:1005,type_id:15},{id:1006,type_id:16,from_id:1005,to_id:1000},{id:1007,type_id:70,from_id:1006,to_id:183},{id:1008,type_id:73,from_id:1005,to_id:1001},{id:1009,type_id:16,from_id:1002,to_id:1000},{id:1010,type_id:70,from_id:1009,to_id:1001}]){affected_rows}}',
    '{"data":{"insert_links":{"affected_rows":11}}}',
  ],
  [
    '{selectors(order_by:[{selector_id:asc},{item_id:asc}]){item_id selector_id selector_include_id query_id} mp_aggregate(where:{item_id:{_eq:1001}}){aggregate{count}}}',
    '{"data":{"selectors":[{"item_id":1000,"selector_id":1002,"selector_include_id":1003,"query_id":0},{"item_id":1001,"selector_id":1002,"selector_include_id":1003,"query_id":0},{"item_id":1000,"selector_id":1005,"selector_include_id":1006,"query_id":1001},{"item_id":1001,"selector_id":1005,"selector_include_id":1006,"query_id":1001}],"mp_aggregate":{"aggregate":{"count":6}}}}',
  ],
  [
    'mutation{insert_links_one{id type_id from_id to_id}}',
    '{"data":{"insert_links_one":{"id":1011,"type_id":0,"from_id":0,"to_id":0}}}',
  ],
  ['mutation{delete_numbers{affected_rows}}', '{"data":{"delete_numbers":{"affected_rows":0}}}'],
];

describe('select and mutate', () => {
  it('answer each step as the Rust clink did', () => {
    const { store } = deepStore();
    const settings = settingsOf();
    for (const [query, auth, answer] of STEPS) {
      assert.equal(ask(store, settings, query, auth), answer, query);
    }
  });

  it('read selectors down their trees and grants without the core', () => {
    const { store } = deepStore({ core: true });
    const settings = settingsOf();
    for (const [query, answer] of CORE_STEPS) {
      assert.equal(ask(store, settings, query, ADMIN), answer, query);
    }
    const policed = deepStore({
      rule: '(AllowRead (Anything Anything)) (AllowWrite (Doc Anything))',
    });
    assert.equal(
      ask(policed.store, settings, '{can{action_id object_id subject_id}}', ADMIN),
      '{"data":{"can":[{"action_id":0,"object_id":0,"subject_id":0}]}}',
    );
  });
});
