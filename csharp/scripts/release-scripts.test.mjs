import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import test from 'node:test';

const repoRoot = new URL('../..', import.meta.url).pathname;

function runNode(script, args, options = {}) {
  return execFileSync(process.execPath, [join(repoRoot, script), ...args], {
    cwd: options.cwd ?? repoRoot,
    encoding: 'utf8',
    env: { ...process.env, ...(options.env ?? {}) },
  });
}

function runGit(args, cwd) {
  return execFileSync('git', args, {
    cwd,
    encoding: 'utf8',
  });
}

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

  runNode('csharp/scripts/merge-changesets.mjs', [
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

  const stdout = runNode('csharp/scripts/create-github-release.mjs', [
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

test('create-github-release dry run reports matching assets without uploading', () => {
  const dir = mkdtempSync(join(tmpdir(), 'link-cli-release-assets-'));
  const changelog = join(dir, 'CHANGELOG.md');
  writeFileSync(
    changelog,
    '# Changelog\n\n## [2.4.0] - 2026-05-12\n\nFixed release automation.\n'
  );
  const artifacts = join(dir, 'artifacts');
  mkdirSync(artifacts, { recursive: true });
  writeFileSync(join(artifacts, 'clink.2.4.0.nupkg'), 'fake');
  writeFileSync(join(artifacts, 'clink.2.4.0.snupkg'), 'fake');
  writeFileSync(join(artifacts, 'unrelated.txt'), 'fake');

  const stdout = runNode('csharp/scripts/create-github-release.mjs', [
    '--release-version',
    '2.4.0',
    '--repository',
    'link-foundation/link-cli',
    '--tag-prefix',
    'csharp-v',
    '--changelog-path',
    changelog,
    '--assets-glob',
    join(artifacts, '*.nupkg'),
    '--dry-run',
  ]);
  const payload = JSON.parse(stdout.slice(stdout.indexOf('{')));

  assert.equal(payload.tag_name, 'csharp-v2.4.0');
});

test('version-and-commit creates a C# release commit when the next tag is missing', () => {
  const dir = mkdtempSync(join(tmpdir(), 'link-cli-csharp-version-'));
  const remote = join(dir, 'remote.git');
  const work = join(dir, 'work');
  const outputFile = join(dir, 'github-output.txt');

  mkdirSync(work, { recursive: true });
  runGit(['init', '--bare', '--initial-branch=main', remote], dir);
  runGit(['init', '-b', 'main'], work);
  runGit(['config', 'user.name', 'Test User'], work);
  runGit(['config', 'user.email', 'test@example.com'], work);

  const projectDir = join(work, 'csharp', 'Foundation.Data.Doublets.Cli');
  const changesetDir = join(work, 'csharp', '.changeset');
  mkdirSync(projectDir, { recursive: true });
  mkdirSync(changesetDir, { recursive: true });
  writeFileSync(
    join(projectDir, 'Foundation.Data.Doublets.Cli.csproj'),
    '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><Version>2.3.0</Version></PropertyGroup></Project>\n'
  );
  writeFileSync(
    join(changesetDir, 'release.md'),
    "---\n'Foundation.Data.Doublets.Cli': minor\n---\n\nFix release automation.\n"
  );

  runGit(['add', '.'], work);
  runGit(['commit', '-m', 'initial'], work);
  runGit(['remote', 'add', 'origin', remote], work);
  runGit(['push', '-u', 'origin', 'main'], work);

  runNode('csharp/scripts/version-and-commit.mjs', ['--mode', 'changeset'], {
    cwd: work,
    env: { GITHUB_OUTPUT: outputFile },
  });

  const csproj = readFileSync(
    join(projectDir, 'Foundation.Data.Doublets.Cli.csproj'),
    'utf8'
  );
  const outputs = readFileSync(outputFile, 'utf8');

  assert.match(csproj, /<Version>2\.4\.0<\/Version>/);
  assert.match(outputs, /^version_committed=true$/m);
  assert.match(outputs, /^new_version=2\.4\.0$/m);
  assert.match(runGit(['rev-parse', '--verify', 'csharp-v2.4.0'], work), /[a-f0-9]{40}/);
  assert.match(runGit(['ls-remote', '--tags', 'origin', 'csharp-v2.4.0'], work), /csharp-v2\.4\.0/);
});
