#!/usr/bin/env node
// Runs `node --test OPTIONS FILES` over the `*.test.mjs` files of a directory:
// `node scripts/run-tests.mjs DIRECTORY [OPTIONS...]`. Node.js 20 expands no
// glob that `--test` is given, and npm runs scripts with cmd.exe on Windows,
// which expands none either, so `test/*.test.mjs` would reach Node.js as is.
import { spawnSync } from 'node:child_process';
import { readdirSync } from 'node:fs';
import { join } from 'node:path';

const [directory, ...options] = process.argv.slice(2);
const files = readdirSync(directory)
  .filter((name) => name.endsWith('.test.mjs'))
  .sort()
  .map((name) => join(directory, name));
const run = spawnSync(process.execPath, ['--test', ...options, ...files], { stdio: 'inherit' });
if (run.error) throw run.error;
process.exitCode = run.status ?? 1;
