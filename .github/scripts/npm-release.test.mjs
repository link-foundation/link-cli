import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { readJobs, workflows } from './workflow-files.mjs';

for (const name of ['js-client', 'js-server']) {
  test(`${name}: pull requests validate the tarball without npm registry credentials`, () => {
    const { text } = workflows.find((workflow) => workflow.name === `${name}.yml`);
    const jobs = readJobs(text);
    const { body: pack } = jobs.find((job) => job.name === 'package-check');
    const { body: release } = jobs.find((job) => job.name === 'release');
    const { body: preflight } = jobs.find((job) => job.name === 'release-preflight');
    const mainOnly = /if: github\.event_name == 'push' && github\.ref == 'refs\/heads\/main'/;
    assert.match(pack, /if: inputs\.verify_trusted_publishing != true/);
    assert.match(
      pack,
      /shell: bash\n +run: npm pack --dry-run --json \| node .*check-npm-tarball-size\.mjs/,
    );
    assert.doesNotMatch(pack, /main-writer|npm (view|publish)|id-token: write/);
    assert.match(release, mainOnly);
    assert.match(release, /needs: \[release-preflight, package-check,/);
    assert.match(preflight, mainOnly);
  });
}

test('the npm tarball size check requires a report and identifies the largest files on failure', () => {
  const script = fileURLToPath(new URL('./check-npm-tarball-size.mjs', import.meta.url));
  const validate = (report) =>
    spawnSync(process.execPath, [script], {
      input: JSON.stringify(report),
      encoding: 'utf8',
    });
  const maxTarballBytes = 157 * 1024 * 1024;
  const pack = {
    name: 'fixture',
    size: maxTarballBytes,
    files: [
      { path: 'small.js', size: 1 },
      { path: 'large.bin', size: maxTarballBytes },
    ],
  };
  const accepted = validate([pack]);
  assert.equal(accepted.status, 0, accepted.stderr);
  assert.match(accepted.stdout, /compressed bytes/);

  const oversized = validate([{ ...pack, size: maxTarballBytes + 1 }]);
  assert.equal(oversized.status, 1);
  assert.match(oversized.stderr, /Largest files: "large\.bin".*"small\.js"/);

  const missing = validate([]);
  assert.equal(missing.status, 1);
  assert.match(missing.stderr, /Expected one npm pack report/);
});
