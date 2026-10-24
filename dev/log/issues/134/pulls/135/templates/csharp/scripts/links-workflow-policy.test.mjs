import { describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';

import { extractLycheeRequestOptions } from './recheck-broken-links.mjs';

const LINKS_WORKFLOW = '.github/workflows/links.yml';
const LYCHEE_IGNORE = '.lycheeignore';

function readWorkflow() {
  return readFileSync(LINKS_WORKFLOW, 'utf-8').replaceAll('\r\n', '\n');
}

describe('broken-link workflow policy', () => {
  test('throttles GitHub requests without accepting rate limits as healthy links', () => {
    const config = Bun.TOML.parse(readFileSync('lychee.toml', 'utf-8'));
    expect(config.hosts['github.com']).toMatchObject({ concurrency: 2, request_interval: '1s' });
    expect(config.accept).toBeUndefined();
    const workflow = Bun.YAML.parse(readWorkflow());
    for (const event of ['push', 'pull_request']) {
      expect(workflow.on[event].paths).toContain('lychee.toml');
      expect(workflow.on[event].paths).toContain('.lycheeignore');
      expect(workflow.on[event].paths).toContain('scripts/recheck-broken-links.mjs');
    }
  });
  test('checks Markdown and HTML changes with least privilege', () => {
    const workflow = readWorkflow();

    expect(workflow).toContain("name: Broken Link Checker");
    expect(workflow).toContain("      - '**.md'");
    expect(workflow).toContain("      - '**.html'");
    expect(workflow).toContain('permissions:\n  contents: read');
    expect(workflow).toContain('uses: lycheeverse/lychee-action@v2');
  });

  test('excludes copied case studies and parser fixtures but not C# template HTML', () => {
    const workflow = readWorkflow();

    expect(workflow).toContain('--exclude-path docs/case-studies');
    expect(workflow).toContain('--exclude-path scripts/fixtures');
    expect(workflow).not.toContain('examples/universal-app/index.html');
  });

  test('ignores npm bot-protection responses', () => {
    const ignoredUrls = readFileSync(LYCHEE_IGNORE, 'utf-8');

    expect(ignoredUrls).toContain('https://www\\.npmjs\\.com');
  });

  test('uses a bounded, cancellation-safe Wayback fallback', () => {
    const workflow = readWorkflow();

    expect(workflow).toContain('timeout-minutes: 10');
    expect(workflow).toContain(
      'group: check-${{ github.workflow }}-${{ github.ref }}-link-checker'
    );
    expect(workflow).toContain('cancel-in-progress: true');
    expect(workflow).toContain('fail: false');
    expect(workflow).toContain('run: node scripts/check-web-archive.mjs');
  });

  test('re-asks the failures no host ever answered before declaring links broken', () => {
    const workflow = readWorkflow();
    const recheckIndex = workflow.indexOf('id: recheck');
    const webarchiveIndex = workflow.indexOf('id: webarchive');

    // The re-check runs whenever lychee reported anything, before the
    // archive lookup consumes its recovered list.
    expect(recheckIndex).toBeGreaterThan(-1);
    expect(webarchiveIndex).toBeGreaterThan(recheckIndex);
    expect(workflow).toContain(
      "if: steps.lychee.outputs.exit_code != 0\n        id: recheck"
    );
    expect(workflow).toContain('run: node scripts/recheck-broken-links.mjs');
    expect(workflow).toContain('LYCHEE_OUTPUT: lychee/out.md');
    expect(workflow).toContain('RECOVERED_OUTPUT: lychee/recovered.txt');
    expect(workflow).toContain('RECOVERED_URLS: lychee/recovered.txt');

    // `!= 'true'`, never `== 'false'`: a skipped or crashed re-check leaves
    // the output empty, and only the != form fails safe. Both consumers --
    // the archive lookup and the fail step -- must gate on it.
    const gates = workflow.match(
      /steps\.recheck\.outputs\.all_recovered != 'true'/g
    );
    expect(gates).toHaveLength(2);
    expect(workflow).toContain('!cancelled() &&');
  });

  test('the re-check judges links by the same rules lychee ran with', () => {
    const workflow = readWorkflow();

    // The workflow sets no --accept/--user-agent flags, so lychee runs on
    // its documented defaults; the re-check derives the same pair from this
    // text instead of hardcoding its own idea of the rules.
    expect(extractLycheeRequestOptions(workflow)).toEqual([
      '100..=103,200..=299',
      'lychee',
    ]);
  });

  test('the re-check budget expires before the job cap can', () => {
    const workflow = readWorkflow();
    const capSeconds = Number(/timeout-minutes: (\d+)/.exec(workflow)[1]) * 60;
    const budgetSeconds = Number(
      /RECHECK_BUDGET_SECONDS', (\d+)/.exec(
        readFileSync('scripts/recheck-broken-links.mjs', 'utf-8')
      )[1]
    );

    expect(capSeconds).toBe(600);
    expect(budgetSeconds * 100).toBeLessThanOrEqual(capSeconds * 70);
  });
});
