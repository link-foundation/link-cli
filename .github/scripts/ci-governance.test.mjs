// Repository governance rules of issue #111 that are not the stage gate
// itself (the gate rule lives in workflow-policy.test.mjs): the required
// status checks always report, both suites prove they loaded the golden
// fixtures, idle pull requests go stale, and a red Dependencies check on main
// is tracked in an issue.
import assert from 'node:assert/strict';
import test from 'node:test';
import { readJobs, workflows } from './workflow-files.mjs';

test('the required stage checks report on every pull request (issue #111)', () => {
  for (const name of ['javascript.yml', 'rust.yml', 'csharp.yml']) {
    const { text } = workflows.find((workflow) => workflow.name === name);
    const on = text.slice(text.indexOf('\non:\n'), text.indexOf('\njobs:\n'));
    assert.match(on, /^ {2}pull_request:/m, `${name} must run on pull requests`);
    assert.doesNotMatch(
      on,
      /^ {4}paths(?:-ignore)?:/m,
      `${name}: a path filter would skip the parity check and leave its required check pending`,
    );
    assert.match(text, /^ {2}pipeline-status:\n {4}name: Pipeline Status \(\w+\)/m);
  }
});

test('both suites prove they loaded the shared golden fixtures (issue #111)', () => {
  for (const [name, suite] of [
    ['rust.yml', 'rust'],
    ['csharp.yml', 'csharp'],
  ]) {
    const { text } = workflows.find((workflow) => workflow.name === name);
    const tests = readJobs(text).find((job) => job.name === 'test').body;
    assert.match(
      tests,
      new RegExp(`GOLDEN_FIXTURES_RECEIPT: \\$\\{\\{ runner\\.temp \\}\\}/golden-${suite}\\.json`),
    );
    assert.match(
      tests,
      new RegExp(`node parity/check-parity\\.mjs --receipt ${suite} "\\$RECEIPT"`),
    );
  }
});

test('idle pull requests are flagged after 60 days and then closed (issue #111)', () => {
  const { text } = workflows.find(({ name }) => name === 'stale.yml');
  assert.match(text, /uses: actions\/stale@v\d+/);
  assert.match(text, /days-before-pr-stale: 60\n/);
  assert.match(text, /days-before-pr-close: [1-9]\d*\n/);
  // Issues track wanted work; only pull requests go stale.
  assert.match(text, /days-before-issue-stale: -1\n/);
  assert.match(text, /days-before-issue-close: -1\n/);
  assert.match(text, /^ {2}schedule:/m);
});

test('a red Dependencies check on main opens or updates a tracking issue (issue #111)', () => {
  const { text } = workflows.find(({ name }) => name === 'dependencies.yml');
  const job = readJobs(text).find(({ name }) => name === 'tracking-issue');
  assert.ok(job, 'dependencies.yml has no tracking-issue job');
  assert.match(job.body, /^ {6}- latest$/m);
  assert.match(job.body, /if: \$\{\{ !cancelled\(\) && github\.ref == 'refs\/heads\/main'/);
  assert.match(job.body, /^ {6}issues: write$/m);
  assert.match(job.body, /run: node \.github\/scripts\/dependency-tracking-issue\.mjs/);
  assert.match(text, /^ {2}schedule:/m);
});

// .github/zizmor.yml disables the self-repository audit because actionlint
// cannot parse `uses: $/...` yet (rhysd/actionlint#711). That is safe only
// while every local reference is a job-level reusable workflow call, which
// GitHub resolves from the caller's commit rather than the workspace.
test('no step loads a local action from the workspace', () => {
  for (const { name, text } of workflows) {
    assert.doesNotMatch(text, /^\s+- uses: \.\//m, `${name}: a step-level uses: ./ action`);
    assert.doesNotMatch(text, /^ {6,}uses: \.\//m, `${name}: a step-level uses: ./ action`);
  }
});

// The rules on main require the three stage checks, and the release jobs'
// version commits are new commits without checks. Only the release deploy key
// bypasses those rules; GITHUB_TOKEN cannot be a bypass actor (issue #111).
test('jobs that push a version commit to main use the release deploy key (issue #111)', () => {
  const pushers = workflows.flatMap(({ name, text }) =>
    readJobs(text)
      .filter(({ body }) => /version-and-commit\.(?:mjs|rs)/.test(body))
      .map((job) => ({ workflow: name, ...job })),
  );
  assert.deepEqual(pushers.map(({ workflow, name }) => `${workflow}: ${name}`).sort(), [
    'csharp.yml: instant-release',
    'csharp.yml: release',
    'rust.yml: auto-release',
    'rust.yml: manual-release',
  ]);
  for (const { workflow, name, body } of pushers) {
    const where = `${workflow}: ${name}`;
    assert.match(
      body,
      /^ {4}environment: release$/m,
      `${where} must run in the release environment`,
    );
    const checkout = /- uses: actions\/checkout@\S+\n(?: {8}.*\n)+/.exec(body)?.[0] ?? '';
    assert.match(checkout, /^ {10}ssh-key: \$\{\{ secrets\.RELEASE_DEPLOY_KEY \}\}$/m, where);
    assert.doesNotMatch(checkout, /^ {10}token:/m, `${where} must not push with a token`);
  }
});
