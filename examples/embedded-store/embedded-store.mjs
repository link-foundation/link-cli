// Embedding @link-foundation/link-cli-server as a transactional store, the
// JavaScript counterpart of rust/examples/embedded_store.rs (issue #98).
//
//   node examples/embedded-store/embedded-store.mjs [directory]
//   bun examples/embedded-store/embedded-store.mjs [directory]
//   deno run --allow-all --node-modules-dir=manual examples/embedded-store/embedded-store.mjs [directory]
//
// Run `npm ci` in js/server first: the library imports its npm dependencies
// from js/server/node_modules.
//
// It walks through the four properties that make the library reusable, on a
// database file that is byte for byte the one the Rust example writes:
//
// 1. the store is a file-backed doublets unit store (`DoubletsStorage`, the
//    port of `DoubletsStorage<usize, FileMappedUnitStore<usize>>`);
// 2. writes go through the transactions layer, so an uncommitted
//    transaction is rolled back;
// 3. the database file is mutated **in place**, so a mapping another
//    process is holding never goes stale, and committed writes survive a
//    crash without an explicit `save`;
// 4. an exclusive lock keeps a second writer out, and `storageRevision`
//    tells a reader whether anyone else has written.
import assert from 'node:assert/strict';
import { mkdirSync, rmSync, statSync } from 'node:fs';
import { join } from 'node:path';

import {
  CommitMode,
  DoubletsStorage,
  FileTransitionLog,
  INFINITE_RETENTION,
  TransactionsDecorator,
  sameRevision,
  storageRevision,
} from '../../js/server/src/index.js';

/** The transactions layer over a locked store, as `open` of the Rust example. */
function open(database, log) {
  // `openExclusive` takes the advisory write lock on `<database>.lock` and
  // holds it until the storage is closed.
  const store = DoubletsStorage.openExclusive(database);
  const transactions = new TransactionsDecorator(
    store,
    FileTransitionLog.open(log),
    INFINITE_RETENTION,
    CommitMode.Sync,
    false,
  );
  return { store, transactions };
}

/** What dropping the Rust handles does: the lock and the log are let go, nothing is rolled back. */
function drop({ store, transactions }) {
  transactions.logStore().close();
  store.close();
}

/**
 * Identity of the file behind `path`. A store that is rebuilt through a
 * temporary file and `rename` changes it; an in-place one never does. Only
 * Unix has inodes, so elsewhere the check is skipped.
 */
const fileIdentity = (path) => (process.platform === 'win32' ? null : statSync(path).ino);

const directory = process.argv[2] ?? '/tmp/clink-embedded-store-js';
rmSync(directory, { recursive: true, force: true });
mkdirSync(directory, { recursive: true });
const database = join(directory, 'db.links');
const log = join(directory, 'db.transitions.log');

// -- 1. Commit one write, then abandon another ---------------------------
const first = open(database, log);
first.transactions.beginTransaction();
const committed = first.transactions.create(0, 0);
first.transactions.commit();
console.log(`committed link ${committed}`);
const firstIdentity = fileIdentity(database);

// Letting the handle go without `commit` is what a crash looks like to the
// next process that opens the store.
first.transactions.beginTransaction();
const abandoned = first.transactions.create(0, 0);
console.log(`wrote link ${abandoned} inside a transaction that never commits`);

// While this storage is open, nobody else can take the write lock.
assert.equal(DoubletsStorage.tryOpenExclusive(database), null);
console.log('a second writer is locked out while the first one is open');
drop(first);

// -- 2. Reopen: recovery runs in the constructor -------------------------
const second = open(database, log);
assert.ok(second.transactions.exists(committed), 'committed write must survive');
assert.ok(!second.transactions.exists(abandoned), 'uncommitted write must be rolled back');
console.log(`after recovery: ${committed} is present, ${abandoned} is gone`);
assert.equal(
  fileIdentity(database),
  firstIdentity,
  'the database must be mutated in place, never replaced',
);
console.log(
  firstIdentity === null
    ? "file identity unchanged: another process's mapping stays valid"
    : `inode unchanged (${firstIdentity}): another process's mapping stays valid`,
);
const revision = storageRevision(database);
drop(second);

// -- 3. Notice a write by somebody else ----------------------------------
const third = open(database, log);
third.transactions.beginTransaction();
const extra = third.transactions.create(committed, committed);
third.transactions.commit();
third.transactions.flush();
console.log(`another holder committed link ${extra}`);
drop(third);
console.log(`revision changed: ${!sameRevision(storageRevision(database), revision)}`);

// -- 4. Read the store back through the storage methods -------------------
const reader = DoubletsStorage.openShared(database);
console.log(`links in the store: ${reader.linksCount()}`);
reader.close();
