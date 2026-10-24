import { describe, expect, test } from 'bun:test';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const WORKFLOW_DIR = '.github/workflows';
const RELEASE_WORKFLOW = join(WORKFLOW_DIR, 'release.yml');
const WORKFLOWS_WORKFLOW = join(WORKFLOW_DIR, 'workflows.yml');

// Contexts an attacker can set on a pull request from a fork. Interpolating any
// of them into a `run:` body is a script-injection sink, because the runner
// substitutes the text before the shell parses it. Reading the same value
// through `env:` is inert: the shell expands it after parsing. See issue #49
// and actionlint's "untrusted inputs" check.
const UNTRUSTED_CONTEXTS = [
  /github\.head_ref/,
  /github\.event\.pull_request\.(head\.(ref|label)|title|body)/,
  /github\.event\.(issue|comment|discussion|review)\b[^}]*\.(title|body)/,
  /github\.event\.head_commit\.(message|author)/,
  /github\.event\.commits\[[^\]]*\]\.(message|author)/,
  /github\.event\.workflow_run\.head_branch/,
];

function readWorkflow(filePath) {
  return readFileSync(filePath, 'utf-8').replaceAll('\r\n', '\n');
}

function listWorkflows() {
  return readdirSync(WORKFLOW_DIR)
    .filter((name) => name.endsWith('.yml') || name.endsWith('.yaml'))
    .map((name) => join(WORKFLOW_DIR, name));
}

// Yields every line that belongs to a block-scalar `run:` script, with its
// 1-based line number, so findings can be reported at a reviewable location.
function getRunScriptLines(workflow) {
  const lines = workflow.split('\n');
  const scriptLines = [];
  let runIndent = null;

  lines.forEach((line, index) => {
    if (runIndent !== null) {
      const isBlank = line.trim() === '';
      const indent = line.length - line.trimStart().length;
      if (!isBlank && indent <= runIndent) {
        runIndent = null;
      } else {
        if (!isBlank) {
          scriptLines.push({ number: index + 1, text: line });
        }
        return;
      }
    }

    const runMatch = /^(\s*)-?\s*run:\s*[|>][-+]?\s*$/.exec(line);
    if (runMatch) {
      runIndent = runMatch[1].length;
      return;
    }

    // Single-line `run: ...` form: the script is the value itself.
    const inlineMatch = /^\s*-?\s*run:\s+(?![|>]\s*$)(.+)$/.exec(line);
    if (inlineMatch) {
      scriptLines.push({ number: index + 1, text: inlineMatch[1] });
    }
  });

  return scriptLines;
}

describe('workflow script-injection policy', () => {
  test('no workflow interpolates attacker-controlled context into a run: script', () => {
    const findings = [];

    for (const filePath of listWorkflows()) {
      for (const { number, text } of getRunScriptLines(readWorkflow(filePath))) {
        for (const expression of text.match(/\$\{\{[^}]*\}\}/g) ?? []) {
          if (UNTRUSTED_CONTEXTS.some((pattern) => pattern.test(expression))) {
            findings.push(`${filePath}:${number}: ${expression.trim()}`);
          }
        }
      }
    }

    expect(
      findings,
      `Pass these values through env: instead of interpolating them:\n${findings.join('\n')}`
    ).toEqual([]);
  });

  test('changeset validation reads head_ref and the C# root from env', () => {
    const workflow = readWorkflow(RELEASE_WORKFLOW);

    expect(workflow).toContain('GITHUB_HEAD_REF: ${{ github.head_ref }}');
    expect(workflow).toContain(
      'if [[ "$GITHUB_HEAD_REF" == "changeset-release/"* ]] || [[ "$GITHUB_HEAD_REF" == "changeset-manual-release-"* ]]; then'
    );
    expect(workflow).toContain('bun run "$CSHARP_ROOT/scripts/validate-changeset.mjs"');
  });

  test('quotes $GITHUB_OUTPUT redirections', () => {
    for (const filePath of listWorkflows()) {
      const workflow = readWorkflow(filePath);

      expect(workflow, `${filePath} redirects to an unquoted $GITHUB_OUTPUT`).not.toMatch(
        />>\s*\$GITHUB_OUTPUT/
      );
    }
  });
});

// Returns the body of one top-level job (from `  <name>:` to the next job
// key at the same indent), so per-job assertions do not depend on line
// numbers that every edit would shift.
function getJobBody(workflow, jobName) {
  const lines = workflow.split('\n');
  const start = lines.findIndex((line) => line === `  ${jobName}:`);
  if (start === -1) return null;
  let end = lines.length;
  for (let i = start + 1; i < lines.length; i++) {
    if (/^  [A-Za-z0-9_-]+:\s*$/.test(lines[i])) {
      end = i;
      break;
    }
  }
  return lines.slice(start, end).join('\n');
}

// actions/checkout stores the job token in .git/config as an http.extraheader
// unless told not to. Every checkout must say which it is doing, and only the
// jobs that write to the remote may keep the credential (issue #54).
const CREDENTIAL_KEEPING_JOBS = ['release', 'instant-release', 'changeset-pr'];

describe('workflow credential policy', () => {
  test('every checkout declares its credential persistence explicitly', () => {
    const undeclared = [];
    let checkoutCount = 0;

    for (const filePath of listWorkflows()) {
      const lines = readWorkflow(filePath).split('\n');
      lines.forEach((line, index) => {
        if (!/^(\s*)- uses: actions\/checkout@/.test(line)) return;
        checkoutCount++;
        const stepIndent = line.length - line.trimStart().length;
        let persistence = null;
        for (let i = index + 1; i < lines.length; i++) {
          const next = lines[i];
          if (next.trim() === '') continue;
          if (next.length - next.trimStart().length <= stepIndent) break;
          const match = /^\s*persist-credentials:\s*(true|false)\s*$/.exec(next);
          if (match) persistence = match[1];
        }
        if (persistence === null) undeclared.push(`${filePath}:${index + 1}`);
      });
    }

    expect(
      undeclared,
      `These checkouts do not declare persist-credentials:\n${undeclared.join('\n')}`
    ).toEqual([]);
    expect(checkoutCount).toBe(16);
  });

  test('only the jobs that push keep a credential to push with', () => {
    const release = readWorkflow(RELEASE_WORKFLOW);

    for (const jobName of CREDENTIAL_KEEPING_JOBS) {
      const body = getJobBody(release, jobName);
      expect(body, `${jobName} should exist`).not.toBeNull();
      expect(body, `${jobName} pushes to the remote and needs its credential`).toContain(
        'persist-credentials: true'
      );
    }

    const keeping = [];
    for (const filePath of listWorkflows()) {
      const workflow = readWorkflow(filePath);
      const lines = workflow.split('\n');
      lines.forEach((line, index) => {
        if (/^\s*persist-credentials:\s*true\s*$/.test(line)) {
          keeping.push(`${filePath}:${index + 1}`);
        }
      });
    }

    expect(keeping.length, `expected exactly the three pushing checkouts, got ${keeping.join(', ')}`).toBe(3);
    expect(keeping.every((site) => site.startsWith(RELEASE_WORKFLOW))).toBe(true);

    for (const filePath of listWorkflows()) {
      if (filePath === RELEASE_WORKFLOW) continue;
      expect(readWorkflow(filePath)).not.toContain('persist-credentials: true');
    }
  });
});

describe('workflow linting policy', () => {
  test('lints every workflow with actionlint plus its bundled shellcheck', () => {
    const workflow = readWorkflow(WORKFLOWS_WORKFLOW);

    // The Docker image bundles shellcheck; a native binary without shellcheck on
    // PATH silently skips the shell checks that catch injection sinks. Pinned
    // by digest: a mutable tag of a repository outside this organization is
    // arbitrary code in a job that analyses credentials.
    expect(workflow).toContain('uses: docker://rhysd/actionlint@sha256:');
    expect(workflow).toContain('permissions:\n  contents: read');
    expect(workflow).toContain('timeout-minutes: 10');
    expect(workflow).toContain("      - '.github/workflows/**'");
  });

  test('audits every workflow with zizmor beside actionlint', () => {
    const workflow = readWorkflow(WORKFLOWS_WORKFLOW);

    // The regular pass with the repository's own config; `version` is named
    // because the action's `latest` resolves through its own frozen table, so
    // an unlisted bump would silently keep running the old analyser.
    expect(workflow).toContain('uses: zizmorcore/zizmor-action@v0.6.2');
    expect(workflow).toContain('config: .github/zizmor.yml');
    expect(workflow).toContain('version: 1.29.0');

    // The audits that cover `uses: docker://` image references are
    // Pedantic-persona only; without this second pass the digest pin above
    // could quietly regress to a mutable tag.
    expect(workflow).toContain('--persona pedantic --min-severity high --min-confidence high');
  });
});
