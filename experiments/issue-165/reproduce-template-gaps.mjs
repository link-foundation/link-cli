// Offline reproduction against the immutable template snapshots. No registry calls.
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { parseLycheeFailures as fixedParser } from '../../.github/scripts/recheck-broken-links.mjs';
import { checkCodeqlResults as fixedGate } from '../../.github/scripts/check-codeql-results.mjs';
const root = resolve('dev/log/issues/165/pulls/166/templates');
const report =
  '## Errors per input\n* [404] https://example.com/missing\n* [ERROR] https://example.com/reset';
for (const template of ['rust', 'js', 'csharp']) {
  const { parseLycheeFailures } = await import(
    pathToFileURL(join(root, template, 'scripts/recheck-broken-links.mjs'))
  );
  const failures = parseLycheeFailures(report);
  assert.equal(failures.length, 1);
  console.log(
    `${template}: expected 2 separate failures, got ${failures.length}: ${JSON.stringify(failures)}`,
  );
}
assert.equal(fixedParser(report).length, 2);
console.log('link-cli fixed parser preserves both failures');
const dir = mkdtempSync(join(tmpdir(), 'template-sarif-'));
try {
  writeFileSync(
    join(dir, 'example.sarif'),
    JSON.stringify({
      version: '2.1.0',
      runs: [
        {
          tool: { driver: { name: 'CodeQL' } },
          results: [],
          invocations: [
            {
              executionSuccessful: true,
              toolExecutionNotifications: [
                { level: 'error', message: { text: 'Extraction failed' } },
              ],
            },
          ],
        },
      ],
    }),
  );
  for (const [template, script] of [
    ['rust', 'check-codeql-results.mjs'],
    ['js', 'check-codeql-results.mjs'],
    ['python', 'check_codeql_results.py'],
  ]) {
    const result = spawnSync(
      template === 'python' ? 'python3' : process.execPath,
      [join(root, template, 'scripts', script), dir],
      { encoding: 'utf8' },
    );
    assert.equal(result.status, 0, result.stderr);
    console.log(`${template}: incomplete extraction incorrectly passes (exit ${result.status})`);
  }
  assert.throws(() => fixedGate(dir), /Extraction failed/);
  console.log('link-cli fixed SARIF gate rejects incomplete extraction');
} finally {
  rmSync(dir, { recursive: true, force: true });
}
