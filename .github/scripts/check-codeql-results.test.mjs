import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';

const cli = new URL('./check-codeql-results.mjs', import.meta.url);
const run = (results = []) => ({
  tool: { driver: { name: 'CodeQL' } },
  invocations: [{ executionSuccessful: true }],
  results,
});
const sarif = (...runs) => ({ version: '2.1.0', runs });

test('SARIF gate rejects missing/invalid reports and checks every file/run/severity', () => {
  const dir = mkdtempSync(join(tmpdir(), 'codeql-results-'));
  const check = (expected) => {
    const result = spawnSync(process.execPath, [cli.pathname, dir], {
      encoding: 'utf8',
      timeout: 5000,
    });
    assert.equal(result.status === 0, expected, result.stdout + result.stderr);
    return result;
  };
  const write = (value, name = 'rust.sarif') =>
    writeFileSync(join(dir, name), typeof value === 'string' ? value : JSON.stringify(value));
  try {
    check(false);
    for (const invalid of [
      '{',
      {},
      sarif(),
      sarif({}),
      sarif({ ...run(), results: null }),
      sarif({ ...run(), invocations: [{ executionSuccessful: false }] }),
      sarif({ ...run(), invocations: [] }),
      sarif({ ...run(), results: [null] }),
    ]) {
      write(invalid);
      check(false);
    }
    write(sarif(run()));
    check(true);
    for (const key of ['toolExecutionNotifications', 'toolConfigurationNotifications']) {
      for (const level of ['error', 'warning', undefined]) {
        write(
          sarif({ ...run(), invocations: [{ executionSuccessful: true, [key]: [{ level }] }] }),
        );
        assert.match(check(false).stderr, /diagnostic/);
      }
    }
    for (const level of ['error', 'warning', 'note', 'none', undefined]) {
      const finding = { ruleId: 'test/rule', message: { text: 'unresolved' }, level };
      write(sarif(run(), run([finding])));
      assert.match(check(false).stderr, /test\/rule/);
      write(sarif(run([{ ...finding, suppressions: [{ kind: 'inSource' }] }])));
      check(false);
      write(sarif(run([{ ...finding, baselineState: 'absent' }])));
      check(true);
    }
    write(sarif(run()));
    mkdirSync(join(dir, 'nested'));
    write(
      sarif(run([{ ruleId: 'test/second', message: { text: 'second file' } }])),
      'nested/actions.sarif',
    );
    check(false);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('SARIF text cannot inject workflow commands and every problem is reported (issue #172)', () => {
  const dir = mkdtempSync(join(tmpdir(), 'codeql-results-'));
  try {
    const injected = 'tainted\n::error title=forged::looks like a second, unrelated failure';
    writeFileSync(
      join(dir, 'js.sarif'),
      JSON.stringify(
        sarif({
          ...run([
            { ruleId: 'js/first', message: { text: injected } },
            { ruleId: 'js/second', message: { text: 'second' } },
          ]),
          invocations: [
            {
              executionSuccessful: true,
              toolExecutionNotifications: [
                { level: 'warning', message: { text: 'extractor warning' } },
                { level: 'note' },
              ],
            },
          ],
        }),
      ),
    );
    const result = spawnSync(process.execPath, [cli.pathname, dir], {
      encoding: 'utf8',
      timeout: 5000,
    });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /4 unresolved CodeQL finding\(s\) or diagnostic\(s\)/);
    assert.match(result.stderr, /extractor warning/);
    assert.match(result.stderr, /missing diagnostic message/);
    assert.match(result.stderr, /js\/second/);
    assert.ok(result.stderr.includes(JSON.stringify(injected)));
    assert.ok(!result.stderr.split('\n').some((line) => line.startsWith('::')));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
