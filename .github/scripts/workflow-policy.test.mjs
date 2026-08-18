// Regression tests for the CI/CD invariants restored in issue #96.
//
// These run without any third-party dependency (the lint jobs call them with a
// bare `node --test`), so the workflows are inspected with a small line-based
// scanner instead of a YAML parser. Every rule below encodes a defect that was
// actually present in this repository at some point.

import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const workflowsDir = join(repoRoot, '.github', 'workflows');

const workflows = readdirSync(workflowsDir)
  .filter((name) => name.endsWith('.yml') || name.endsWith('.yaml'))
  .map((name) => ({
    name,
    path: join(workflowsDir, name),
    text: readFileSync(join(workflowsDir, name), 'utf8'),
  }));

test('there is at least one workflow to inspect', () => {
  assert.ok(workflows.length > 0, `no workflows found in ${workflowsDir}`);
});

/** Splits a workflow into `{ name, body }` entries, one per job. */
function readJobs(text) {
  const lines = text.split('\n');
  const jobsStart = lines.findIndex((line) => line === 'jobs:');
  if (jobsStart === -1) {
    return [];
  }
  const jobs = [];
  let current = null;
  for (const line of lines.slice(jobsStart + 1)) {
    if (/^[A-Za-z]/.test(line)) {
      break; // back to a top-level key, the jobs mapping ended
    }
    const header = /^ {2}([A-Za-z0-9_-]+):\s*$/.exec(line);
    if (header) {
      current = { name: header[1], body: [] };
      jobs.push(current);
      continue;
    }
    if (current) {
      current.body.push(line);
    }
  }
  return jobs.map((job) => ({ name: job.name, body: job.body.join('\n') }));
}

/** Reads the `options:` of a `workflow_dispatch` choice input. */
function readChoiceOptions(text, inputName) {
  const lines = text.split('\n');
  const start = lines.findIndex((line) => line.trim() === `${inputName}:`);
  if (start === -1) {
    return [];
  }
  const indent = lines[start].length - lines[start].trimStart().length;
  const options = [];
  let inOptions = false;
  for (const line of lines.slice(start + 1)) {
    const trimmed = line.trim();
    if (trimmed === '') continue;
    const lineIndent = line.length - line.trimStart().length;
    if (lineIndent <= indent) break; // left the input definition
    if (trimmed === 'options:') {
      inOptions = true;
      continue;
    }
    if (inOptions) {
      const item = /^-\s+(.+?)\s*$/.exec(trimmed);
      if (!item) break;
      options.push(item[1].replace(/^['"]|['"]$/g, ''));
    }
  }
  return options;
}

// A workflow that lists PR branches under `push:` while also reacting to
// `pull_request:` runs twice for every push to such a branch, doubling CI cost
// and producing duplicate status checks (issue #96).
test('no workflow reacts to both pull_request and non-main push branches', () => {
  for (const workflow of workflows) {
    if (!/^ {2}pull_request:$/m.test(workflow.text)) continue;
    const lines = workflow.text.split('\n');
    const pushIndex = lines.findIndex((line) => line === '  push:');
    if (pushIndex === -1) continue;
    const branches = [];
    let inBranches = false;
    for (const line of lines.slice(pushIndex + 1)) {
      if (line.trim() === '') continue;
      const indent = line.length - line.trimStart().length;
      if (indent <= 2) break; // left the push trigger
      if (indent === 4) {
        inBranches = line.trim() === 'branches:';
        continue;
      }
      if (inBranches) {
        const item = /^-\s+(.+?)\s*$/.exec(line.trim());
        if (item) branches.push(item[1].replace(/^['"]|['"]$/g, ''));
      }
    }
    const extraneous = branches.filter((branch) => branch !== 'main');
    assert.deepEqual(
      extraneous,
      [],
      `${workflow.name} triggers on both pull_request and push to ${extraneous.join(', ')}, so PR branches run twice`,
    );
  }
});

// Tests that never run in CI are the purest false negative: js/test held a
// failing repository-layout test for months because no workflow invoked it.
test('the JavaScript unit tests are executed by a workflow', () => {
  const jsTests = readdirSync(join(repoRoot, 'js', 'test')).filter((name) => name.endsWith('.test.mjs'));
  assert.ok(jsTests.length > 0, 'expected JavaScript tests under js/test');
  assert.ok(
    workflows.some((workflow) => /npm run test:js/.test(workflow.text)),
    'no workflow runs "npm run test:js", so js/test/*.test.mjs never executes in CI',
  );
});

for (const workflow of workflows) {
  // A job without a timeout can hang for the runner's six-hour default and
  // burn the whole CI budget before anyone notices.
  test(`${workflow.name}: every job declares timeout-minutes`, () => {
    for (const job of readJobs(workflow.text)) {
      assert.match(
        job.body,
        /^ {4}timeout-minutes: \d+$/m,
        `job "${job.name}" in ${workflow.name} has no timeout-minutes`,
      );
    }
  });

  // Least privilege: the default token must be read-only unless a job opts in.
  test(`${workflow.name}: declares a top-level permissions block`, () => {
    assert.match(
      workflow.text,
      /^permissions:$/m,
      `${workflow.name} does not declare top-level permissions`,
    );
  });

  // Every job must be covered by a concurrency group, either its own (writers)
  // or the workflow-level one (readers), so superseded runs do not pile up.
  test(`${workflow.name}: every job is covered by a concurrency group`, () => {
    const hasWorkflowLevel = /^concurrency:$/m.test(workflow.text);
    for (const job of readJobs(workflow.text)) {
      assert.ok(
        hasWorkflowLevel || /^ {4}concurrency:$/m.test(job.body),
        `job "${job.name}" in ${workflow.name} has no concurrency group`,
      );
    }
  });

  // `continue-on-error` turns a real failure into a green run - the exact
  // false negative that hid the Windows test failures reported in issue #96.
  test(`${workflow.name}: does not mask failures with continue-on-error`, () => {
    assert.ok(
      !/continue-on-error/.test(workflow.text),
      `${workflow.name} uses continue-on-error, which masks real failures`,
    );
  });

  // In YAML a leading `!` starts a tag, so `if: !cancelled() && ...` is a
  // parse error. The expression has to be wrapped in ${{ }} on a single line.
  test(`${workflow.name}: single-line if expressions do not start with !`, () => {
    const offenders = workflow.text
      .split('\n')
      .map((line, index) => ({ line, number: index + 1 }))
      .filter(({ line }) => /^\s*if:\s*!/.test(line));
    assert.deepEqual(
      offenders.map(({ number }) => number),
      [],
      `${workflow.name} has unwrapped "if: !..." expressions (wrap them in \${{ }})`,
    );
  });

  // Every advertised release mode must be handled by a job. Before issue #96
  // the C# pipeline offered a "changeset-pr" mode that no job implemented, so
  // selecting it produced a successful run that did nothing at all.
  test(`${workflow.name}: every release_mode option is handled by a job`, () => {
    const modes = readChoiceOptions(workflow.text, 'release_mode');
    for (const mode of modes) {
      assert.ok(
        workflow.text.includes(`release_mode == '${mode}'`),
        `${workflow.name} offers release_mode "${mode}" but no job guards on it`,
      );
    }
  });
}
