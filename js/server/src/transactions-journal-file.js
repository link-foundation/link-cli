// The append-only journal file `clink` keeps next to its database (issue
// #195), and the one-time import of the older links-backed journal:
// rust/src/transactions/journal_file.rs.
//
// Older versions stored each journal entry as the name of a fresh link in
// `<db>.transitions.links` (plus `<db>.transitions.names.links`). Every
// invocation re-read that whole store and rewrote it on save, so each write
// got slower as the history grew. The journal is now a plain text file,
// `<db>.transitions.log`, that a write appends to.
//
// Opening the journal imports a legacy store once: its entries are written to
// a temporary file that is synced and renamed into place before the legacy
// files are removed, so a crash leaves either the legacy store or a complete
// journal. An explicit `--transactions-file` that still holds a links store
// is converted in place the same way.
import {
  closeSync,
  existsSync,
  fsyncSync,
  readSync,
  renameSync,
  rmSync,
  writeFileSync,
} from 'node:fs';

import { makeNamesDatabaseFilename, NamedTypesDecorator } from './named-types.js';
import { openFileSync } from './open-file.js';
import { FileTransitionLog } from './transactions-log.js';
import { io, NamedTransitionLog } from './transactions-storage.js';
import { makeLegacyTransitionsFilename, makeTransitionsLogFilename } from './transactions-types.js';

/** Every journal entry starts with this, so a journal file does too. */
const ENTRY_PREFIX = Buffer.from('__transactions:');

/**
 * Whether `path` holds something other than a text journal: a non-empty
 * file whose first bytes are not (a prefix of) the journal entry prefix.
 */
export function isLinksJournal(path) {
  if (!existsSync(path)) return false;
  const head = Buffer.alloc(ENTRY_PREFIX.length);
  const length = io(() => {
    const fd = openFileSync(path, 'r');
    try {
      return readSync(fd, head, 0, head.length, 0);
    } finally {
      closeSync(fd);
    }
  });
  return length > 0 && !ENTRY_PREFIX.subarray(0, length).equals(head.subarray(0, length));
}

/** Removes `path` when it exists. */
const removeIfExists = (path) => io(() => rmSync(path, { force: true }));

/**
 * Copies every entry of the links-backed journal at `legacy` into the text
 * journal at `target`, then removes the legacy store. Returns whether
 * anything was imported.
 *
 * When `target` already exists (a crash after publishing it), the legacy
 * store is removed only if each of its entries is already in `target`.
 */
export function importLinksJournal(
  legacy,
  target,
  { trace = false, stderr = process.stderr } = {},
) {
  const names = makeNamesDatabaseFilename(legacy);
  const store = NamedTypesDecorator.open(legacy, { namesDatabasePath: names });
  const entries = new NamedTransitionLog(store.withIndexedLookups()).readLogEntries();
  const inPlace = legacy === target;
  if (!inPlace && existsSync(target)) {
    const existing = FileTransitionLog.open(target);
    let present;
    try {
      present = new Set(existing.readLogEntries());
    } finally {
      existing.close();
    }
    if (entries.every((entry) => present.has(entry))) {
      removeIfExists(legacy);
      removeIfExists(names);
    } else if (trace) {
      stderr.write(`[Transactions] Kept legacy journal ${legacy}: ${target} already exists.\n`);
    }
    return false;
  }
  const temporary = `${target}.import`;
  io(() => {
    const fd = openFileSync(temporary, 'w');
    try {
      writeFileSync(fd, entries.map((entry) => `${entry}\n`).join(''));
      fsyncSync(fd);
    } finally {
      closeSync(fd);
    }
    renameSync(temporary, target);
  });
  if (!inPlace) removeIfExists(legacy);
  removeIfExists(names);
  FileTransitionLog.open(target).close(); // syncs the directory entries
  if (trace) {
    stderr.write(
      `[Transactions] Imported ${entries.length} entries from legacy journal ${legacy} into ${target}.\n`,
    );
  }
  return true;
}

/**
 * `open_transitions_journal`: opens the journal of `database` (`explicit`
 * when given, the conventional `<stem>.transitions.log` otherwise),
 * importing a legacy links-backed journal first.
 */
export function openTransitionsJournal(database, explicit, options = {}) {
  let path;
  if (explicit != null) {
    if (isLinksJournal(explicit)) importLinksJournal(explicit, explicit, options);
    path = explicit;
  } else {
    path = makeTransitionsLogFilename(database);
    const legacy = makeLegacyTransitionsFilename(database);
    if (existsSync(legacy)) importLinksJournal(legacy, path, options);
  }
  return FileTransitionLog.open(path);
}

export { makeLegacyTransitionsFilename, makeTransitionsLogFilename };
