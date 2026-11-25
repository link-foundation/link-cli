// The store lock (issue #194) across implementations: the Rust `clink`
// (CLINK_RUST) and bin/clink.js take the same `<db>.lock` and
// `<db>.lock.d`, so writers of both keep every write on one store, and a
// store one of them holds stops the other, which names the holder.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, it } from 'node:test';

import { scratchDirectory } from '../helpers/storage.mjs';
import { JS, RUST, skip } from './harness.mjs';

const IMPLEMENTATIONS = {
  rust: { command: RUST, prefix: [] },
  js: { command: process.execPath, prefix: [JS] },
};

/** Runs `clink` of `implementation` and resolves with its status and output. */
function run(implementation, args) {
  const { command, prefix } = IMPLEMENTATIONS[implementation];
  return new Promise((resolve, reject) => {
    const child = spawn(command, [...prefix, ...args], { stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => (stdout += chunk));
    child.stderr.on('data', (chunk) => (stderr += chunk));
    child.on('error', reject);
    child.on('close', (status) => resolve({ status, stdout, stderr }));
  });
}

/**
 * Starts `clink --serve stdio` of `implementation`, which holds the store
 * lock while it serves, and resolves once it listens. `stop()` closes its
 * standard input and waits for it to exit.
 */
function serve(implementation, args) {
  const { command, prefix } = IMPLEMENTATIONS[implementation];
  const child = spawn(command, [...prefix, ...args, '--serve', 'stdio'], {
    stdio: ['pipe', 'ignore', 'pipe'],
  });
  const exited = new Promise((resolve) => child.on('close', resolve));
  return new Promise((resolve, reject) => {
    let stderr = '';
    child.on('error', reject);
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
      if (stderr.includes('listening on stdio')) {
        resolve({
          stop: () => {
            child.stdin.end();
            return exited;
          },
        });
      }
    });
    exited.then((status) => reject(new Error(`clink --serve exited with ${status}: ${stderr}`)));
  });
}

const PROGRAM = { rust: 'program=clink (rust) ', js: 'program=clink (js) ' };

describe('store lock across the Rust and JavaScript clink', { skip }, () => {
  for (const kind of ['flock', 'pid']) {
    it(`keeps every write of concurrent Rust and JavaScript writers (--lock ${kind})`, async () => {
      const directory = scratchDirectory();
      const db = join(directory, 's.links');
      const writers = 12;
      const runs = [];
      for (let index = 1; index <= writers; index++) {
        const input = join(directory, `in${index}.lino`);
        writeFileSync(input, `(w${index}: w${index} w${index})\n`);
        const implementation = index % 2 === 0 ? 'rust' : 'js';
        runs.push(run(implementation, ['--db', db, '--lock', kind, '--in', input]));
      }
      for (const result of await Promise.all(runs)) {
        assert.equal(result.status, 0, result.stderr);
      }
      const exports = [];
      for (const implementation of ['rust', 'js']) {
        const output = join(directory, `${implementation}.lino`);
        const result = await run(implementation, ['--db', db, '--lock', kind, '--out', output]);
        assert.equal(result.status, 0, result.stderr);
        exports.push(readFileSync(output, 'utf8'));
      }
      assert.equal(exports[0], exports[1]);
      for (let index = 1; index <= writers; index++) {
        assert.ok(exports[0].includes(`(w${index}: w${index} w${index})`), exports[0]);
      }
    });

    for (const [holder, waiter] of [
      ['rust', 'js'],
      ['js', 'rust'],
    ]) {
      it(`stops a ${waiter} writer on a store the ${holder} clink holds (--lock ${kind})`, async () => {
        const db = join(scratchDirectory(), 's.links');
        const server = await serve(holder, ['--db', db, '--lock', kind]);
        try {
          const blocked = await run(waiter, ['--db', db, '--lock', kind, '--lock-timeout', '50', '() ((1 1))']);
          assert.equal(blocked.status, 1);
          assert.match(blocked.stderr, /^Error: Lock error: store is locked: /);
          assert.ok(blocked.stderr.includes(PROGRAM[holder]), blocked.stderr);
          assert.ok(
            blocked.stderr.includes(`gave up waiting for the ${kind === 'pid' ? 'pid' : 'exclusive flock'} lock after 50 ms`),
            blocked.stderr,
          );
          const status = await run(waiter, ['--db', db, '--lock-status']);
          assert.equal(status.status, 0, status.stderr);
          assert.ok(status.stdout.includes(PROGRAM[holder]), status.stdout);
        } finally {
          await server.stop();
        }
        const after = await run(waiter, ['--db', db, '--lock', kind, '--lock-timeout', '0', '() ((1 1))']);
        assert.equal(after.status, 0, after.stderr);
      });
    }
  }
});
