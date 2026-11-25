#!/usr/bin/env node
// Write-latency benchmark for `clink --transactions` (issue #195).
//
// Usage:
//   node .github/scripts/transactions-benchmark.mjs [--warmup N] [--gap N]
//     [--window N] [--max-ratio R] [--slack-ms MS] -- <clink command...>
//
// Runs `clink --db <tmp>/data.links --transactions --retention sized:64` on
// a constant-size store: every write swaps the target of link 1 between 1
// and 2, so the live store never grows and only the history does. It times
// a window of writes starting at write N (after the warmup) and the same
// window starting at write N + gap, and fails when the later median exceeds
// max-ratio times the earlier one plus slack-ms. Before #195 each write
// loaded and rewrote the whole transitions store, so write 40 already took
// several times as long as write 3.
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readdirSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const DEFAULTS = Object.freeze({
  warmup: 10,
  gap: 100,
  window: 15,
  maxRatio: 2,
  slackMs: 50,
});

/** Parses the options before `--` and the clink command after it. */
export function parseArgs(argv) {
  const separator = argv.indexOf('--');
  if (separator < 0 || separator === argv.length - 1) {
    throw new Error('usage: transactions-benchmark.mjs [options] -- <clink command...>');
  }
  const options = { ...DEFAULTS };
  const names = {
    '--warmup': 'warmup',
    '--gap': 'gap',
    '--window': 'window',
    '--max-ratio': 'maxRatio',
    '--slack-ms': 'slackMs',
  };
  const flags = argv.slice(0, separator);
  for (let index = 0; index < flags.length; index += 2) {
    const name = names[flags[index]];
    const value = Number(flags[index + 1]);
    if (!name || !Number.isFinite(value) || value < 0) {
      throw new Error(`invalid option ${flags[index]} ${flags[index + 1] ?? ''}`.trim());
    }
    options[name] = value;
  }
  if (options.window < 1 || options.maxRatio < 1)
    throw new Error('--window and --max-ratio must be at least 1');
  return { options, command: argv.slice(separator + 1) };
}

/** The query of write `index` (1-based): create links 1 and 2, then swap the target of link 1. */
export function queryFor(index) {
  if (index === 1) return '() ((1: 1 1) (2: 2 2))';
  return index % 2 === 0 ? '((1: 1 1)) ((1: 1 2))' : '((1: 1 2)) ((1: 1 1))';
}

export function median(values) {
  if (values.length === 0) throw new Error('median of no values');
  const sorted = [...values].sort((a, b) => a - b);
  const middle = sorted.length >> 1;
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
}

/** Compares the two windows; `ok` is false when later writes slowed down. */
export function evaluate(early, late, { maxRatio, slackMs }) {
  const before = median(early);
  const after = median(late);
  const limit = before * maxRatio + slackMs;
  return { before, after, limit, ok: after <= limit };
}

/** Runs the benchmark with `run(args)` performing one clink call and returning its duration in ms. */
export function benchmark(options, run) {
  const first = options.warmup + 1;
  const second = first + options.gap;
  const last = second + options.window - 1;
  const early = [];
  const late = [];
  for (let index = 1; index <= last; index++) {
    const elapsed = run(queryFor(index), index);
    if (index >= first && index < first + options.window) early.push(elapsed);
    if (index >= second) late.push(elapsed);
  }
  return { first, second, last, early, late, ...evaluate(early, late, options) };
}

/** Times one `clink` call against the database in `directory`. */
export function clinkRunner(command, directory) {
  const database = join(directory, 'data.links');
  return (query, index) => {
    const args = [
      ...command.slice(1),
      '--db',
      database,
      '--transactions',
      '--retention',
      'sized:64',
      query,
    ];
    const started = process.hrtime.bigint();
    const result = spawnSync(command[0], args, { encoding: 'utf8' });
    const elapsed = Number(process.hrtime.bigint() - started) / 1e6;
    if (result.error) throw result.error;
    if (result.status !== 0) {
      throw new Error(`write ${index} failed (exit ${result.status}): ${result.stderr}`);
    }
    return elapsed;
  };
}

function sizes(directory) {
  return readdirSync(directory)
    .sort()
    .map((name) => `${name}=${statSync(join(directory, name)).size}`)
    .join(' ');
}

export function main(argv, log = console.log) {
  const { options, command } = parseArgs(argv);
  const directory = mkdtempSync(join(tmpdir(), 'transactions-benchmark-'));
  try {
    const result = benchmark(options, clinkRunner(command, directory));
    const ms = (value) => `${value.toFixed(1)} ms`;
    log(
      `writes ${result.first}..${result.first + options.window - 1}: median ${ms(result.before)}`,
    );
    log(
      `writes ${result.second}..${result.last}: median ${ms(result.after)} (limit ${ms(result.limit)})`,
    );
    log(`files after ${result.last} writes: ${sizes(directory)}`);
    if (!result.ok) {
      log(
        `::error::clink --transactions slowed down: write ${result.second} is not within ${options.maxRatio}x + ${options.slackMs} ms of write ${result.first} (issue #195)`,
      );
      return 1;
    }
    return 0;
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exitCode = main(process.argv.slice(2));
}
