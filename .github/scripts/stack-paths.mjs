#!/usr/bin/env node
// Decides whether a pull request touches a stack (issue #111).
//
// rust.yml and csharp.yml used `on.pull_request.paths`, so a pull request
// outside those paths never produced their "Pipeline Status" checks and a
// branch protection rule requiring them could never be satisfied. The
// workflows now run on every pull request; this script replaces the filter
// inside the run, so the expensive jobs still skip unrelated changes while
// the JavaScript stage and the pipeline status always report.
//
// Usage: node .github/scripts/stack-paths.mjs <rust|csharp>
// Env:   GITHUB_EVENT_NAME, GITHUB_BASE_SHA, GITHUB_HEAD_SHA, GITHUB_OUTPUT,
//        STACK_PATHS_DEBUG=1 to list every changed file and its verdict.
// Writes `stack-changed=true|false` to $GITHUB_OUTPUT. Events other than
// pull_request always check the whole stack.
import { execFileSync } from 'node:child_process';
import { appendFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { globToRegExp } from '../../parity/check-parity.mjs';

// Every shared input of a stack: its own tree, the fixtures its golden
// fixture test reads, the parity manifests and the CI code that runs it.
const common = [
  'docs/protocol/**',
  'parity/**',
  '.github/workflows/javascript.yml',
  '.github/workflows/javascript-stage.yml',
  '.github/scripts/**',
];
export const STACK_PATHS = {
  rust: ['rust/**', '.github/workflows/rust.yml', ...common],
  csharp: ['csharp/**', 'examples/*-csharp/**', '.github/workflows/csharp.yml', ...common],
};

/** Returns the changed files that belong to the stack. */
export function stackFiles(stack, files) {
  const globs = STACK_PATHS[stack];
  if (!globs) throw new Error(`Unknown stack "${stack}"; expected ${Object.keys(STACK_PATHS)}`);
  const patterns = globs.map(globToRegExp);
  return files.filter((file) => patterns.some((pattern) => pattern.test(file)));
}

export function changedFiles(base, head, cwd = process.cwd()) {
  if (!base || !head) throw new Error('GITHUB_BASE_SHA and GITHUB_HEAD_SHA are required');
  // Three dots: only what the pull request changed since it left the base.
  return execFileSync('git', ['diff', '--name-only', '-z', `${base}...${head}`], {
    cwd,
    encoding: 'utf8',
  })
    .split('\0')
    .filter(Boolean);
}

export function run(stack, env = process.env, log = console.log) {
  let changed = true;
  if (env.GITHUB_EVENT_NAME === 'pull_request') {
    const files = changedFiles(env.GITHUB_BASE_SHA, env.GITHUB_HEAD_SHA);
    const matched = stackFiles(stack, files);
    if (env.STACK_PATHS_DEBUG === '1')
      for (const file of files) log(`${matched.includes(file) ? '+' : '-'} ${file}`);
    changed = matched.length > 0;
    log(
      `${stack}: ${matched.length} of ${files.length} changed files belong to the stack` +
        (changed ? ` (first: ${matched[0]})` : '; its build and test jobs are skipped'),
    );
  } else {
    log(`${stack}: ${env.GITHUB_EVENT_NAME || 'local'} event checks the whole stack`);
  }
  if (env.GITHUB_OUTPUT) appendFileSync(env.GITHUB_OUTPUT, `stack-changed=${changed}\n`);
  return changed;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    run(process.argv[2]);
  } catch (error) {
    console.error(`::error::${error.message}`);
    process.exitCode = 1;
  }
}
