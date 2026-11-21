// Shared golden fixtures (issue #111): `parity/golden-fixtures.json` lists the
// `docs/protocol` files that the C#, Rust and JavaScript suites must all pass.
// This test loads every listed fixture, checks its recorded SHA-256 and parses
// each JSON fixture, as rust/tests/golden_fixtures_tests.rs does. With
// `GOLDEN_FIXTURES_RECEIPT` set, it writes the hashes it loaded so CI can prove
// with `node parity/check-parity.mjs --receipt javascript <file>` that this
// suite did not skip the fixtures. The JavaScript tests that read each fixture
// (in js/client and js/server) are its `consumers.javascript` entries.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

// Fixtures with a shape worth checking beyond being JSON, as in the Rust test.
function checkSpecific(path, bytes) {
  if (path.endsWith('graphql/deep-fresh.json.gz')) {
    assert.deepEqual([...bytes.subarray(0, 2)], [0x1f, 0x8b], 'deep-fresh.json.gz is gzip');
  }
  if (path.endsWith('graphql/deepclient-operations.json')) {
    const operations = JSON.parse(bytes.toString('utf8'));
    assert.ok(Array.isArray(operations), 'a list of recorded operations');
    // Entries are recorded operations, plus one transport descriptor
    // (`protocol`, `websocket`) that has no query.
    const recorded = operations.filter((operation) => 'query' in operation);
    assert.ok(recorded.length > 0, 'deepclient-operations.json has no operations');
    for (const operation of recorded) {
      assert.equal(typeof operation.operationName, 'string', JSON.stringify(operation));
      assert.equal(typeof operation.query, 'string', JSON.stringify(operation));
    }
  }
}

test('the JavaScript suite loads every golden fixture (issue #111)', () => {
  const manifest = JSON.parse(readFileSync(join(root, 'parity/golden-fixtures.json'), 'utf8'));
  assert.ok(manifest.fixtures.length > 0, 'the golden fixture list is empty');
  assert.ok(manifest.suites.includes('javascript'), 'javascript is one of the suites');
  const loaded = {};
  for (const fixture of manifest.fixtures) {
    const bytes = readFileSync(join(root, fixture.path));
    const actual = sha256(bytes);
    assert.equal(
      actual,
      fixture.sha256,
      `${fixture.path} changed; review every suite, then run node parity/check-parity.mjs --update`,
    );
    assert.ok(
      fixture.consumers?.javascript?.length > 0,
      `${fixture.path}: list the JavaScript tests that read it`,
    );
    if (fixture.path.endsWith('.json')) {
      assert.doesNotThrow(() => JSON.parse(bytes.toString('utf8')), `${fixture.path} is JSON`);
    }
    checkSpecific(fixture.path, bytes);
    loaded[fixture.path] = actual;
  }
  const receipt = process.env.GOLDEN_FIXTURES_RECEIPT;
  if (receipt) {
    writeFileSync(
      receipt,
      `${JSON.stringify({ suite: 'javascript', fixtures: loaded }, null, 2)}\n`,
    );
  }
});

test('hashes are lowercase hex', () => {
  assert.equal(
    sha256(Buffer.from('abc')),
    'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad',
  );
});
