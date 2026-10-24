// Tests for printUntrusted (issue #60): contributor-controlled text printed
// by the changeset scripts must be bracketed between ::stop-commands::<token>
// and ::<token>:: on CI, because the runner parses workflow commands out of
// anything a step prints -- including `##[` appearing mid-line in a
// changeset description.
import { describe, expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { printUntrusted } from './print-untrusted.mjs';

const here = join(fileURLToPath(import.meta.url), '..');
const VALIDATE_SCRIPT = join(here, 'validate-changeset.mjs');
const MERGE_SCRIPT = join(here, 'merge-changesets.mjs');

// Runs fn with process.env overrides; undefined deletes the variable, which
// matters for GITHUB_ACTIONS: CI runners export it as 'true' and the
// plain-print behaviour must be exercised without it.
function withEnv(overrides, fn) {
  const saved = new Map();
  for (const [key, value] of Object.entries(overrides)) {
    saved.set(key, process.env[key]);
    if (value === undefined) delete process.env[key];
    else process.env[key] = value;
  }
  try {
    fn();
  } finally {
    for (const [key, value] of saved) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  }
}

describe('printUntrusted', () => {
  test('brackets the text between stop-commands and its resume on CI', () => {
    const lines = [];
    withEnv({ GITHUB_ACTIONS: 'true' }, () => {
      printUntrusted('##[error]cache poisoned', (line) => lines.push(line));
    });

    expect(lines).toHaveLength(3);
    const token = /^::stop-commands::([0-9a-f]+)$/.exec(lines[0])[1];
    expect(token).toHaveLength(32);
    expect(lines[1]).toBe('##[error]cache poisoned');
    expect(lines[2]).toBe(`::${token}::`);
  });

  test('every call gets a fresh token, so printed text cannot resume early', () => {
    const first = [];
    const second = [];
    withEnv({ GITHUB_ACTIONS: 'true' }, () => {
      printUntrusted('::stop-commands::pause-logging', (l) => first.push(l));
      printUntrusted('second', (l) => second.push(l));
    });

    const firstToken = /^::stop-commands::(.+)$/.exec(first[0])[1];
    const secondToken = /^::stop-commands::(.+)$/.exec(second[0])[1];

    expect(firstToken).not.toBe('pause-logging');
    expect(firstToken).not.toBe(secondToken);
    expect(first[2]).toBe(`::${firstToken}::`);
  });

  test('multi-line text stays verbatim inside the bracket', () => {
    const lines = [];
    const text = 'line one\nline two ##[add-mask]x\nline three';
    withEnv({ GITHUB_ACTIONS: 'true' }, () => {
      printUntrusted(text, (line) => lines.push(line));
    });

    expect(lines).toHaveLength(3);
    expect(lines[1]).toBe(text);
  });

  test('prints plainly outside GitHub Actions', () => {
    const lines = [];
    withEnv({ GITHUB_ACTIONS: undefined }, () => {
      printUntrusted('##[error]harmless locally', (line) => lines.push(line));
    });

    expect(lines).toEqual(['##[error]harmless locally']);
  });

  test('markers and text share one stream', () => {
    const out = [];
    const err = [];
    withEnv({ GITHUB_ACTIONS: 'true' }, () => {
      printUntrusted('payload', (line) => err.push(line));
    });

    expect(err).toHaveLength(3);
    expect(out).toEqual([]);
  });
});

// Asserts that somewhere in `output` a stop-commands bracket surrounds the
// payload, and returns the token used.
function expectBracketed(output, payload) {
  const stopMatch = /::stop-commands::([0-9a-f]{32})/.exec(output);
  expect(stopMatch, 'expected a stop-commands marker').not.toBeNull();

  const token = stopMatch[1];
  const stopIndex = output.indexOf(`::stop-commands::${token}`);
  const payloadIndex = output.indexOf(payload);
  const resumeIndex = output.indexOf(`::${token}::`, payloadIndex);

  expect(payloadIndex).toBeGreaterThan(stopIndex);
  expect(resumeIndex).toBeGreaterThan(payloadIndex);
  return token;
}

function runScript(scriptPath, cwd, env = {}) {
  return spawnSync('bun', ['run', scriptPath], {
    cwd,
    env: { ...process.env, GITHUB_ACTIONS: 'true', ...env },
    encoding: 'utf-8',
  });
}

function setupChangesetDir(files) {
  const dir = mkdtempSync(join(tmpdir(), 'print-untrusted-'));
  mkdirSync(join(dir, '.changeset'), { recursive: true });
  for (const [name, content] of Object.entries(files)) {
    writeFileSync(join(dir, '.changeset', name), content);
  }
  return dir;
}

describe('validate-changeset integration', () => {
  test('the printed description of a valid changeset is bracketed', () => {
    const payload = '##[error]this line would annotate the run';
    const dir = setupChangesetDir({
      'good.md': `---\n'MyPackage': patch\n---\n\nFix the parser. ${payload}\n`,
    });
    try {
      const result = runScript(
        VALIDATE_SCRIPT,
        dir,
        { GITHUB_BASE_REF: '', GITHUB_BASE_SHA: '', GITHUB_HEAD_SHA: '' }
      );

      expect(result.status).toBe(0);
      expectBracketed(result.stdout, payload);
    } finally {
      rmSync(dir, { force: true, recursive: true });
    }
  });

  test('the printed file content of a rejected changeset is bracketed on stderr', () => {
    const payload = 'Body with ##[error]payload inside';
    const dir = setupChangesetDir({
      'bad.md': `---\n'NotMyPackage': major\n---\n\n${payload}\n`,
    });
    try {
      const result = runScript(
        VALIDATE_SCRIPT,
        dir,
        { GITHUB_BASE_REF: '', GITHUB_BASE_SHA: '', GITHUB_HEAD_SHA: '' }
      );

      expect(result.status).toBe(1);
      expectBracketed(result.stderr, payload);
    } finally {
      rmSync(dir, { force: true, recursive: true });
    }
  });
});

describe('merge-changesets integration', () => {
  test('the merged changeset content is bracketed', () => {
    const payload = 'Add feature B ##[add-mask]would-burn-a-mask-slot';
    const dir = setupChangesetDir({
      'a.md': `---\n'MyPackage': patch\n---\n\nAdd feature A\n`,
      'b.md': `---\n'MyPackage': minor\n---\n\n${payload}\n`,
    });
    try {
      const result = runScript(MERGE_SCRIPT, dir);

      expect(result.status).toBe(0);
      expectBracketed(result.stdout, payload);
    } finally {
      rmSync(dir, { force: true, recursive: true });
    }
  });
});
