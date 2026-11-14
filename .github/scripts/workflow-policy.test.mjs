// Regression tests for the CI/CD invariants restored in issue #96.
//
// These run without any third-party dependency (the lint jobs call them with a
// bare `node --test`), so the workflows are inspected with a small line-based
// scanner instead of a YAML parser. Every rule below encodes a defect that was
// actually present in this repository at some point.

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { dependencyFiles } from './check-latest-dependencies.mjs';
import { readJobs, repoRoot, workflows, workflowsDir } from './workflow-files.mjs';

test('generic hooks preserve immutable evidence while secrets still scan it', () => {
  const config = readFileSync(join(repoRoot, '.pre-commit-config.yaml'), 'utf8');
  const generic = config.split('  - repo: local')[0];
  assert.match(generic, /exclude: \^\(dev\/log\/\|docs\/case-studies\/\)/);
  const secrets = config.match(/ {6}- id: secrets\n([\s\S]*?)(?=\n {6}- id:|$)/)?.[1];
  assert.ok(secrets, 'tracked-file secrets hook must remain installed');
  assert.doesNotMatch(secrets, /\bexclude:/);
  assert.match(secrets, /entry: node \.github\/scripts\/check-secrets\.mjs/);
});

test('there is at least one workflow to inspect', () => {
  assert.ok(workflows.length > 0, `no workflows found in ${workflowsDir}`);
});

test('CodeQL scans the complete checkout without a truncated PR diff', () => {
  const security = workflows.find(({ name }) => name === 'security.yml');
  const codeql = readJobs(security.text).find(({ name }) => name === 'codeql');
  assert.match(codeql.body, /^ {6}CODEQL_OVERLAY_DATABASE_MODE: none$/m);
  assert.match(codeql.body, /^ {6}CODEQL_ACTION_DIFF_INFORMED_QUERIES: 'false'$/m);
  assert.match(codeql.body, /check-codeql-results\.mjs codeql-results/);
});

/** A job that calls a reusable workflow instead of running steps. */
function isCallerJob(body) {
  return /^ {4}uses: /m.test(body);
}

/** A job that pushes to the repository or publishes to a registry or Pages. */
function isWriterJob(body) {
  // Coverage uses OIDC too, but does not mutate the repository or release.
  return /^ {6}(?:contents|pages|packages): write$/m.test(body) || /npm publish/.test(body);
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
    } else if (
      current &&
      line.trim() !== '' &&
      line.length - line.trimStart().length <= current.indent
    ) {
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
  const jsTests = readdirSync(join(repoRoot, 'js', 'test')).filter((name) =>
    name.endsWith('.test.mjs'),
  );
  assert.ok(jsTests.length > 0, 'expected JavaScript tests under js/test');
  assert.ok(
    workflows.some((workflow) => /npm run test:js/.test(workflow.text)),
    'no workflow runs "npm run test:js", so js/test/*.test.mjs never executes in CI',
  );
});

for (const workflow of workflows) {
  // A job without a timeout can hang for the runner's six-hour default and
  // burn the whole CI budget before anyone notices.
  // A job that calls a reusable workflow (`uses:`) cannot set
  // timeout-minutes; the called workflow's own jobs carry it.
  test(`${workflow.name}: every job declares timeout-minutes`, () => {
    for (const job of readJobs(workflow.text).filter(({ body }) => !isCallerJob(body))) {
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
    for (const job of readJobs(workflow.text).filter(({ body }) => !isCallerJob(body))) {
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

  // An authentication attempt may fail so the preflight can classify it or
  // verify a fallback. The following preflight must enforce the final verdict.
  test(`${workflow.name}: only classified authentication attempts may continue after failure`, () => {
    for (const job of readJobs(workflow.text)) {
      for (const step of readSteps(job.body).filter(({ text }) =>
        /continue-on-error:/.test(text),
      )) {
        assert.equal(workflow.name, 'csharp.yml');
        assert.equal(job.name, 'release-preflight');
        assert.match(step.text, /id: preflight_nuget/);
        assert.match(step.text, /uses: NuGet\/login@/);
        assert.match(job.body, /NUGET_OIDC_OUTCOME: \$\{\{ steps\.preflight_nuget\.outcome \}\}/);
        assert.match(job.body, /run: node \.github\/scripts\/release-preflight\.mjs nuget/);
      }
    }
  });

  // Trusted publishing that is not configured on crates.io fails with
  // "No Trusted Publishing config found", so it must only run once it is.
  test(`${workflow.name}: crates.io trusted publishing runs only when enabled`, () => {
    const releaseJobs = readJobs(workflow.text).filter(
      ({ name }) => name !== 'verify-trusted-publishing',
    );
    for (const step of releaseJobs
      .flatMap(({ body }) => readSteps(body))
      .filter((s) => trustedPublishingAuth.test(s.text))) {
      assert.match(
        step.text,
        /^\s*if: vars\.CRATES_IO_TRUSTED_PUBLISHING == 'true' && /m,
        `${workflow.name}:${step.first} exchanges a trusted publishing token without checking that it is configured`,
      );
      const id = /^\s*id:\s*(\S+)\s*$/m.exec(step.text)?.[1];
      assert.ok(id, `${workflow.name}:${step.first}: the trusted publishing step needs an id`);
      const consumers = workflow.text
        .split('\n')
        .filter((other) => other.includes(`steps.${id}.outputs.token`));
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
      .filter(({ code }) =>
        /(?:not set|not configured|missing)[^\n]*\bskipp(?:ing|ed)\b|\bskipping\b[^\n]*publish/i.test(
          code,
        ),
      )
      .map(({ first }) => first);
    assert.deepEqual(
      offenders,
      [],
      `${workflow.name} skips publishing on missing credentials at lines ${offenders}`,
    );
  });

  // `if npm view pkg@version` reads every failure, including an unreachable
  // registry, as "not published", so main then publishes an existing version
  // and fails with a misleading error (issue #134). Only E404 means "missing".
  test(`${workflow.name}: npm lookups tell "not published" apart from a failure`, () => {
    const offenders = readSteps(workflow.text)
      .filter(({ text }) => /\bnpm view\b/.test(text.replace(/^\s*#.*$/gm, '')))
      .filter(({ text }) => !/code E404/.test(text) || /npm view[^\n]*>\s*\/dev\/null/.test(text))
      .map(({ first }) => first);
    assert.deepEqual(
      offenders,
      [],
      `${workflow.name} treats any npm view failure as "not published" at lines ${offenders}`,
    );
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
      while (start > 0 && lines[start - 1].length - lines[start - 1].trimStart().length >= indent)
        start -= 1;
      let end = index;
      while (
        end + 1 < lines.length &&
        (lines[end + 1].trim() === '' ||
          lines[end + 1].length - lines[end + 1].trimStart().length >= indent)
      )
        end += 1;
      const body = lines.slice(start, end + 1).join('\n');
      if (/^\s*required: true$/m.test(body) && !/^\s*default:/m.test(body)) {
        offenders.push(lines[start - 1].trim());
      }
    });
    assert.deepEqual(
      offenders,
      [],
      `${workflow.name} has required choice inputs without a default`,
    );
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
    assert.deepEqual(
      offenders,
      [],
      `${workflow.name} uses a -latest runner label at lines ${offenders}`,
    );
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

// Issue #136: Codecov is optional, with OIDC instead of a repository secret.
// Only an explicitly inactive/missing repository may relax upload errors;
// malformed responses and operational failures remain errors.
test('coverage uploads use OIDC and retain reports without requiring Codecov', () => {
  let uploads = 0;
  for (const { name, text } of workflows) {
    assert.doesNotMatch(
      text,
      /secrets\.CODECOV_TOKEN|^ {2}codecov-token:/m,
      `${name} requires a Codecov secret`,
    );
    for (const job of readJobs(text)) {
      for (const step of readSteps(job.body).filter(({ text }) =>
        /uses: codecov\/codecov-action@/.test(text),
      )) {
        uploads += 1;
        assert.match(step.text, /uses: codecov\/codecov-action@v7/);
        assert.match(step.text, /^\s*use_oidc: true$/m);
        assert.match(job.body, /^ {6}id-token: write$/m);
        assert.match(
          step.text,
          /^\s*fail_ci_if_error: \$\{\{ steps\.codecov\.outputs\.required \}\}$/m,
        );
        assert.match(job.body, /node .github\/scripts\/coverage\.mjs codecov/);
        assert.match(step.text, /^\s*(?:directory|files):/m);
        assert.match(step.text, /^\s*plugins: noop$/m);
        assert.match(job.body, /node .github\/scripts\/coverage\.mjs summary/);
        assert.match(job.body, /uses: actions\/upload-artifact@/);
        assert.match(job.body, /if-no-files-found: error/);
        if (name === 'csharp.yml') {
          // Release lambdas can have compiled branches without source lines;
          // Cobertura omits those methods from its source listing.
          assert.match(job.body, /XPlat Code Coverage;Format=json,cobertura/);
          assert.match(job.body, /TestResults\/\*\*\/coverage\.json/);
          assert.match(
            job.body,
            /python3 experiments\/issue-138\/check-causal-raw-coverage\.py[^\n]* --require-session/,
          );
        }
      }
    }
  }
  assert.ok(uploads > 0, 'coverage uploading was removed');
});

test('Rust PR lint compiles release helpers without executing them', () => {
  const rust = workflows.find(({ name }) => name === 'rust.yml');
  const lint = readJobs(rust.text).find(({ name }) => name === 'lint');
  assert.match(lint.body, /run: bash rust\/scripts\/check-scripts\.sh/);
});

test('registry trust can be verified without versioning or publishing a release', () => {
  for (const name of ['rust.yml', 'csharp.yml']) {
    const workflow = workflows.find((candidate) => candidate.name === name);
    assert.ok(
      readChoiceOptions(workflow.text, 'release_mode').includes('verify-trusted-publishing'),
    );
    const verify = readJobs(workflow.text).find(({ name }) => name === 'verify-trusted-publishing');
    assert.ok(verify, `${name} needs an authentication-only verification job`);
    assert.match(
      verify.body,
      /github.event_name == 'workflow_dispatch'.*release_mode == 'verify-trusted-publishing'/,
    );
    assert.match(verify.body, /^ {6}id-token: write$/m);
    assert.doesNotMatch(
      verify.body,
      /secrets\.|cargo publish|nuget push|version-and-commit|contents: write/,
    );
    assert.match(verify.body, name === 'rust.yml' ? trustedPublishingAuth : /uses: NuGet\/login@/);
    for (const job of readJobs(workflow.text).filter(({ name }) =>
      ['lint', 'test'].includes(name),
    )) {
      assert.match(
        job.body,
        /inputs.release_mode != 'verify-trusted-publishing'/,
        `${name}: authentication-only dispatch must not start or cancel ${job.name}`,
      );
    }
  }
  const npm = workflows.find(({ name }) => name === 'js-client.yml');
  const verify = readJobs(npm.text).find(({ name }) => name === 'verify-trusted-publishing');
  assert.match(
    verify.body,
    /github.event_name == 'workflow_dispatch' && inputs.verify_trusted_publishing/,
  );
  assert.match(verify.body, /^ {6}id-token: write$/m);
  assert.match(verify.body, /node .github\/scripts\/verify-npm-trust.mjs/);
  assert.doesNotMatch(verify.body, /secrets\.|npm publish/);
  assert.doesNotMatch(
    npm.text,
    /secrets\.NPM_TOKEN|NODE_AUTH_TOKEN/,
    'verified npm OIDC publishing must not fall back to a long-lived token',
  );
});

test('Dependabot watches every directory the dependency check reads', () => {
  const config = readFileSync(join(repoRoot, '.github', 'dependabot.yml'), 'utf8');
  const watched = (ecosystem) => {
    const block = config
      .split(/\n(?= {2}- package-ecosystem:)/)
      .find((part) => part.includes(`package-ecosystem: ${ecosystem}\n`));
    return [...block.matchAll(/^\s+(?:- |directory: )(\/[\w./-]*)\s*$/gm)].map(
      ([, directory]) => directory,
    );
  };
  const tracked = dependencyFiles(
    execFileSync('git', ['ls-files', '-z'], {
      cwd: repoRoot,
      encoding: 'utf8',
    }).split('\0'),
  );
  const directoriesOf = (pattern, needs) =>
    tracked
      .filter(
        (file) => pattern.test(file) && needs.test(readFileSync(join(repoRoot, file), 'utf8')),
      )
      .map((file) => `/${dirname(file)}`.replace(/\/\.$/, ''));
  const expectations = {
    cargo: directoriesOf(/(^|\/)Cargo\.toml$/, /^\[(?:.*\.)?(?:dev-|build-)?dependencies/m),
    nuget: directoriesOf(/\.csproj$/, /<PackageReference /),
    npm: directoriesOf(/(^|\/)package\.json$/, /"(?:dev)?[dD]ependencies"/),
    pip: directoriesOf(/(^|\/)requirements[^/]*\.txt$/, /==/),
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

// The combined `actions/cache` saved in its post step on every pull request
// miss. Pull request caches are scoped to their merge ref, so nothing else
// could restore them, and multi-GB rust/target archives kept evicting main's
// caches under the 10 GB repository quota. The Windows Rust test job then
// compiled from scratch and spent 3-5 minutes compressing ~3 GB after every
// test had passed, until run 42877437350 was cancelled in that post step at
// its 45-minute limit (issue #172).
test('caches are restored everywhere but saved only by main pushes on a miss', () => {
  for (const { name, text } of workflows) {
    assert.doesNotMatch(text, /uses: actions\/cache@/, `${name} uses the always-saving cache`);
    for (const job of readJobs(text)) {
      const steps = readSteps(job.body);
      const restores = steps.filter((step) => /uses: actions\/cache\/restore@/.test(step.text));
      const saves = steps.filter((step) => /uses: actions\/cache\/save@/.test(step.text));
      assert.equal(saves.length, restores.length, `${name} ${job.name} restores without saving`);
      for (const [index, save] of saves.entries()) {
        const id = /^\s+id: (\S+)$/m.exec(restores[index].text)?.[1];
        assert.ok(id, `${name} ${job.name}: the restore step needs an id`);
        assert.match(
          save.text,
          /github\.event_name == 'push' && github\.ref == 'refs\/heads\/main'/,
        );
        assert.ok(
          save.text.includes(`steps.${id}.outputs.cache-hit != 'true'`),
          `${name} ${job.name}`,
        );
        assert.ok(save.text.includes(`key: \${{ steps.${id}.outputs.cache-primary-key }}`));
        const paths = (step) =>
          /path: \|\n([\s\S]*?)\n\s+key:/
            .exec(step.text)?.[1]
            .split('\n')
            .filter((line) => !line.trim().startsWith('#'))
            .join('\n')
            .trim();
        assert.equal(paths(save), paths(restores[index]), `${name} ${job.name} saves other paths`);
      }
    }
  }
});

test('the Rust test cache is saved before the long test run, under its own key', () => {
  const rust = workflows.find(({ name }) => name === 'rust.yml');
  const test = readJobs(rust.text).find(({ name }) => name === 'test');
  const save = test.body.indexOf('uses: actions/cache/save@');
  assert.ok(save > test.body.indexOf('--all-features --no-run'));
  assert.ok(save < test.body.indexOf('- name: Run tests'));
  assert.match(test.body, /key: \$\{\{ runner\.os \}\}-cargo-test-/);
});

// A failed credential preflight on main used to skip lint, test and build, so
// a registry problem also hid whether the code was broken (issue #172). Only
// the jobs that publish may wait on the preflight; the gate still fails main.
test('a failed release preflight blocks publishing but never the code checks', () => {
  const publishers = {
    'csharp.yml': ['release', 'instant-release'],
    'rust.yml': ['auto-release', 'manual-release'],
    'js-client.yml': ['release'],
  };
  const checks = ['lint', 'test', 'fresh-merge', 'build', 'unit', 'conformance'];
  for (const [name, releaseJobs] of Object.entries(publishers)) {
    const { text } = workflows.find((workflow) => workflow.name === name);
    const jobs = readJobs(text);
    for (const job of jobs.filter(({ name: job }) => checks.includes(job))) {
      assert.doesNotMatch(
        job.body,
        /release-preflight/,
        `${name} ${job.name} waits on the preflight`,
      );
    }
    for (const releaseJob of releaseJobs) {
      const job = jobs.find(({ name: job }) => job === releaseJob);
      assert.ok(job, `${name} has no ${releaseJob} job`);
      assert.match(job.body, /needs: \[release-preflight, /, `${name} ${releaseJob}`);
      const condition = /^ {4}if: (.*)$/m.exec(job.body)?.[1];
      if (condition) assert.match(condition, /needs\.release-preflight\.result == 'success'/);
    }
    const gate = jobs.find(({ name: job }) => job === 'pipeline-status');
    assert.match(gate.body, /- release-preflight/, `${name} gate ignores the preflight`);
  }
});

// === Stage gate (issue #111) ===
// Rust and C# jobs may start only after the JavaScript stage (lint,
// `npm run test:js`, parity check) succeeded for the same commit. Each
// workflow calls javascript-stage.yml as job `javascript`, and every job that
// installs or runs Rust or .NET lists `javascript` in its own `needs:`. A job
// whose `if:` overrides the implicit success check (`always()`,
// `!cancelled()`, `failure()`) must test `needs.javascript.result` itself, or
// a failed, cancelled or skipped JavaScript stage would still start it.
const JS_STAGE = './.github/workflows/javascript-stage.yml';
const nativeTooling =
  /(?<![\w-])(?:cargo|rustup|rustc|wasm-pack|rust-script|dotnet)(?![\w-])|dtolnay\/rust-toolchain|actions\/setup-dotnet|taiki-e\/install-action/;

/** Drops YAML comments so a comment that names cargo does not count. */
function withoutComments(body) {
  return body
    .split('\n')
    .filter((line) => !/^\s*#/.test(line))
    .map((line) => line.replace(/\s+#\s.*$/, ''))
    .join('\n');
}

/** Reads `needs: x`, `needs: [a, b]` and the block-list form. */
function readNeeds(body) {
  const inline = /^ {4}needs:[ \t]*(.+)$/m.exec(body);
  if (inline) {
    return inline[1]
      .replace(/^\[|\]$/g, '')
      .split(',')
      .map((need) => need.trim())
      .filter(Boolean);
  }
  const block = /^ {4}needs:\n((?: {6}- .+\n?)+)/m.exec(body);
  return block
    ? block[1]
        .split('\n')
        .map((line) => line.replace(/^ {6}- /, '').trim())
        .filter(Boolean)
    : [];
}

/** Reads a job's `if:`, including a `|` block. */
function readIf(body) {
  const match = /^ {4}if:[ \t]*(.*)$((?:\n {6}.*)*)/m.exec(body);
  return match ? `${match[1]}${match[2]}` : '';
}

/** Lists the jobs of a workflow that could start Rust or C# without the gate. */
export function gateViolations(text, readWorkflow = () => '') {
  const jobs = readJobs(text);
  const violations = [];
  const caller = jobs.find(({ name }) => name === 'javascript');
  for (const job of jobs) {
    if (job.name === 'javascript') continue;
    const called = /^ {4}uses: (\.\/\.github\/workflows\/[\w.-]+)/m.exec(job.body)?.[1];
    const body = withoutComments(called ? readWorkflow(called) : job.body);
    if (!nativeTooling.test(body)) continue;
    if (!caller || !caller.body.includes(`uses: ${JS_STAGE}`)) {
      violations.push(`${job.name}: the workflow has no "javascript" job calling ${JS_STAGE}`);
      continue;
    }
    if (!readNeeds(job.body).includes('javascript')) {
      violations.push(`${job.name}: runs Rust or .NET but does not list javascript in needs`);
    }
    const condition = readIf(job.body);
    if (
      /\b(?:always|cancelled|failure)\(\)/.test(condition) &&
      !/needs\.javascript\.result == 'success'/.test(condition)
    ) {
      violations.push(
        `${job.name}: its if: overrides the success check without needs.javascript.result == 'success'`,
      );
    }
  }
  return violations;
}

test('the gate rule rejects workflows that bypass the JavaScript stage', () => {
  const stage = `  javascript:\n    uses: ${JS_STAGE}\n`;
  const job = (lines) =>
    `  rust:\n    runs-on: ubuntu-24.04\n${lines}    steps:\n      - run: cargo test\n`;
  assert.deepEqual(gateViolations(`jobs:\n${stage}${job('    needs: [javascript]\n')}`), []);
  assert.deepEqual(
    gateViolations(`jobs:\n${stage}${job('    needs:\n      - lint\n      - javascript\n')}`),
    [],
  );
  assert.match(gateViolations(`jobs:\n${job('')}`).join(), /no "javascript" job/);
  assert.match(
    gateViolations(`jobs:\n${stage}${job('    needs: lint\n')}`).join(),
    /does not list/,
  );
  for (const condition of [
    'always()',
    '${{ !cancelled() }}',
    "|\n      !cancelled() && x == 'y'",
  ]) {
    assert.match(
      gateViolations(
        `jobs:\n${stage}${job(`    needs: [javascript]\n    if: ${condition}\n`)}`,
      ).join(),
      /overrides the success check/,
    );
  }
  const gated =
    "    needs: [javascript]\n    if: ${{ !cancelled() && needs.javascript.result == 'success' }}\n";
  assert.deepEqual(gateViolations(`jobs:\n${stage}${job(gated)}`), []);
  // Hyphenated names (`cargo-audit` in a needs list) and comments are not tooling.
  const gate = `  status:\n    needs: [cargo-audit]\n    steps:\n      # cargo test runs in the rust job\n      - run: echo ok\n`;
  assert.deepEqual(gateViolations(`jobs:\n${gate}`), []);
  // A job calling another reusable workflow is judged by that workflow's steps.
  const call = `  nested:\n    uses: ./.github/workflows/build.yml\n`;
  assert.match(gateViolations(`jobs:\n${stage}${call}`, () => job('')).join(), /nested: runs Rust/);
});

for (const workflow of workflows.filter(({ name }) => name !== 'javascript-stage.yml')) {
  test(`${workflow.name}: Rust and C# jobs wait for the JavaScript stage (issue #111)`, () => {
    const read = (path) => readFileSync(join(repoRoot, path), 'utf8');
    assert.deepEqual(gateViolations(workflow.text, read), []);
  });
}

test('the JavaScript stage needs no Rust or .NET and runs lint, tests and parity', () => {
  const stage = workflows.find(({ name }) => name === 'javascript-stage.yml');
  assert.ok(stage, 'javascript-stage.yml is missing');
  assert.doesNotMatch(withoutComments(stage.text), nativeTooling);
  assert.match(stage.text, /^on:\n {2}workflow_call:\n/m);
  for (const command of [
    'npm ci',
    'npm run lint',
    'npm run test:js',
    'node parity/check-parity.mjs',
  ]) {
    assert.ok(stage.text.includes(`run: ${command}`), `the JavaScript stage must run "${command}"`);
  }
  // The one-sided change rule needs the base commit in the clone.
  assert.match(stage.text, /fetch-depth: 0/);
  assert.match(
    stage.text,
    /PARITY_BASE_SHA: \$\{\{ github\.event\.pull_request\.base\.sha \|\| github\.event\.before \}\}/,
  );
  const wasm = workflows.find(({ name }) => name === 'wasm.yml');
  assert.doesNotMatch(wasm.text, /run: npm run test:js/, 'test:js belongs to the JavaScript stage');
});
