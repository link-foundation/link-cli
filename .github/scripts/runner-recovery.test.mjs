import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

test('runner recovery verifies annotations, pagination and retry limits offline', { timeout: 10000 }, () => {
  const result = spawnSync('python3', ['-B', new URL('./unacquired_jobs_test.py', import.meta.url).pathname], { encoding: 'utf8' });
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
});
