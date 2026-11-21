import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { excludedProjects, repoRoot, verifyPackages } from './verify-packages.mjs';

const packages = [
  { packageId: 'clink', version: '5.1.0' },
  { packageId: 'Foundation.Data.Doublets.Cli', version: '5.1.0' },
];
const expected = packages.map(({ packageId, version }) => `${packageId}.${version}.nupkg`);

test('every example and experiment project explicitly disables packing', () => {
  const projects = excludedProjects();
  assert.ok(projects.length >= 20);
  for (const project of projects) {
    assert.match(
      readFileSync(join(repoRoot, project), 'utf8'),
      /<IsPackable>false<\/IsPackable>/,
      project,
    );
  }
});

test('package allowlist rejects extra, missing, stale and mismatched packages', () => {
  verifyPackages(expected, packages);
  verifyPackages([...expected, 'README.md'], packages);
  for (const files of [
    [],
    expected.slice(1),
    [...expected, 'Probe.1.0.0.nupkg'],
    [...expected, 'clink.4.0.6.nupkg'],
    [...expected, 'Probe.1.0.0.snupkg'],
  ]) {
    assert.throws(() => verifyPackages(files, packages), /Unexpected NuGet artifacts/);
  }
  assert.throws(
    () => verifyPackages(expected, [packages[0], { ...packages[1], version: '5.1.1' }]),
    /versions must match/,
  );
});

test('every pack/upload/publish path validates the package allowlist', () => {
  const workflow = readFileSync(join(repoRoot, '.github/workflows/csharp.yml'), 'utf8');
  const packCommands = workflow.match(
    /dotnet pack --no-build[^\n]*\n[^\n]*node scripts\/verify-packages.mjs/g,
  );
  assert.equal(packCommands.length, 3);
  assert.match(workflow, /run: node scripts\/verify-packages.mjs --pack-excluded/);
  assert.match(workflow, /- publish-current/);
  assert.match(
    workflow,
    /name: Check for changesets\n\s+if: inputs.release_mode != 'publish-current'/,
  );
  assert.match(workflow, /workflow_dispatch' && inputs.release_mode == 'publish-current'/);
  assert.match(workflow, /check-release-needed.mjs --library-csproj/);
});
