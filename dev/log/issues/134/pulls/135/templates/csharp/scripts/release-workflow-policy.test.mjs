import { describe, expect, test } from 'bun:test';
import { readFileSync, statSync } from 'node:fs';

const RELEASE_WORKFLOW = '.github/workflows/release.yml';
const DOCS_WORKFLOW = '.github/workflows/docs.yml';

const EXPECTED_JOB_TIMEOUTS = new Map([
  ['detect-changes', 5],
  ['changeset-check', 10],
  ['lint', 20],
  ['test', 30],
  ['build', 20],
  ['release-preflight', 5],
  ['release', 30],
  ['instant-release', 30],
  ['changeset-pr', 10],
  ['pipeline-status', 5],
]);

function readWorkflow(filePath) {
  return readFileSync(filePath, 'utf-8').replaceAll('\r\n', '\n');
}

// Jobs whose `needs` graph includes detect-changes, which is intentionally
// skipped for workflow_dispatch. Without a status-check function in their `if`,
// GitHub Actions adds an implicit success() over the skipped dependency and
// skips these jobs (and the release they gate). See issue #23.
const JOBS_REQUIRING_STATUS_CHECK = ['lint', 'test', 'build', 'release', 'instant-release'];

const STATUS_CHECK_FUNCTIONS = ['always()', '!cancelled()', 'cancelled()', 'success()', 'failure()'];

function getJobCondition(jobBlock) {
  const lines = jobBlock.split('\n');
  const ifIndex = lines.findIndex((line) => /^    if:/.test(line));
  if (ifIndex === -1) {
    return '';
  }

  const conditionLines = [lines[ifIndex].replace(/^    if:\s*\|?\s*/, '')];
  for (const line of lines.slice(ifIndex + 1)) {
    // Condition continues while indented deeper than the `if:` key.
    if (/^ {6,}\S/.test(line) || line.trim() === '') {
      conditionLines.push(line);
      continue;
    }
    break;
  }

  return conditionLines.join('\n').trim();
}

function getJobBlocks(workflow) {
  const lines = workflow.split('\n');
  const jobsStart = lines.findIndex((line) => line === 'jobs:');
  if (jobsStart === -1) {
    return new Map();
  }

  const blocks = new Map();
  let currentName = '';
  let currentLines = [];

  for (const line of lines.slice(jobsStart + 1)) {
    const match = /^  ([a-zA-Z0-9_-]+):\s*$/.exec(line);
    if (match) {
      if (currentName) {
        blocks.set(currentName, currentLines.join('\n'));
      }
      currentName = match[1];
      currentLines = [line];
      continue;
    }

    if (currentName) {
      currentLines.push(line);
    }
  }

  if (currentName) {
    blocks.set(currentName, currentLines.join('\n'));
  }

  return blocks;
}

function getTopLevelBlock(workflow, blockName) {
  const lines = workflow.split('\n');
  const blockStart = lines.findIndex((line) => line === `${blockName}:`);
  if (blockStart === -1) {
    return '';
  }

  const blockLines = [lines[blockStart]];
  for (const line of lines.slice(blockStart + 1)) {
    if (/^\S/.test(line) && line.trim() !== '') {
      break;
    }
    blockLines.push(line);
  }

  return blockLines.join('\n');
}

function getStepBlock(jobBlock, stepName) {
  const lines = jobBlock.split('\n');
  const stepStart = lines.findIndex((line) => line.trim() === `- name: ${stepName}`);
  if (stepStart === -1) {
    return '';
  }

  const stepLines = [lines[stepStart]];
  for (const line of lines.slice(stepStart + 1)) {
    if (/^ {6}- /.test(line)) {
      break;
    }
    stepLines.push(line);
  }

  return stepLines.join('\n');
}

describe('release workflow policy', () => {
  test('both publishers request OIDC just before push and never silently fall back after login', () => {
    const workflow = Bun.YAML.parse(readWorkflow(RELEASE_WORKFLOW));
    expect(workflow.jobs['release-preflight'].steps.at(-1).env.NUGET_USER).toBe('${{ vars.NUGET_USER }}');
    for (const name of ['release', 'instant-release']) {
      const job = workflow.jobs[name];
      expect(job.permissions['id-token']).toBe('write');
      const loginIndex = job.steps.findIndex((step) => step.id === 'nuget-login');
      const pushIndex = job.steps.findIndex((step) => step.id === 'nuget_publish');
      expect(loginIndex).toBeGreaterThan(-1);
      expect(pushIndex).toBe(loginIndex + 1);
      const login = job.steps[loginIndex];
      expect(login.uses).toBe('NuGet/login@8d196754b4036150537f80ac539e15c2f1028841');
      expect(login.if).toContain("vars.NUGET_USER != ''");
      expect(login.if).toContain("steps.version.outputs.version_committed == 'true'");
      expect(login.with.user).toBe('${{ vars.NUGET_USER }}');
      const publish = job.steps[pushIndex];
      expect(publish.env.NUGET_API_KEY).toBe("${{ vars.NUGET_USER == '' && secrets.NUGET_API_KEY || steps.nuget-login.outputs.NUGET_API_KEY }}");
      expect(publish.run).toContain('scripts/push-nuget-package.mjs');
      expect(publish.run).not.toContain('skipping');
      const release = job.steps.find((step) => step.name === 'Create GitHub Release');
      expect(release.if).toContain("steps.nuget_publish.outputs.published == 'true'");
    }
    for (const [name, job] of Object.entries(workflow.jobs)) {
      if (['release', 'instant-release'].includes(name)) continue;
      expect(job.permissions?.['id-token'], name).toBeUndefined();
    }
  });
  test('does not cancel release runs on main when newer pushes arrive', () => {
    const workflow = readWorkflow(RELEASE_WORKFLOW);

    expect(workflow).toContain(
      'group: ${{ github.workflow }}-${{ github.ref }}'
    );
    expect(workflow).toContain(
      "cancel-in-progress: ${{ github.ref != 'refs/heads/main' }}"
    );
    expect(workflow).not.toContain('cancel-in-progress: true');
  });

  test('sets explicit timeout-minutes on every job', () => {
    const workflow = readWorkflow(RELEASE_WORKFLOW);
    const jobBlocks = getJobBlocks(workflow);

    expect([...jobBlocks.keys()]).toEqual([...EXPECTED_JOB_TIMEOUTS.keys()]);

    for (const [jobName, timeoutMinutes] of EXPECTED_JOB_TIMEOUTS) {
      expect(jobBlocks.get(jobName)).toContain(
        `\n    timeout-minutes: ${timeoutMinutes}`
      );
    }
  });

  test('guards jobs with skipped dependencies using a status-check function', () => {
    const workflow = readWorkflow(RELEASE_WORKFLOW);
    const jobBlocks = getJobBlocks(workflow);

    for (const jobName of JOBS_REQUIRING_STATUS_CHECK) {
      const condition = getJobCondition(jobBlocks.get(jobName));
      expect(condition).not.toBe('');
      expect(
        STATUS_CHECK_FUNCTIONS.some((fn) => condition.includes(fn)),
        `Job "${jobName}" must use a status-check function (e.g. always()) because it depends on a possibly skipped job`
      ).toBe(true);
    }
  });

  test('instant-release keeps explicit success checks on its dependencies', () => {
    const workflow = readWorkflow(RELEASE_WORKFLOW);
    const condition = getJobCondition(getJobBlocks(workflow).get('instant-release'));

    expect(condition).toContain("github.event.inputs.release_mode == 'instant'");
    expect(condition).toContain("needs.lint.result == 'success'");
    expect(condition).toContain("needs.test.result == 'success'");
    expect(condition).toContain("needs.build.result == 'success'");
  });

  test('change-gated jobs use detector output for both pull requests and pushes', () => {
    const workflow = readWorkflow(RELEASE_WORKFLOW);
    const jobBlocks = getJobBlocks(workflow);

    for (const jobName of ['lint', 'test']) {
      const job = jobBlocks.get(jobName);
      const condition = getJobCondition(job);

      expect(job).toContain('needs: [detect-changes]');
      expect(condition).toContain('always()');
      expect(condition).toContain('!cancelled()');
      expect(condition).toContain("github.event_name == 'workflow_dispatch'");
      expect(condition).toContain(
        "needs.detect-changes.outputs.any-code-changed == 'true'"
      );
      expect(condition).not.toContain("github.event_name == 'push'");
      expect(condition).not.toContain('needs.changeset-check.result');
    }
  });

  test('publishes only the detector output consumed by job gates', () => {
    const workflow = readWorkflow(RELEASE_WORKFLOW);
    const detectJob = getJobBlocks(workflow).get('detect-changes');

    expect(detectJob).toContain(
      'any-code-changed: ${{ steps.changes.outputs.any-code-changed }}'
    );
    expect(detectJob).not.toMatch(/^\s+(cs|csproj|sln|props|mjs|docs|workflow)-changed:/m);
  });

  test('uses the current GitHub Action versions required by the template', () => {
    const workflow = readWorkflow(RELEASE_WORKFLOW);
    const docsWorkflow = readWorkflow(DOCS_WORKFLOW);

    expect(workflow).toContain('uses: actions/checkout@v6');
    expect(workflow).toContain('uses: actions/setup-dotnet@v5');
    expect(workflow).toContain('uses: actions/upload-artifact@v7');
    expect(workflow).toContain('uses: codecov/codecov-action@v7');
    expect(workflow).toContain('uses: peter-evans/create-pull-request@v8');
    expect(workflow).not.toContain('uses: actions/checkout@v4');
    expect(workflow).not.toContain('uses: actions/setup-dotnet@v4');
    expect(workflow).not.toContain('uses: actions/upload-artifact@v4');
    expect(workflow).not.toContain('uses: codecov/codecov-action@v4');
    expect(workflow).not.toContain('uses: peter-evans/create-pull-request@v7');
    expect(docsWorkflow).toContain('uses: actions/setup-dotnet@v5');
    expect(docsWorkflow).not.toContain('uses: actions/setup-dotnet@v4');
  });

  test('configures the default Git branch before checkout runs', () => {
    const workflow = readWorkflow(RELEASE_WORKFLOW);
    const workflowEnv = getTopLevelBlock(workflow, 'env');

    expect(workflowEnv).toContain("\n  GIT_CONFIG_COUNT: '1'");
    expect(workflowEnv).toContain('\n  GIT_CONFIG_KEY_0: init.defaultBranch');
    expect(workflowEnv).toContain('\n  GIT_CONFIG_VALUE_0: main');
  });

  test('gates Codecov uploads on an explicit token', () => {
    const workflow = readWorkflow(RELEASE_WORKFLOW);
    const testJob = getJobBlocks(workflow).get('test');
    const uploadStep = getStepBlock(testJob, 'Upload coverage to Codecov');
    const missingTokenStep = getStepBlock(
      testJob,
      'Skip Codecov upload when token is unavailable'
    );

    expect(testJob).toContain('CODECOV_TOKEN: ${{ secrets.CODECOV_TOKEN }}');
    expect(uploadStep).toContain(
      "if: matrix.os == 'ubuntu-24.04' && env.CODECOV_TOKEN != ''"
    );
    expect(uploadStep).toContain('uses: codecov/codecov-action@v7');
    expect(uploadStep).toContain('token: ${{ env.CODECOV_TOKEN }}');
    expect(uploadStep).toContain('fail_ci_if_error: true');
    expect(uploadStep).not.toContain('fail_ci_if_error: false');
    expect(missingTokenStep).toContain(
      "if: matrix.os == 'ubuntu-24.04' && env.CODECOV_TOKEN == ''"
    );
    expect(missingTokenStep).toContain(
      '::notice::CODECOV_TOKEN is not configured; skipping Codecov upload.'
    );
  });

  test('smoke-tests published NuGet packages before creating releases', () => {
    const workflow = readWorkflow(RELEASE_WORKFLOW);
    const jobBlocks = getJobBlocks(workflow);

    for (const jobName of ['release', 'instant-release']) {
      const job = jobBlocks.get(jobName);
      const waitIndex = job.indexOf('- name: Wait for NuGet indexing');
      const smokeIndex = job.indexOf('- name: Smoke-test published NuGet package');
      const releaseIndex = job.indexOf('- name: Create GitHub Release');

      expect(waitIndex).toBeGreaterThan(-1);
      expect(smokeIndex).toBeGreaterThan(waitIndex);
      expect(releaseIndex).toBeGreaterThan(smokeIndex);
      expect(job).toContain('bun run scripts/smoke-test-nuget-package.mjs');
      expect(job).not.toMatch(/\|\s*head\b/);
    }
  });

  test('pipeline-status observes every job of the release workflow', () => {
    const workflow = readWorkflow(RELEASE_WORKFLOW);
    const jobBlocks = getJobBlocks(workflow);
    const jobNames = [...jobBlocks.keys()].filter((name) => name !== 'pipeline-status');

    const gate = jobBlocks.get('pipeline-status');
    expect(gate, 'pipeline-status job should exist').toBeDefined();

    // The observer is the terminal job: without if: always() it inherits the
    // skip of whichever dependency was cancelled and disappears exactly when
    // it is needed.
    expect(gate).toContain('if: always()');

    // Every job must be observed, in the order the jobs appear, or a later
    // addition silently escapes the gate. A cancelled job killed by
    // timeout-minutes is reported as `cancelled` — invisible without this.
    const needsList = /\n    needs:\n((?:      - [a-z-]+\n)+)/.exec(`${gate}\n`);
    expect(needsList, 'pipeline-status must declare its needs as a list').not.toBeNull();
    const observed = needsList[1]
      .split('\n')
      .filter((line) => line.startsWith('      - '))
      .map((line) => line.replace('      - ', ''));
    expect(observed).toEqual(jobNames);

    expect(gate).toContain('NEEDS_JSON: ${{ toJSON(needs) }}');
    expect(gate).toContain("IS_MAIN: ${{ github.ref == 'refs/heads/main' && github.event_name == 'push' }}");
    // The gate proves a cancelled job was not superseded before failing it;
    // both inputs are required by scripts/check-pipeline-status.sh.
    expect(gate).toContain('RUN_SHA: ${{ github.sha }}');
    expect(gate).toContain('BRANCH_REF: ${{ github.ref_name }}');
    expect(gate).toContain('run: bash scripts/check-pipeline-status.sh');
    expect(gate).toContain('persist-credentials: false');
  });

  test('budgets every long step below 70% of its job cap', () => {
    const workflow = readWorkflow(RELEASE_WORKFLOW);
    const jobBlocks = getJobBlocks(workflow);
    const MAX_BUDGET_SHARE_PERCENT = 70;
    let wrappedSteps = 0;

    for (const [jobName, block] of jobBlocks) {
      // The job cap is the timeout-minutes declared at job level, before the
      // `steps:` key; step-level timeouts (uses: steps) come later and are
      // not caps.
      const beforeSteps = block.slice(0, block.indexOf('\n    steps:'));
      const capMatch = /timeout-minutes:\s*(\d+)/.exec(beforeSteps);
      expect(capMatch, `${jobName} declares a job-level timeout-minutes`).not.toBeNull();
      const capSeconds = Number(capMatch[1]) * 60;

      for (const [, budgetText] of block.matchAll(/run-with-budget-warning\.sh (\d+)/g)) {
        wrappedSteps++;
        const budget = Number(budgetText);
        expect(
          budget * 100 <= capSeconds * MAX_BUDGET_SHARE_PERCENT,
          `${jobName}: budget ${budget}s must expire at or before ${MAX_BUDGET_SHARE_PERCENT}% of its ${capSeconds}s cap, or the cap fires first and the budget is decorative`
        ).toBe(true);
      }

      if (jobName === 'test') {
        // The test matrix includes windows-2025, where the default shell is
        // pwsh; every wrapped step there must pin shell: bash or the wrapper
        // never runs.
        const shellCount = (block.match(/^\s+shell: bash$/gm) ?? []).length;
        expect(shellCount, 'each wrapped test step pins shell: bash').toBeGreaterThanOrEqual(3);
      }
    }

    expect(wrappedSteps).toBe(20);

    // The wrapper must exist, be executable, and own the three behaviours the
    // naive `timeout(1)` line lacks: process-group signalling, the 70% warning,
    // and the TERM-then-KILL grace window.
    const wrapper = readFileSync('scripts/run-with-budget-warning.sh', 'utf-8');
    expect(wrapper).toContain('set -m');
    expect(wrapper).toContain('BUDGET_WARN_PERCENT');
    expect(wrapper).toContain('BUDGET_GRACE_SECONDS');
    expect(statSync('scripts/run-with-budget-warning.sh').mode & 0o111).not.toBe(0);
  });

  test('release jobs gate on the preflight verdict', () => {
    const workflow = readWorkflow(RELEASE_WORKFLOW);
    const jobBlocks = getJobBlocks(workflow);

    // The preflight job runs the same probes in release mode on main and
    // dispatch, and in report mode on pull requests, where a fork has no
    // publishing secrets.
    const preflight = jobBlocks.get('release-preflight');
    expect(preflight, 'release-preflight job should exist').toBeDefined();
    expect(preflight).toContain("'release' || 'report'");
    expect(preflight).toContain('NUGET_API_KEY: ${{ secrets.NUGET_API_KEY }}');
    expect(preflight).toContain('GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}');
    expect(preflight).toContain('bun run scripts/preflight-credentials.mjs');

    for (const jobName of ['release', 'instant-release']) {
      const job = jobBlocks.get(jobName);
      expect(job, `${jobName} must need the preflight`).toMatch(
        /needs: \[.*release-preflight.*\]/
      );
      expect(
        job,
        `${jobName} must check the preflight result, not just its completion`
      ).toContain("needs.release-preflight.result == 'success'");
    }

    // The probe is a real script with real tests, not a presence check.
    expect(readFileSync('scripts/preflight-credentials.mjs', 'utf-8')).toMatch(
      /checkGithubPushPermission/
    );
  });
});
