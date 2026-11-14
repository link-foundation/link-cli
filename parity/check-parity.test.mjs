// Unit tests of the cross-language parity gate (issue #111). They run in the
// JavaScript stage through `npm run test:js`, before any Rust or C# job.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import {
  acknowledge,
  checkCoverage,
  checkFingerprints,
  checkGlobs,
  checkGoldenFixtures,
  checkOneSidedChanges,
  checkReceipt,
  computeFingerprints,
  fingerprint,
  globToRegExp,
  readBaseManifest,
  sha256,
  updateManifest,
  validateManifest,
} from './check-parity.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const sources = {
  'cs/Query.cs': 'class Query {}',
  'cs/Helpers.cs': 'static class Helpers {}',
  'rs/query.rs': 'pub fn query() {}',
  'js/query.js': 'export const query = 1;',
  'tests/cs/QueryTests.cs': '[Fact] void Query() {}',
  'tests/rs/query_tests.rs': '#[test] fn query() {}',
  'tests/js/query.test.mjs': "test('query', () => {});",
};
const files = Object.keys(sources);
const readText = (file) => sources[file];

function sample() {
  const manifest = {
    schema: 1,
    languages: {
      csharp: { roots: ['cs/*.cs'] },
      rust: { roots: ['rs/*.rs'] },
      javascript: { roots: ['js/*.js'] },
    },
    shared: [{ paths: ['cs/Helpers.cs'], reason: 'extension methods used everywhere' }],
    features: [
      {
        id: 'query',
        title: 'Queries',
        csharp: {
          status: 'implemented',
          sources: ['cs/Query.cs'],
          tests: ['tests/cs/QueryTests.cs'],
        },
        rust: {
          status: 'implemented',
          sources: ['rs/query.rs'],
          tests: ['tests/rs/query_tests.rs'],
        },
        javascript: {
          status: 'implemented',
          sources: ['js/query.js'],
          tests: ['tests/js/query.test.mjs'],
        },
      },
    ],
  };
  return updateManifest(manifest, computeFingerprints(manifest, files, readText));
}

const clone = (value) => JSON.parse(JSON.stringify(value));
const files2 = (overrides) => (file) => overrides[file] ?? sources[file];

test('globs follow the repository syntax', () => {
  assert.ok(globToRegExp('rust/src/**/*.rs').test('rust/src/lib.rs'));
  assert.ok(globToRegExp('rust/src/**/*.rs').test('rust/src/engine/lsm/store.rs'));
  assert.ok(!globToRegExp('rust/src/*.rs').test('rust/src/engine/lsm.rs'));
  assert.ok(globToRegExp('Engine/{Rows,Journal}.cs').test('Engine/Journal.cs'));
  assert.ok(!globToRegExp('Engine/{Rows,Journal}.cs').test('Engine/Fork.cs'));
  assert.ok(globToRegExp('a?.js').test('ab.js'));
  assert.ok(!globToRegExp('a.js').test('abjs'));
});

test('fingerprints ignore line endings but not content or paths', () => {
  const lf = fingerprint(['a'], () => 'x\ny');
  assert.equal(
    fingerprint(['a'], () => 'x\r\ny'),
    lf,
  );
  assert.notEqual(
    fingerprint(['a'], () => 'x\nz'),
    lf,
  );
  assert.notEqual(
    fingerprint(['b'], () => 'x\ny'),
    lf,
  );
  assert.match(lf, /^[0-9a-f]{16}$/);
});

test('a complete manifest passes every check', () => {
  const manifest = sample();
  assert.deepEqual(validateManifest(manifest), []);
  assert.deepEqual(checkGlobs(manifest, files), []);
  assert.deepEqual(checkCoverage(manifest, files), []);
  assert.deepEqual(checkFingerprints(manifest, computeFingerprints(manifest, files, readText)), []);
  assert.deepEqual(checkOneSidedChanges(manifest, sample()), []);
});

test('the schema requires status evidence for each language', () => {
  const manifest = sample();
  delete manifest.features[0].rust.tests;
  manifest.features[0].csharp = { status: 'missing' };
  manifest.features[0].javascript = { status: 'delegated', to: 'javascript' };
  const errors = validateManifest(manifest).join('\n');
  assert.match(errors, /query\.rust: implemented cells need test evidence/);
  assert.match(errors, /query\.csharp: missing cells need a tracking "issue"/);
  assert.match(errors, /query\.javascript: "to" must name the implementing language/);
  assert.match(errors, /query\.javascript: delegated cells need "via" globs/);
});

test('a glob that matches nothing fails', () => {
  const manifest = sample();
  manifest.features[0].rust.tests = ['rs/missing_tests.rs'];
  assert.match(
    checkGlobs(manifest, files).join(),
    /glob rs\/missing_tests\.rs matches no tracked file/,
  );
});

test('a new source file outside every feature fails coverage', () => {
  const manifest = sample();
  const errors = checkCoverage(manifest, [...files, 'rs/new_feature.rs']);
  assert.deepEqual(errors, [
    `rs/new_feature.rs: rust source belongs to no feature in parity/features.json`,
  ]);
});

test('changing code without refreshing the manifest fails', () => {
  const manifest = sample();
  const current = computeFingerprints(
    manifest,
    files,
    files2({ 'rs/query.rs': 'pub fn query2() {}' }),
  );
  assert.match(
    checkFingerprints(manifest, current).join(),
    /query\.rust: fingerprint \w+ is stale/,
  );
});

test('one implementation changing a feature without the others fails', () => {
  const base = sample();
  const head = clone(base);
  const read = files2({ 'rs/query.rs': 'pub fn query2() {}' });
  updateManifest(head, computeFingerprints(head, files, read));
  const [error] = checkOneSidedChanges(head, base);
  assert.match(error, /^query: only rust changed; csharp, javascript did not/);
  assert.match(error, /--acknowledge query/);
});

test('changing every implementation together passes', () => {
  const base = sample();
  const head = clone(base);
  const read = files2({
    'cs/Query.cs': 'class Query2 {}',
    'rs/query.rs': 'pub fn query2() {}',
    'js/query.js': 'export const query = 2;',
  });
  updateManifest(head, computeFingerprints(head, files, read));
  assert.deepEqual(checkOneSidedChanges(head, base), []);
});

test('an acknowledgement covers exactly the current one-sided change', () => {
  const base = sample();
  const head = clone(base);
  updateManifest(head, computeFingerprints(head, files, files2({ 'rs/query.rs': 'v2' })));
  acknowledge(head, base, 'query', 'Rust-only performance fix', 'https://example.test/pr/1');
  assert.deepEqual(checkOneSidedChanges(head, base), []);
  // A later Rust change needs a new acknowledgement.
  updateManifest(head, computeFingerprints(head, files, files2({ 'rs/query.rs': 'v3' })));
  assert.equal(checkOneSidedChanges(head, base).length, 1);
  assert.throws(() => acknowledge(head, base, 'query', '', ''), /needs --reason and --ref/);
});

test('missing, delegated and new features do not need the other languages', () => {
  const base = sample();
  base.features[0].javascript = { status: 'missing', issue: 'https://example.test/issues/2' };
  const head = clone(base);
  updateManifest(
    head,
    computeFingerprints(head, files, files2({ 'rs/query.rs': 'v2', 'cs/Query.cs': 'v2' })),
  );
  head.features.push({ id: 'fresh', title: 'Fresh', csharp: {}, rust: {}, javascript: {} });
  assert.deepEqual(checkOneSidedChanges(head, base), []);
  assert.deepEqual(checkOneSidedChanges(head, undefined), []);
});

test('golden fixtures must match their hash and stay referenced by both suites', () => {
  const fixtureFiles = ['docs/v.json', 'cs/T.cs', 'rs/t.rs', 'docs/extra.json'];
  const text = { 'docs/v.json': '{}', 'cs/T.cs': 'read("v.json")', 'rs/t.rs': 'nothing here' };
  const golden = {
    schema: 1,
    suites: ['csharp', 'rust'],
    include: ['docs/*.json'],
    fixtures: [
      {
        path: 'docs/v.json',
        sha256: sha256('{}'),
        consumers: { csharp: ['cs/T.cs'], rust: ['rs/t.rs'] },
      },
    ],
  };
  const errors = checkGoldenFixtures(
    golden,
    fixtureFiles,
    (f) => Buffer.from(text[f] ?? ''),
    (f) => text[f],
  );
  assert.deepEqual(errors, [
    'docs/v.json: consumer rs/t.rs no longer mentions v.json',
    'docs/extra.json: fixture is missing from parity/golden-fixtures.json',
  ]);
  golden.fixtures[0].sha256 = sha256('old');
  golden.fixtures[0].consumers.rust = [];
  const stale = checkGoldenFixtures(
    golden,
    fixtureFiles,
    (f) => Buffer.from(text[f] ?? ''),
    (f) => text[f],
  );
  assert.match(stale.join('\n'), /docs\/v\.json: sha256 \w+ differs/);
  assert.match(stale.join('\n'), /list at least one rust test that reads it/);
});

test('a suite receipt must list every golden fixture with its hash', () => {
  const golden = {
    fixtures: [
      { path: 'a.json', sha256: 'aa' },
      { path: 'b.json', sha256: 'bb' },
    ],
  };
  assert.deepEqual(
    checkReceipt(golden, 'rust', { suite: 'rust', fixtures: { 'a.json': 'aa', 'b.json': 'bb' } }),
    [],
  );
  assert.deepEqual(
    checkReceipt(golden, 'rust', { suite: 'csharp', fixtures: { 'a.json': 'aa' } }),
    ['receipt suite is csharp, expected rust', 'rust did not load b.json (receipt: absent)'],
  );
});

test('the repository manifest is complete and current', () => {
  const output = execFileSync(process.execPath, [resolve(root, 'parity/check-parity.mjs')], {
    cwd: root,
    encoding: 'utf8',
    env: { ...process.env, PARITY_BASE_SHA: '' },
  });
  assert.match(output, /Parity: \d+ features, .* 0 findings/);
});

test('a base commit missing from a shallow clone fails instead of skipping the rule', () => {
  assert.equal(readBaseManifest(root, ''), undefined);
  assert.equal(readBaseManifest(root, '0'.repeat(40)), undefined);
  assert.throws(() => readBaseManifest(root, 'f'.repeat(40)), /is not in this clone/);
});
