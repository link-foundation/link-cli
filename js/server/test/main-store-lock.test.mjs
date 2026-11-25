// `main` (src/main.js) locks the stores of every invocation (issue #194): which
// stores and in which mode, `--lock-status`, the lock options checked before
// any store opens, and the reproduction of the issue — concurrent `clink --in`
// processes, every one that exits 0 keeping its write. The Rust counterpart is
// rust/tests/cli_store_lock_tests.rs.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

import { Cli } from '../src/cli.js';
import { lockedStores, lockMode, main } from '../src/main.js';
import { lockBackend, lockFilePath, LockMode } from '../src/storage-lock.js';
import { lockDirPath, LockOwner } from '../src/store-lock.js';
import { scratchDirectory } from './helpers/storage.mjs';

const BIN = fileURLToPath(new URL('../bin/clink.js', import.meta.url));
// Bun and Deno run this file as well; the processes are Node.js ones.
const NOT_NODE =
  (process.versions.bun !== undefined || globalThis.Deno !== undefined) &&
  'spawns bin/clink.js with Node.js';

/** A stream that keeps what is written to it. */
function sink() {
  const chunks = [];
  return {
    write: (text) => chunks.push(String(text)),
    text: () => chunks.join(''),
  };
}

/** Runs `main(args)` in-process: `{ status, out, err }`. */
async function clink(args, cwd) {
  const stdout = sink();
  const stderr = sink();
  const status = await main(args, { stdout, stderr, env: {}, cwd });
  return { status, out: stdout.text(), err: stderr.text() };
}

const parsed = (...args) => Cli.parse(args).cli;

test('locks the database and every side store it was given, once', () => {
  assert.deepEqual(lockedStores(parsed('--db', 'a.links')), ['a.links']);
  assert.deepEqual(
    lockedStores(
      parsed(
        '--db',
        'a.links',
        '--transactions-file',
        't.links',
        '--vc-file',
        'v.links',
        '--triggers-file',
        'g.links',
        '--tokens',
        ' tokens.links ',
        '--permissions',
        'p.links',
      ),
    ),
    ['a.links', 't.links', 'v.links', 'g.links', 'tokens.links', 'p.links'],
  );
  // Tokens and permissions kept in the database itself add no store.
  assert.deepEqual(lockedStores(parsed('--tokens', ' self ', '--permissions', ' ')), ['db.links']);
});

test('shares the lock among read-only runs of an existing store only', () => {
  const db = join(scratchDirectory(), 's.links');
  // Opening a store that does not exist yet creates it.
  assert.equal(lockMode(parsed('--db', db, '--after')), LockMode.Exclusive);
  writeFileSync(db, '');
  for (const args of [[], ['--out', 'x.lino'], ['--after'], ['--structure', '1'], ['']]) {
    assert.equal(lockMode(parsed('--db', db, ...args)), LockMode.Shared, args.join(' '));
  }
  for (const args of [
    ['() ((1 1))'],
    ['--query', '() ((1 1))'],
    ['--in', 'x.lino'],
    ['--binary-in', 'x.bin'],
    ['--always', '() ()'],
    ['--serve', 'tcp://127.0.0.1:0'],
    ['--grant-token', 'read'],
    ['--revoke-token', 'abc'],
    ['--transactions'],
    ['--log'],
  ]) {
    assert.equal(lockMode(parsed('--db', db, ...args)), LockMode.Exclusive, args.join(' '));
  }
});

test('--lock-status reports every store without creating any', async () => {
  const dir = scratchDirectory();
  const db = join(dir, 's.links');
  const log = join(dir, 'log.links');
  writeFileSync(join(dir, 'held'), '');
  const result = await clink(['--db', db, '--transactions-file', log, '--lock-status'], dir);
  assert.deepEqual(result, {
    status: 0,
    out:
      `flock ${lockFilePath(log)}: unlocked\npid ${lockDirPath(log)}: unlocked\n` +
      `flock ${lockFilePath(db)}: unlocked\npid ${lockDirPath(db)}: unlocked\n`,
    err: '',
  });
  assert.deepEqual(readdirSync(dir), ['held']);
});

test('checks the lock options before any store opens', async () => {
  const dir = scratchDirectory();
  const db = join(dir, 's.links');
  assert.deepEqual(await clink(['--db', db, '--lock', 'mutex', '() ((1 1))'], dir), {
    status: 1,
    out: '',
    err: "Error: Invalid link format: Invalid --lock value 'mutex'. Use 'flock', 'pid' or 'none'.\n",
  });
  const late = await clink(['--db', db, '--lock-timeout', 'soon'], dir);
  assert.equal(late.status, 1);
  assert.match(late.err, /Invalid --lock-timeout value 'soon'/);
  assert.deepEqual(readdirSync(dir), []);
});

test('fails naming the owner of a held pid lock, and --no-lock passes it by', async () => {
  const dir = scratchDirectory();
  const db = join(dir, 's.links');
  const lock = lockDirPath(db);
  mkdirSync(lock);
  writeFileSync(join(lock, 'owner'), LockOwner.current('held-by-test').toRecord());
  const held = await clink(
    ['--db', db, '--lock', 'pid', '--lock-timeout', '20', '() ((1 1))'],
    dir,
  );
  assert.equal(held.status, 1);
  assert.match(
    held.err,
    new RegExp(
      `^Error: Lock error: store is locked: .*s\\.links\\.lock\\.d is held by pid=${process.pid} `,
    ),
  );
  assert.equal((await clink(['--db', db, '--no-lock', '() ((1 1))'], dir)).status, 0);
  const after = await clink(['--db', db, '--lock', 'none', '--after'], dir);
  assert.equal(after.out, '(1: 1 1)\n');
});

/** Runs `bin/clink.js` with `args`; resolves its exit status and standard error. */
function spawnClink(args) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [BIN, ...args], {
      stdio: ['ignore', 'ignore', 'pipe'],
    });
    let stderr = '';
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
    });
    child.on('error', reject);
    child.on('close', (status) => resolve({ status, stderr }));
  });
}

const KINDS = [
  ['the default lock', []],
  ['--lock pid', ['--lock', 'pid']],
  ['--lock flock', ['--lock', 'flock'], lockBackend().unavailable],
];

for (const [name, lockArgs, unavailable] of KINDS) {
  test(
    `concurrent --in imports under ${name} all keep their writes (#194)`,
    { skip: NOT_NODE || unavailable, timeout: 60_000 },
    async () => {
      const dir = scratchDirectory();
      const db = join(dir, 's.links');
      const writers = Array.from({ length: 8 }, (_, index) => {
        const input = join(dir, `in${index + 1}.lino`);
        writeFileSync(input, `(w${index + 1}: w${index + 1} w${index + 1})\n`);
        return spawnClink(['--db', db, ...lockArgs, '--in', input]);
      });
      const results = await Promise.all(writers);
      for (const result of results) assert.equal(result.status, 0, result.stderr);
      const out = join(dir, 'out.lino');
      assert.equal((await clink(['--db', db, '--out', out], dir)).status, 0);
      const exported = readFileSync(out, 'utf8');
      for (let index = 1; index <= results.length; index++) {
        assert.ok(
          exported.includes(`(w${index}: w${index} w${index})`),
          `lost w${index}: ${exported}`,
        );
      }
      const leftovers = readdirSync(dir).filter(
        (entry) =>
          entry.endsWith('.saving') || entry.endsWith('.owner') || entry.includes('.lock.d'),
      );
      assert.deepEqual(leftovers, []);
    },
  );
}
