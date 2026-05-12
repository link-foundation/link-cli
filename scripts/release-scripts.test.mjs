import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import test from 'node:test';

const repoRoot = new URL('..', import.meta.url).pathname;

function runNode(script, args, options = {}) {
  return execFileSync(process.execPath, [join(repoRoot, script), ...args], {
    cwd: options.cwd ?? repoRoot,
    encoding: 'utf8',
    env: { ...process.env, ...(options.env ?? {}) },
  });
}

test('get-bump-type reads fragments from the requested directory', () => {
  const dir = mkdtempSync(join(tmpdir(), 'link-cli-bump-'));
  const changelogDir = join(dir, 'rust', 'changelog.d');
  mkdirSync(changelogDir, { recursive: true });
  const outputFile = join(dir, 'outputs.txt');

  writeFileSync(
    join(changelogDir, 'minor.md'),
    '---\nbump: minor\n---\n\nAdd a release improvement.\n'
  );
  writeFileSync(
    join(changelogDir, 'patch.md'),
    '---\nbump: patch\n---\n\nFix release metadata.\n'
  );

  const stdout = runNode('scripts/get-bump-type.mjs', ['--dir', changelogDir], {
    env: { GITHUB_OUTPUT: outputFile },
  });

  assert.match(stdout, /Determined bump type: minor/);
  const outputs = readFileSync(outputFile, 'utf8');
  assert.match(outputs, /^bump_type=minor$/m);
  assert.match(outputs, /^has_fragments=true$/m);
});

test('merge-changesets honors the requested directory and package name', () => {
  const dir = mkdtempSync(join(tmpdir(), 'link-cli-changesets-'));
  const changesetDir = join(dir, 'csharp', '.changeset');
  mkdirSync(changesetDir, { recursive: true });

  writeFileSync(
    join(changesetDir, 'one.md'),
    "---\n'Foundation.Data.Doublets.Cli': patch\n---\n\nFirst fix.\n"
  );
  writeFileSync(
    join(changesetDir, 'two.md'),
    "---\n'Foundation.Data.Doublets.Cli': minor\n---\n\nSecond fix.\n"
  );

  runNode('scripts/merge-changesets.mjs', [
    '--dir',
    changesetDir,
    '--package-name',
    'Foundation.Data.Doublets.Cli',
  ]);

  const files = readdirSync(changesetDir).filter((file) => file.endsWith('.md'));
  assert.equal(files.length, 1);
  const merged = readFileSync(join(changesetDir, files[0]), 'utf8');
  assert.match(merged, /'Foundation\.Data\.Doublets\.Cli': minor/);
  assert.match(merged, /First fix\./);
  assert.match(merged, /Second fix\./);
});

test('create-github-release dry run uses tag prefix and component changelog', () => {
  const dir = mkdtempSync(join(tmpdir(), 'link-cli-release-'));
  const changelog = join(dir, 'CHANGELOG.md');
  writeFileSync(
    changelog,
    '# Changelog\n\n## [2.4.0] - 2026-05-12\n\nFixed release automation.\n\n## [2.3.0] - 2026-05-01\n\nPrevious release.\n'
  );

  const stdout = runNode('scripts/create-github-release.mjs', [
    '--release-version',
    '2.4.0',
    '--repository',
    'link-foundation/link-cli',
    '--tag-prefix',
    'csharp-v',
    '--language',
    'C#',
    '--package-id',
    'clink',
    '--changelog-path',
    changelog,
    '--dry-run',
  ]);
  const payload = JSON.parse(stdout.slice(stdout.indexOf('{')));

  assert.equal(payload.tag_name, 'csharp-v2.4.0');
  assert.equal(payload.name, 'C# v2.4.0');
  assert.match(payload.body, /Fixed release automation\./);
  assert.match(payload.body, /Package: `clink`/);
});

test('collect-changelog honors component paths from the repository root', () => {
  const dir = mkdtempSync(join(tmpdir(), 'link-cli-collect-'));
  const rustDir = join(dir, 'rust');
  const changelogDir = join(rustDir, 'changelog.d');
  mkdirSync(changelogDir, { recursive: true });

  writeFileSync(join(rustDir, 'Cargo.toml'), 'version = "2.4.0"\n');
  writeFileSync(
    join(rustDir, 'CHANGELOG.md'),
    '# Changelog\n\n## [2.3.0] - 2026-05-01\n\nPrevious release.\n'
  );
  writeFileSync(
    join(changelogDir, 'entry.md'),
    '---\nbump: minor\n---\n\nCollected component changelog.\n'
  );

  runNode(
    'scripts/collect-changelog.mjs',
    ['--dir', 'rust/changelog.d', '--output', 'rust/CHANGELOG.md'],
    { cwd: dir }
  );

  const changelog = readFileSync(join(rustDir, 'CHANGELOG.md'), 'utf8');
  assert.match(changelog, /## \[2\.4\.0\] - \d{4}-\d{2}-\d{2}/);
  assert.match(changelog, /Collected component changelog\./);
  assert.equal(existsSync(join(changelogDir, 'entry.md')), false);
});
