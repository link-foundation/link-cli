import assert from 'node:assert/strict';
import test from 'node:test';
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
    assert.match(pack, /run: npm pack --dry-run/);
    assert.doesNotMatch(pack, /main-writer|npm (view|publish)|id-token: write/);
    assert.match(release, mainOnly);
    assert.match(release, /needs: \[release-preflight, package-check,/);
    assert.match(preflight, mainOnly);
  });
}
