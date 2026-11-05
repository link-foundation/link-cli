import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { validateLycheeReport, splitRecoveredUrls } from './check-web-archive.mjs';
import { recheckUnanswered, allFailuresRecovered, parseLycheeFailures } from './recheck-broken-links.mjs';

test('missing or incomplete link evidence never turns a failed checker green', () => {
  for (const content of ['', '# Report', '| Errors | 0 |', '## Errors per input\n* [WAT] https://example.com']) {
    assert.throws(() => validateLycheeReport(content, '1'), /report/);
  }
  const root = mkdtempSync(join(tmpdir(), 'missing-lychee-'));
  try {
    const result = spawnSync(process.execPath, [new URL('./check-web-archive.mjs', import.meta.url).pathname], {
      env: { ...process.env, LYCHEE_OUTPUT: join(root, 'missing.md') }, encoding: 'utf8',
    });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Missing lychee report/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('transient recovery preserves permanent errors and cannot claim partial recovery as complete', async () => {
  const report = '## Errors per input\n* [404] https://example.com/missing\n* [ERROR] https://example.com/reset';
  const failures = parseLycheeFailures(report);
  assert.deepEqual(failures.map((f) => f.retryable), [false, true]);
  let requests = 0;
  const result = await recheckUnanswered(['https://example.com/reset'], {
    accept: '200..=299', userAgent: 'lychee', initialWaitMs: 0, budgetSeconds: 1,
    fetchImpl: async () => { if (++requests === 1) throw Error('ECONNRESET'); return { status: 200 }; },
  });
  assert.equal(requests, 2);
  assert.equal(allFailuresRecovered(1, 1, result), false);
  assert.equal(allFailuresRecovered(0, 1, result), true);
  assert.deepEqual(splitRecoveredUrls(failures.map((f) => f.url), result.recovered.join('\n')).remaining, ['https://example.com/missing']);
});

test('a permanent response terminates retries immediately', async () => {
  let requests = 0;
  const result = await recheckUnanswered(['https://example.com/missing'], {
    accept: '200..=299', userAgent: 'lychee', initialWaitMs: 0, budgetSeconds: 1,
    fetchImpl: async () => { requests++; return { status: 404 }; },
  });
  assert.equal(requests, 1);
  assert.equal(result.stillBroken.length, 1);
});
