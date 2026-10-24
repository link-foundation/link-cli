// Regression tests for the CI/CD invariants restored in issue #96.
//
// These run without any third-party dependency (the lint jobs call them with a
// bare `node --test`), so the workflows are inspected with a small line-based
// scanner instead of a YAML parser. Every rule below encodes a defect that was
// actually present in this repository at some point.

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
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

/** A job that pushes to the repository or publishes to a registry or Pages. */
function isWriterJob(body) {
  return /^ {6}(?:contents|pages|id-token|packages): write$/m.test(body);
}

const trustedPublishingAuth = /^\s*uses:\s*rust-lang\/crates-io-auth-action@/m;

/** Splits a workflow into `{ first, last, text }` entries, one per job step. */
function readSteps(text) {
  const steps = [];
  let current = null;
  text.split('\n').forEach((line, index) => {
    const start = /^(\s*)- (?:name|uses|id|run|if):/.exec(line);
    if (start && (!current || start[1].length <= current.indent)) {
      current = { indent: start[1].length, first: index + 1, lines: [line] };
      steps.push(current);
    } else if (current && line.trim() !== '' && line.length - line.trimStart().length <= current.indent) {
      current = null; // left the steps list
    } else if (current) {
      current.lines.push(line);
    }
  });
  return steps.map((step) => ({
    first: step.first,
    last: step.first + step.lines.length - 1,
    text: step.lines.join('\n'),
  }));
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

  // A workflow-level group holds the whole run, release and registry wait
  // included, so on main a third push cancelled the queued second run, and a
  // cancellable one would interrupt a writer that already pushed (issue #134).
  // Writers own a never-cancelled group instead.
  test(`${workflow.name}: writer jobs own a never-cancelled concurrency group`, () => {
    const writers = readJobs(workflow.text).filter((job) => isWriterJob(job.body));
    if (writers.length === 0) return;
    assert.doesNotMatch(
      workflow.text,
      /^concurrency:$/m,
      `${workflow.name} has writer jobs (${writers.map((job) => job.name).join(', ')}) but a workflow-level concurrency group`,
    );
    for (const job of writers) {
      assert.match(
        job.body,
        /^ {4}concurrency:\n {6}group: [^\n]+\n {6}cancel-in-progress: false$/m,
        `writer job "${job.name}" in ${workflow.name} needs its own concurrency group with cancel-in-progress: false`,
      );
    }
  });

  // `continue-on-error` turns a real failure into a green run - the exact
  // false negative that hid the Windows test failures reported in issue #96.
  // The crates.io trusted publishing exchange used to be the one exception,
  // but its expected failure left a red annotation on every green release
  // (issue #134), so it is now gated on a repository variable instead.
  test(`${workflow.name}: does not mask failures with continue-on-error`, () => {
    const offenders = workflow.text
      .split('\n')
      .map((line, index) => ({ line, number: index + 1 }))
      .filter(({ line }) => /^\s*continue-on-error:/.test(line));
    assert.deepEqual(
      offenders.map(({ number }) => `${workflow.name}:${number}`),
      [],
      'continue-on-error masks real failures',
    );
  });

  // Trusted publishing that is not configured on crates.io fails with
  // "No Trusted Publishing config found", so it must only run once it is.
  test(`${workflow.name}: crates.io trusted publishing runs only when enabled`, () => {
    for (const step of readSteps(workflow.text).filter((s) => trustedPublishingAuth.test(s.text))) {
      assert.match(
        step.text,
        /^\s*if: vars\.CRATES_IO_TRUSTED_PUBLISHING == 'true' && /m,
        `${workflow.name}:${step.first} exchanges a trusted publishing token without checking that it is configured`,
      );
      const id = /^\s*id:\s*(\S+)\s*$/m.exec(step.text)?.[1];
      assert.ok(id, `${workflow.name}:${step.first}: the trusted publishing step needs an id`);
      const consumers = workflow.text.split('\n').filter((other) => other.includes(`steps.${id}.outputs.token`));
      assert.ok(consumers.length > 0, `${workflow.name}: nothing reads steps.${id}.outputs.token`);
      for (const consumer of consumers) {
        assert.match(
          consumer,
          new RegExp(`steps\\.${id}\\.outputs\\.token \\|\\| secrets\\.`),
          `${workflow.name}: "${consumer.trim()}" has no token secret for runs without trusted publishing`,
        );
      }
    }
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

  // A release whose publish step quietly skips on a missing credential still
  // ends green, so a version is tagged and announced but never reaches the
  // registry (issue #134: the C# NuGet push printed "NUGET_API_KEY not set,
  // skipping NuGet publish"). Missing credentials must fail the job instead.
  test(`${workflow.name}: publish steps do not silently skip on missing credentials`, () => {
    const offenders = readSteps(workflow.text)
      .filter(({ text }) => /\bsecrets\.[A-Z0-9_]*(?:TOKEN|KEY)\b/.test(text))
      .map((step) => ({ ...step, code: step.text.replace(/^\s*#.*$/gm, '') }))
      .filter(({ code }) => /(?:not set|not configured|missing)[^\n]*\bskipp(?:ing|ed)\b|\bskipping\b[^\n]*publish/i.test(code))
      .map(({ first }) => first);
    assert.deepEqual(offenders, [], `${workflow.name} skips publishing on missing credentials at lines ${offenders}`);
  });

  // `gh workflow run` and the REST API refuse a dispatch that omits a
  // required input without a default, although the web form preselects the
  // first option, so the same release cannot be started from the CLI.
  test(`${workflow.name}: required choice inputs declare a default`, () => {
    const lines = workflow.text.split('\n');
    const offenders = [];
    lines.forEach((line, index) => {
      if (line.trim() !== 'type: choice') return;
      const indent = line.length - line.trimStart().length;
      let start = index;
      while (start > 0 && lines[start - 1].length - lines[start - 1].trimStart().length >= indent) start -= 1;
      let end = index;
      while (end + 1 < lines.length && (lines[end + 1].trim() === '' || lines[end + 1].length - lines[end + 1].trimStart().length >= indent)) end += 1;
      const body = lines.slice(start, end + 1).join('\n');
      if (/^\s*required: true$/m.test(body) && !/^\s*default:/m.test(body)) {
        offenders.push(lines[start - 1].trim());
      }
    });
    assert.deepEqual(offenders, [], `${workflow.name} has required choice inputs without a default`);
  });

  // `ubuntu-latest` moved these jobs to Ubuntu 26.04 mid-rollout while the
  // runner-images README still documented it as 24.04, with no diff in this
  // repository (issue #134). Runner OS upgrades belong in a dedicated PR.
  test(`${workflow.name}: hosted runners use pinned OS labels`, () => {
    const offenders = workflow.text
      .split('\n')
      .map((line, index) => ({ line: line.replace(/#.*$/, ''), number: index + 1 }))
      .filter(({ line }) => /\b(?:ubuntu|macos|windows)-latest\b/.test(line))
      .map(({ number }) => number);
    assert.deepEqual(offenders, [], `${workflow.name} uses a -latest runner label at lines ${offenders}`);
  });

  // actions/checkout runs `git init` before any step can configure Git, so
  // only the environment reaches it; without it every checkout printed the
  // "Using 'master' as the name for the initial branch" hint (issue #134).
  test(`${workflow.name}: provides init.defaultBranch to actions/checkout`, () => {
    if (!/uses: actions\/checkout@/.test(workflow.text)) return;
    assert.match(
      workflow.text,
      /^env:\n(?: {2}.*\n)*? {2}GIT_CONFIG_KEY_0: init\.defaultBranch$/m,
      `${workflow.name} does not set init.defaultBranch through GIT_CONFIG_* env`,
    );
  });
}

// GitHub Pages serves a single site per repository, so two workflows that both
// upload a Pages artifact silently overwrite each other: whichever deploys last
// wins and the other one's URLs answer 404. Until issue #96 both docs.yml and
// wasm.yml deployed, which is why the API reference links in README.md were
// broken. docs.yml now assembles everything into one artifact.
test('exactly one workflow publishes GitHub Pages', () => {
  const publishers = workflows
    .filter((workflow) => /uses:\s*actions\/deploy-pages@/.test(workflow.text))
    .map((workflow) => workflow.name);
  assert.deepEqual(
    publishers,
    ['docs.yml'],
    `expected docs.yml to be the only Pages publisher, found: ${publishers.join(', ')}`,
  );
});

test('only the Pages publisher uploads a Pages artifact', () => {
  const uploaders = workflows
    .filter((workflow) => /uses:\s*actions\/upload-pages-artifact@/.test(workflow.text))
    .map((workflow) => workflow.name);
  assert.deepEqual(
    uploaders,
    ['docs.yml'],
    `a second Pages artifact replaces the published site, found: ${uploaders.join(', ')}`,
  );
});

// `cargo clippy` without `-D warnings` prints its findings and exits 0, so lint
// regressions land silently. Both Rust workspaces must be gated (issue #96).
test('every clippy invocation denies warnings', () => {
  for (const workflow of workflows) {
    const invocations = workflow.text
      .split('\n')
      .map((line, index) => ({ line: line.trim(), number: index + 1 }))
      .filter(({ line }) => /^(run:\s*)?cargo clippy\b/.test(line));
    for (const { line, number } of invocations) {
      assert.match(
        line,
        /--\s+-D\s+warnings/,
        `${workflow.name}:${number}: "${line}" does not fail on clippy warnings`,
      );
    }
  }
});

// A pull request that is green on its own head can still break main: a clean
// textual merge is not necessarily a compiling one. Each stack that has a
// pull-request pipeline re-runs its checks on the simulated merge result.
test('the Rust and C# pipelines simulate a fresh merge on pull requests', () => {
  for (const name of ['rust.yml', 'csharp.yml']) {
    const workflow = workflows.find((candidate) => candidate.name === name);
    assert.ok(workflow, `${name} is missing`);
    assert.match(
      workflow.text,
      /simulate-fresh-merge\.sh/,
      `${name} does not run .github/scripts/simulate-fresh-merge.sh`,
    );
  }
});

// The dependency check reads every manifest git tracks (issue #104), so
// Dependabot must watch each of their directories, or nothing proposes the
// update the check asks for.
// cargo audit reports unmaintained, unsound and yanked advisories as warnings
// and still exits 0, so the audit job passed while listing findings.
test('every cargo audit invocation denies warnings', () => {
  const offenders = workflows.flatMap(({ name, text }) =>
    text
      .split('\n')
      .filter((line) => /^\s*run: cargo audit\b/.test(line) && !/--deny warnings\b/.test(line))
      .map((line) => `${name}: ${line.trim()}`),
  );
  assert.deepEqual(offenders, []);
});

// Codecov rejects tokenless uploads from this repository, and the upload ran
// with `fail_ci_if_error: false`, so it failed on every run without anyone
// noticing; its file search also uploaded a Rust test file whose name
// contains "coverage" (issue #134).
test('every Codecov upload uses a token, reads only its reports and fails loudly', () => {
  const uploads = workflows.flatMap(({ name, text }) =>
    readSteps(text)
      .filter((step) => /uses: codecov\/codecov-action@/.test(step.text))
      .map((step) => ({ ...step, name })),
  );
  for (const { name, first, text } of uploads) {
    assert.match(text, /^\s*token: \$\{\{ secrets\.CODECOV_TOKEN \}\}$/m, `${name}:${first} uploads without a token`);
    assert.match(text, /^\s*fail_ci_if_error: true$/m, `${name}:${first} hides a failed upload`);
    assert.match(text, /^\s*(?:directory|files):/m, `${name}:${first} searches the whole tree for reports`);
  }
});

test('Dependabot watches every directory the dependency check reads', () => {
  const config = readFileSync(join(repoRoot, '.github', 'dependabot.yml'), 'utf8');
  const watched = (ecosystem) => {
    const block = config.split(/\n(?=  - package-ecosystem:)/).find((part) => part.includes(`package-ecosystem: ${ecosystem}\n`));
    return [...block.matchAll(/^\s+(?:- |directory: )(\/[\w./-]*)\s*$/gm)].map(([, directory]) => directory);
  };
  const tracked = execFileSync('git', ['ls-files', '-z'], { cwd: repoRoot, encoding: 'utf8' }).split('\0');
  const directoriesOf = (pattern, needs) =>
    tracked
      .filter((file) => pattern.test(file) && needs.test(readFileSync(join(repoRoot, file), 'utf8')))
      .map((file) => `/${dirname(file)}`.replace(/\/\.$/, ''));
  const expectations = {
    cargo: directoriesOf(/(^|\/)Cargo\.toml$/, /^\[(?:.*\.)?(?:dev-|build-)?dependencies/m),
    nuget: directoriesOf(/\.csproj$/, /<PackageReference /),
    npm: directoriesOf(/(^|\/)package\.json$/, /"(?:dev)?[dD]ependencies"/),
  };
  for (const [ecosystem, directories] of Object.entries(expectations)) {
    for (const directory of directories) {
      assert.ok(
        watched(ecosystem).some((root) => directory === root || directory.startsWith(`${root}/`)),
        `${ecosystem} manifest in ${directory} is not watched by Dependabot`,
      );
    }
  }
});
