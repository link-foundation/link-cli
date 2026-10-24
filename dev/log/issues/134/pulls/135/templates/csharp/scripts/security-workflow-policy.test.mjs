import { describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';

const SECURITY_WORKFLOW = '.github/workflows/security.yml';

function readWorkflow() {
  return readFileSync(SECURITY_WORKFLOW, 'utf-8').replaceAll('\r\n', '\n');
}

describe('security workflow policy', () => {
  test('scans pushes, pull requests, and the weekly schedule', () => {
    const workflow = readWorkflow();

    expect(workflow).toContain('branches: [main]');
    expect(workflow).toContain('pull_request:');
    expect(workflow).toContain("cron: '0 6 * * 1'");
  });

  test('analyzes C# and GitHub Actions with CodeQL', () => {
    const workflow = readWorkflow();

    expect(workflow).toContain('language: [csharp, actions]');
    expect(workflow).toContain('uses: github/codeql-action/init@v4');
    expect(workflow).toContain('uses: github/codeql-action/autobuild@v4');
    expect(workflow).toContain('uses: github/codeql-action/analyze@v4');
    expect(workflow).toContain('security-events: write');
  });

  test('rejects high-severity dependency changes on pull requests', () => {
    const workflow = readWorkflow();

    expect(workflow).toContain("if: github.event_name == 'pull_request'");
    expect(workflow).toContain('uses: actions/dependency-review-action@v5');
    expect(workflow).toContain('fail-on-severity: high');
    expect(workflow).toContain('comment-summary-in-pr: on-failure');
    expect(workflow).toContain('pull-requests: write');
  });

  test('uses current checkout and bounded jobs', () => {
    const workflow = readWorkflow();

    expect(workflow).toContain('uses: actions/checkout@v6');
    expect(workflow).toContain('timeout-minutes: 30');
    expect(workflow).toContain('timeout-minutes: 10');
    expect(workflow).toContain('cancel-in-progress: true');
  });
});
