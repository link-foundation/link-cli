// Regression tests for the lychee report parser used by the Broken Link
// Checker workflow. Both cases below were live CI defects found in issue #96:
// run 32145481148 escalated 9 "broken" links while lychee itself reported 4
// errors (false positives from the redirects section), and the four real
// errors included two that the script could never verify (false negative).
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  extractErrorsSection,
  extractBrokenLinks,
} from './check-web-archive.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const report = readFileSync(join(here, 'fixtures', 'lychee-report.md'), 'utf-8');

test('the errors section stops at the next top-level heading', () => {
  const section = extractErrorsSection(report);
  assert.ok(section.includes('Errors in README.md'));
  assert.ok(
    !section.includes('Redirects per input'),
    'the redirects section must not leak into the errors section'
  );
});

test('a report without an errors section yields nothing', () => {
  assert.equal(extractErrorsSection('# Report\n\n## Redirects per input\n\n* https://example.com --[301]--> https://example.org\n'), '');
});

test('redirected links are not reported as broken', () => {
  const { urls } = extractBrokenLinks(report);
  for (const redirected of [
    'https://docs.rs/link-cli',
    'https://github.com/linksplatform/Protocols.Lino',
    'https://habr.com/ru/articles/804617',
  ]) {
    assert.ok(
      !urls.some((url) => url.startsWith(redirected)),
      `${redirected} redirects successfully and must not be treated as broken`
    );
  }
});

test('every http error is extracted exactly once', () => {
  const { urls } = extractBrokenLinks(report);
  assert.deepEqual(urls, [
    'https://link-foundation.github.io/link-cli/csharp/',
    'https://link-foundation.github.io/link-cli/rust/link_cli/',
  ]);
});

test('errors that the Wayback Machine cannot answer are still reported', () => {
  const { others } = extractBrokenLinks(report);
  assert.equal(
    others.length,
    2,
    'the missing DocFX file and the unresolvable root-relative link must not be silently dropped'
  );
  assert.ok(others.some((link) => link.endsWith('Foundation.Data.Doublets.Cli.yml')));
});

test('the parsed error count matches the count lychee reports', () => {
  const { urls, others } = extractBrokenLinks(report);
  const reported = Number(/🚫 Errors\s*\|\s*(\d+)/.exec(report)[1]);
  assert.equal(urls.length + others.length, reported);
});
