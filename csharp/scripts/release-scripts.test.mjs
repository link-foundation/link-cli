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
