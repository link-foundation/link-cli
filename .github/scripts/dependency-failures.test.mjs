import assert from 'node:assert/strict';
import test from 'node:test';
import { checkLockfiles, checkExitCode, dependencyFiles } from './check-latest-dependencies.mjs';

test('a failed Cargo lookup is unknown, even when it prints updates', () => {
  for (const failure of [{ status: 101 }, { status: null, error: Error('spawn cargo ENOENT') }]) {
    const result = checkLockfiles(['rust/Cargo.lock', 'rust/wasm/Cargo.lock'], {
      run: () => ({ ...failure, stderr: 'Updating example v1.0.0 -> v1.0.1\nerror: registry unavailable' }),
    });
    assert.equal(result.outdated.length, 0);
    assert.equal(result.failures.length, 2);
    assert.match(result.failures[1], /rust\/wasm\/Cargo.lock.*registry unavailable/s);
    assert.equal(checkExitCode({ ...result, mode: 'report' }), 1);
  }
});

test('successful dry runs report the locked updates', () => {
  const result = checkLockfiles(['rust/Cargo.lock'], {
    run: () => ({ status: 0, stderr: 'Updating example v1.0.0 -> v1.0.1' }),
  });
  assert.deepEqual(result, { failures: [], outdated: [{ source: 'rust/Cargo.lock', name: 'example', version: '1.0.0', latest: '1.0.1' }] });
  assert.equal(checkExitCode(result), 1);
  assert.equal(checkExitCode({ ...result, mode: 'report' }), 0);
  assert.equal(checkExitCode({ failures: [], outdated: [] }), 0);
  assert.throws(() => checkExitCode({ ...result, mode: 'typo' }), /Unknown/);
});

test('archived template manifests are excluded while runnable examples stay audited', () => {
  assert.deepEqual(dependencyFiles(['rust/Cargo.toml', 'dev/log/issues/165/pulls/166/templates/rust/Cargo.toml', 'examples/probe/Cargo.toml']),
    ['rust/Cargo.toml', 'examples/probe/Cargo.toml']);
});
