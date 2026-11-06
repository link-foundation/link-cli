import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

const detector = new URL('./detect-code-changes.mjs', import.meta.url).pathname;

test('a failed PR Git comparison fails change detection', { timeout: 10000 }, () => {
  const dir = mkdtempSync(join(tmpdir(), 'link-cli-detection-failure-'));
  try {
    const git = join(dir, 'git');
    writeFileSync(
      git,
      '#!/bin/sh\nif [ "$1" = diff ]; then echo "comparison failed" >&2; exit 42; fi\nexit 0\n',
    );
    chmodSync(git, 0o755);
    const result = spawnSync(process.execPath, [detector], {
      cwd: dir,
      encoding: 'utf8',
      env: {
        ...process.env,
        PATH: `${dir}:${process.env.PATH}`,
        GITHUB_EVENT_NAME: 'pull_request',
        GITHUB_BASE_SHA: '1'.repeat(40),
        GITHUB_HEAD_SHA: '2'.repeat(40),
        GITHUB_OUTPUT: '',
      },
    });
    assert.equal(result.status, 1, result.stdout + result.stderr);
    assert.match(result.stderr, /comparison failed/);
    assert.doesNotMatch(result.stdout, /cs-changed=false/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test(
  'a first commit checks all files instead of treating Git failure as no changes',
  { timeout: 10000 },
  () => {
    const dir = mkdtempSync(join(tmpdir(), 'link-cli-detection-first-'));
    try {
      const git = (...args) => execFileSync('git', args, { cwd: dir, stdio: 'pipe' });
      git('init', '-b', 'main');
      git('config', 'user.name', 'Regression test');
      git('config', 'user.email', 'test@example.invalid');
      writeFileSync(join(dir, 'Program.cs'), 'class Program {}\n');
      git('add', 'Program.cs');
      git('commit', '-m', 'Initial source');
      const output = execFileSync(process.execPath, [detector], {
        cwd: dir,
        encoding: 'utf8',
        stdio: 'pipe',
        env: { ...process.env, GITHUB_EVENT_NAME: 'push', GITHUB_OUTPUT: '' },
      });
      assert.match(output, /cs-changed=true/);
      assert.match(output, /csharp-code-changed=true/);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  },
);

test('PR change detection requires both comparison SHAs', { timeout: 10000 }, () => {
  const result = spawnSync(process.execPath, [detector], {
    encoding: 'utf8',
    env: {
      ...process.env,
      GITHUB_EVENT_NAME: 'pull_request',
      GITHUB_BASE_SHA: '',
      GITHUB_HEAD_SHA: '',
      GITHUB_OUTPUT: '',
    },
  });
  assert.equal(result.status, 1, result.stdout + result.stderr);
  assert.match(result.stderr, /GITHUB_BASE_SHA.*GITHUB_HEAD_SHA/);
});

test('PR change detection rejects invalid commit identities', { timeout: 10000 }, () => {
  const result = spawnSync(process.execPath, [detector], {
    encoding: 'utf8',
    env: {
      ...process.env,
      GITHUB_EVENT_NAME: 'pull_request',
      GITHUB_BASE_SHA: '--help',
      GITHUB_HEAD_SHA: '2'.repeat(40),
      GITHUB_OUTPUT: '',
    },
  });
  assert.equal(result.status, 1, result.stdout + result.stderr);
  assert.match(result.stderr, /Invalid PR commit identity/);
});

test('moving C# code into examples still requires a changeset', { timeout: 10000 }, () => {
  const dir = mkdtempSync(join(tmpdir(), 'link-cli-detection-move-'));
  try {
    const git = (...args) => execFileSync('git', args, { cwd: dir, stdio: 'pipe' });
    git('init', '-b', 'main');
    git('config', 'user.name', 'Regression test');
    git('config', 'user.email', 'test@example.invalid');
    mkdirSync(join(dir, 'csharp'));
    mkdirSync(join(dir, 'examples'));
    writeFileSync(join(dir, 'csharp', 'Program.cs'), 'class Program {}\n');
    git('add', '.');
    git('commit', '-m', 'Initial source');
    git('mv', 'csharp/Program.cs', 'examples/Program.cs');
    git('commit', '-m', 'Move source');
    const output = execFileSync(process.execPath, [detector], {
      cwd: dir,
      encoding: 'utf8',
      stdio: 'pipe',
      env: { ...process.env, GITHUB_EVENT_NAME: 'push', GITHUB_OUTPUT: '' },
    });
    assert.match(output, /csharp-code-changed=true/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
