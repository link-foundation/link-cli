import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  extractBrokenLinks,
  splitRecoveredUrls,
  validateLycheeReport,
} from './check-web-archive.mjs';
import {
  recheckUnanswered,
  allFailuresRecovered,
  parseLycheeFailures,
} from './recheck-broken-links.mjs';

test('missing or incomplete link evidence never turns a failed checker green', () => {
  for (const content of [
    '',
    '# Report',
    '| Errors | 0 |',
    '## Errors per input\n* [WAT] https://example.com',
  ]) {
    assert.throws(() => validateLycheeReport(content, '2'), /report/);
  }
  const root = mkdtempSync(join(tmpdir(), 'missing-lychee-'));
  try {
    const result = spawnSync(
      process.execPath,
      [new URL('./check-web-archive.mjs', import.meta.url).pathname],
      {
        env: { ...process.env, LYCHEE_OUTPUT: join(root, 'missing.md') },
        encoding: 'utf8',
      },
    );
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Missing lychee report/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('transient recovery preserves permanent errors and cannot claim partial recovery as complete', async () => {
  const report =
    '## Errors per input\n* [404] https://example.com/missing\n* [ERROR] https://example.com/reset';
  const failures = parseLycheeFailures(report);
  assert.deepEqual(
    failures.map((f) => f.retryable),
    [false, true],
  );
  let requests = 0;
  const result = await recheckUnanswered(['https://example.com/reset'], {
    accept: '200..=299',
    userAgent: 'lychee',
    initialWaitMs: 0,
    budgetSeconds: 1,
    fetchImpl: async () => {
      if (++requests === 1) throw Error('ECONNRESET');
      return { status: 200 };
    },
  });
  assert.equal(requests, 2);
  assert.equal(allFailuresRecovered(1, 1, result), false);
  assert.equal(allFailuresRecovered(0, 1, result), true);
  assert.deepEqual(
    splitRecoveredUrls(
      failures.map((f) => f.url),
      result.recovered.join('\n'),
    ).remaining,
    ['https://example.com/missing'],
  );
});

test('a permanent response terminates retries immediately', async () => {
  let requests = 0;
  const result = await recheckUnanswered(['https://example.com/missing'], {
    accept: '200..=299',
    userAgent: 'lychee',
    initialWaitMs: 0,
    budgetSeconds: 1,
    fetchImpl: async () => {
      requests++;
      return { status: 404 };
    },
  });
  assert.equal(requests, 1);
  assert.equal(result.stillBroken.length, 1);
});

// lychee lists timed-out links under "## Timeouts per input", not under the
// errors (lychee-bin/src/formatters/stats/markdown.rs), and exits 2 for them.
// Reading only the errors section declared a recovered 503 plus an unverified
// timeout "all recovered", and rejected a timeouts-only report as malformed
// (issue #172).
const timeoutReport = (errors) => `# Summary

| Status         | Count |
|----------------|-------|
| 🔍 Total       | 5     |
| ✅ Successful  | 3     |
| ⏳ Timeouts    | 1     |
| 🚫 Errors      | ${errors.length}     |
${errors.length ? `\n## Errors per input\n\n### Errors in README.md\n\n${errors.join('\n')}\n` : ''}
## Timeouts per input

### Timeouts in README.md

* [TIMEOUT] <https://example.com/slow> (at 1:1) | Request timed out

## Redirects per input

### Redirects in README.md

* https://example.com/old --[308]--> https://example.com/new
`;

test('timed-out links are failures that must be rechecked (issue #172)', () => {
  const mixed = timeoutReport([
    '* [503] <https://example.com/busy> (at 2:1) | Rejected status code: 503',
  ]);
  validateLycheeReport(mixed, '2');
  assert.deepEqual(
    parseLycheeFailures(mixed).map(({ url, retryable }) => [url, retryable]),
    [
      ['https://example.com/busy', true],
      ['https://example.com/slow', true],
    ],
  );
  assert.deepEqual(extractBrokenLinks(mixed).urls, [
    'https://example.com/busy',
    'https://example.com/slow',
  ]);

  const onlyTimeouts = timeoutReport([]);
  validateLycheeReport(onlyTimeouts, '2');
  assert.deepEqual(
    parseLycheeFailures(onlyTimeouts).map(({ url }) => url),
    ['https://example.com/slow'],
  );

  const miscounted = mixed.replace('⏳ Timeouts    | 1', '⏳ Timeouts    | 2');
  assert.throws(() => validateLycheeReport(miscounted, '2'), /report/);
  // A crashed or misconfigured run is never a recoverable link failure.
  for (const code of ['1', '3']) {
    assert.throws(() => validateLycheeReport(mixed, code), /not a link-failure result/);
  }
});

test('a lychee record never borrows text from the next line (issue #172)', () => {
  const report = [
    '## Errors per input',
    '### Errors in README.md',
    '* [ERROR',
    '* [404] https://example.com/missing | Rejected status code: 404',
    '* [404]',
    'https://example.com/next-line',
    '',
  ].join('\n');
  assert.deepEqual(
    parseLycheeFailures(report).map(({ marker, url }) => [marker, url]),
    [['404', 'https://example.com/missing']],
  );
  assert.deepEqual(extractBrokenLinks(report).urls, ['https://example.com/missing']);
});
