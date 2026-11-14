// Temporary: demonstrates the stage gate of issue #111. This test fails on
// purpose, so `npm run test:js` fails and no Rust or C# job may start. The
// next commit removes it; the run IDs are in
// docs/case-studies/issue-111/data/ci-evidence.md.
import assert from 'node:assert/strict';
import test from 'node:test';

test('issue #111 gate demonstration: a failing npm run test:js', () => {
  assert.fail('deliberate failure: the Rust and C# jobs must not start');
});
