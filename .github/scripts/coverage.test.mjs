import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { codecovRequired, summarizeCoverage } from './coverage.mjs';

const reply = (status, body) => new Response(JSON.stringify(body), { status });

test('only inactive or missing Codecov repositories make uploading optional', async () => {
  for (const [status, body, expected] of [
    [200, { activated: true }, true],
    [200, { activated: false }, false],
    [404, { detail: 'Not found' }, false],
  ]) {
    const warnings = [];
    const required = await codecovRequired('link-foundation/link-cli', {
      fetch: async (url) => {
        assert.equal(url, 'https://api.codecov.io/api/v2/github/link-foundation/repos/link-cli/');
        return reply(status, body);
      },
      warn: (message) => warnings.push(message),
    });
    assert.equal(required, expected);
    assert.equal(warnings.length, expected ? 0 : 1);
    if (!expected) assert.match(warnings[0], /::warning.*not activated.*artifact/);
  }
});

test('Codecov lookup failures and malformed responses do not silently disable upload checks', async () => {
  for (const status of [401, 403, 429, 500, 503]) {
    await assert.rejects(codecovRequired('link-foundation/link-cli', {
      fetch: async () => reply(status, {}),
    }), new RegExp(String(status)));
  }
  for (const body of [{}, { activated: 'false' }, null]) {
    await assert.rejects(codecovRequired('link-foundation/link-cli', {
      fetch: async () => reply(200, body),
    }), /activated/);
  }
  await assert.rejects(codecovRequired('link-foundation/link-cli', {
    fetch: async () => { throw new Error('network down'); },
  }), /network down/);
});

const coverage = (lines, covered, branches, branchesCovered) =>
  `<?xml version="1.0"?><coverage lines-valid="${lines}" lines-covered="${covered}"
    branches-valid="${branches}" branches-covered="${branchesCovered}"><packages /></coverage>`;

test('Cobertura totals remain visible in the summary without Codecov', () => {
  const summary = summarizeCoverage([coverage(10, 8, 4, 2), coverage(20, 10, 0, 0)]);
  assert.match(summary, /18 \/ 30 \(60\.00%\)/);
  assert.match(summary, /2 \/ 4 \(50\.00%\)/);
  assert.match(summarizeCoverage([coverage(0, 0, 0, 0)]), /0 \/ 0 \(n\/a\)/);
});

test('missing and invalid coverage reports fail instead of publishing misleading totals', () => {
  for (const reports of [[], ['garbage'], ['<coverage />'], [coverage(2, 3, 0, 0)], [coverage(-1, 0, 0, 0)]]) {
    assert.throws(() => summarizeCoverage(reports), /coverage|Cobertura/);
  }
});

test('summary command finds nested Cobertura files and appends the Actions summary', () => {
  const root = mkdtempSync(join(tmpdir(), 'coverage-136-'));
  try {
    mkdirSync(join(root, 'results', 'run'), { recursive: true });
    writeFileSync(join(root, 'results', 'run', 'coverage.cobertura.xml'), coverage(10, 8, 4, 2));
    writeFileSync(join(root, 'results', 'coverage-test.rs'), 'not a coverage report');
    const summary = join(root, 'summary.md');
    writeFileSync(summary, 'Previous step\n');
    execFileSync(process.execPath, [new URL('./coverage.mjs', import.meta.url).pathname, 'summary', join(root, 'results')], {
      env: { ...process.env, GITHUB_STEP_SUMMARY: summary },
      stdio: 'pipe',
      timeout: 10_000,
    });
    assert.match(readFileSync(summary, 'utf8'), /Previous step\n[\s\S]*8 \/ 10 \(80\.00%\)/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
