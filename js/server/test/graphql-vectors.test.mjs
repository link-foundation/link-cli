// The recorded files of docs/protocol/graphql, run through the executor as
// rust/tests/graphql_vectors.rs, rust/tests/graphql_server.rs and
// rust/tests/golden_fixtures_tests.rs read them: execute-vectors.json,
// edge-vectors.json over deep-core.json, deepclient-operations.json over
// deep-fresh.json.gz, and introspection.json as the schema the server embeds.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';

import { execute } from '../src/graphql-execute.js';
import { Json } from '../src/graphql-json.js';
import { ADMIN, GUEST, deepStore, fixture, fixturePath, settingsOf } from './helpers/graphql.mjs';

/** Runs the steps of a vector file; the names of the steps that failed, with their answers. */
function run(vectors, core) {
  const { store } = deepStore({ core });
  const parsed = Json.parse(vectors);
  store.loadPackage(parsed.get('seed'));
  const settings = settingsOf();
  const failures = [];
  for (const step of parsed.get('steps').asList()) {
    const [answer] = execute(
      store,
      settings,
      step.get('query').asStr(),
      step.get('operation')?.asStr() ?? null,
      step.get('variables') ?? Json.NULL,
      GUEST,
    );
    const code = step.get('error')?.asStr() ?? null;
    const passed =
      code === null
        ? answer.get('data')?.write() === step.get('data').write()
        : answer.write().includes(`"code":"${code}"`);
    if (!passed) failures.push(`${step.get('name').asStr()}\t${answer.write()}`);
  }
  return failures;
}

describe('the shared vectors', () => {
  it('execute-vectors.json passes every step', () => {
    assert.deepEqual(run(fixture('execute-vectors.json'), false), []);
  });

  it('edge-vectors.json passes every step over deep-core.json', () => {
    assert.deepEqual(run(fixture('edge-vectors.json'), true), []);
  });

  it('deep-core.json loads once: its rows are left alone the second time', () => {
    const { store } = deepStore({ core: true });
    const settings = settingsOf();
    const count = '{links_aggregate{aggregate{count}}}';
    const before = execute(store, settings, count, null, Json.NULL, ADMIN)[0].write();
    store.loadPackage(Json.parse(fixture('deep-core.json')));
    assert.equal(execute(store, settings, count, null, Json.NULL, ADMIN)[0].write(), before);
    assert.match(before, /^\{"data":\{"links_aggregate":\{"aggregate":\{"count":\d+\}\}\}\}$/);
  });
});

describe('introspection.json', () => {
  it('is the schema the server embeds, byte for byte', () => {
    const recorded = readFileSync(fixturePath('introspection.json'));
    const embedded = readFileSync(new URL('../src/graphql-introspection.json', import.meta.url));
    assert.ok(recorded.equals(embedded), 'src/graphql-introspection.json differs from docs');
  });
});

describe('deepclient-operations.json over deep-fresh.json.gz', () => {
  it('answers each recorded operation of DeepClient as Deep did', { timeout: 120_000 }, () => {
    const bytes = readFileSync(fixturePath('deep-fresh.json.gz'));
    assert.deepEqual([...bytes.subarray(0, 2)], [0x1f, 0x8b], 'deep-fresh.json.gz is gzip');
    const { store } = deepStore();
    store.loadPackage(Json.parse(gunzipSync(bytes).toString('utf8')));
    const settings = settingsOf();
    // Deep's own id sequence had run further than the rows it exported; the
    // Rust clink gives the new links these ids.
    const fresh = new Map([
      ['"linkId":2748', '"linkId":2733'],
      ['"id":2763', '"id":2735'],
      ['"id":2764', '"id":2736'],
      ['"id":2765', '"id":2737'],
    ]);
    const operations = JSON.parse(fixture('deepclient-operations.json'));
    const recorded = operations.filter((operation) => operation.query !== undefined);
    assert.deepEqual(
      operations.filter((operation) => operation.query === undefined),
      [{ websocket: '/v1/graphql', protocol: 'graphql-ws' }],
    );
    assert.equal(recorded.length, 14);
    for (const operation of recorded) {
      assert.equal(typeof operation.operationName, 'string');
      assert.equal(operation.status, 200);
      const [answer] = execute(
        store,
        settings,
        operation.query,
        operation.operationName,
        Json.parse(JSON.stringify(operation.variables)),
        operation.auth ? ADMIN : GUEST,
      );
      let expected = Json.parse(operation.response).write();
      for (const [deep, clink] of fresh) expected = expected.replace(deep, clink);
      const masked = answer.write().replace(/"token":"[^"]*"/g, '"token":"<jwt>"');
      assert.equal(masked, expected, operation.operationName);
    }
  });
});
