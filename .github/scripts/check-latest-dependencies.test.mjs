// Tests for the dependency freshness check. They use no network: the parsers
// and the version comparison decide whether a declared version is behind, so
// a parser that misses an entry would be a silent false negative.
import test from 'node:test';
import assert from 'node:assert/strict';

import {
  compareVersions,
  isOutdated,
  latestStable,
  parseCargoManifest,
  parseCargoUpdates,
  parseCsproj,
  parsePackageJson,
  parsePackageLock,
  parseWorkflowActions,
  parseWorkflowTools,
  versionParts,
} from './check-latest-dependencies.mjs';

test('versions compare numerically, not as text', () => {
  assert.deepEqual(versionParts('v24.21.0'), [24, 21, 0]);
  assert.deepEqual(versionParts('10.0.x'), [10, 0]);
  assert.deepEqual(versionParts('1.2.3-beta.1'), [1, 2, 3]);
  assert.equal(compareVersions('0.2.129', '0.2.13'), 1);
  assert.equal(compareVersions('2.0', '2.0.0'), 0);
  assert.equal(compareVersions('18.9.0', '18.10.1'), -1);
});

test('a declared version is compared only as far as it is written', () => {
  assert.equal(isOutdated('v7', 'v7'), false);
  assert.equal(isOutdated('v6', 'v7'), true);
  assert.equal(isOutdated('24.x', 'v24.21.0'), false);
  assert.equal(isOutdated('20.x', 'v24.21.0'), true);
  assert.equal(isOutdated('10.0.x', '10.0'), false);
  assert.equal(isOutdated('2.0.20', '2.0.21'), true);
  assert.equal(isOutdated('2.0.21', '2.0.21'), false);
  // A version ahead of the registry (for example a yanked release) is not behind.
  assert.equal(isOutdated('3.0.0', '2.9.9'), false);
});

test('pre-releases are never the latest version', () => {
  assert.equal(latestStable(['1.0.0', '1.1.0-rc.1', '1.0.1']), '1.0.1');
  assert.equal(latestStable(['v6', 'v6.1.0', 'v7.0.0-beta', 'v7-preview']), 'v6.1.0');
  assert.equal(latestStable([]), undefined);
});

test('every Cargo dependency form is found, path-only ones are skipped', () => {
  const manifest = `
[package]
version = "9.9.9"

[dependencies]
thiserror = "2.0.21" # a comment
link-cli = { path = ".." }
serde = { version = "1.0.229", features = ["derive"] }

[dev-dependencies]
tempfile = "3.27.0"

[target.'cfg(unix)'.dependencies]
libc = "0.2.180"

[dependencies.web-sys]
version = "0.3.106"
features = [
  "console",
]

[features]
default = ["x"]
`;
  assert.deepEqual(parseCargoManifest(manifest), [
    { name: 'thiserror', version: '2.0.21' },
    { name: 'serde', version: '1.0.229' },
    { name: 'tempfile', version: '3.27.0' },
    { name: 'libc', version: '0.2.180' },
    { name: 'web-sys', version: '0.3.106' },
  ]);
});

test('NuGet package references are found', () => {
  const project = `<ItemGroup>
    <PackageReference Include="xunit" Version="2.9.3" />
    <PackageReference Include="coverlet.collector" Version="10.1.0">
      <PrivateAssets>all</PrivateAssets>
    </PackageReference>
    <ProjectReference Include="..\\Library.csproj" />
  </ItemGroup>`;
  assert.deepEqual(parseCsproj(project), [
    { name: 'xunit', version: '2.9.3' },
    { name: 'coverlet.collector', version: '10.1.0' },
  ]);
});

test('npm ranges lose their operator, and locked versions come from the lock file', () => {
  const manifest = JSON.stringify({
    dependencies: { react: '^19.3.0' },
    devDependencies: { '@vitejs/plugin-react': '~6.1.1', vite: '8.3.2' },
  });
  const dependencies = parsePackageJson(manifest);
  assert.deepEqual(dependencies, [
    { name: 'react', version: '19.3.0' },
    { name: '@vitejs/plugin-react', version: '6.1.1' },
    { name: 'vite', version: '8.3.2' },
  ]);
  const lock = JSON.stringify({
    packages: {
      '': {},
      'node_modules/react': { version: '19.2.8' },
      'node_modules/vite': { version: '8.3.2' },
      'node_modules/nested/node_modules/react': { version: '1.0.0' },
    },
  });
  assert.deepEqual(parsePackageLock(lock, dependencies.map(({ name }) => name)), [
    { name: 'react', version: '19.2.8' },
    { name: 'vite', version: '8.3.2' },
  ]);
});

test('actions pinned to a major tag are found once per repository', () => {
  const workflow = `
      - uses: actions/checkout@v7
      - uses: github/codeql-action/init@v4
      - uses: github/codeql-action/analyze@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: ./.github/actions/local
      - uses: actions/setup-node@v7
`;
  assert.deepEqual(parseWorkflowActions(workflow), [
    { name: 'actions/checkout', version: 'v7' },
    { name: 'github/codeql-action', version: 'v4' },
    { name: 'actions/setup-node', version: 'v7' },
  ]);
});

test('runtimes and installed tools are found', () => {
  const workflow = `
        with:
          node-version: '24.x'
          dotnet-version: "10.0.x"
      - uses: taiki-e/install-action@v2
        with:
          tool: cargo-audit@0.22.2, wasm-pack
`;
  assert.deepEqual(parseWorkflowTools(workflow), [
    { kind: 'node', name: 'Node.js', version: '24.x' },
    { kind: 'dotnet', name: '.NET', version: '10.0.x' },
    { kind: 'crate', name: 'cargo-audit', version: '0.22.2' },
  ]);
});

test('only real updates of cargo update --dry-run count', () => {
  const report = `    Updating crates.io index
     Locking 2 packages to latest compatible versions
    Updating autocfg v1.5.0 -> v1.5.1
    Updating thiserror v2.0.20 -> v2.0.21
      Adding tokio v1.53.2
   Unchanged rand v0.8.5 (available: v0.9.2)
warning: aborting update due to dry run
`;
  assert.deepEqual(parseCargoUpdates(report), [
    { name: 'autocfg', version: '1.5.0', latest: '1.5.1' },
    { name: 'thiserror', version: '2.0.20', latest: '2.0.21' },
  ]);
});
