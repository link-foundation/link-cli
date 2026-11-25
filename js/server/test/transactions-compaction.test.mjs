// Issue #195: the append-only journal stays bounded under a retention policy
// (rust/src/transactions/compaction.rs and the compaction of
// `FileTransitionLog` in rust/src/transactions/log.rs), and `clink` imports
// the older links-backed journal once (rust/src/transactions/journal_file.rs).
import assert from 'node:assert/strict';
import { chmodSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, it } from 'node:test';
import { InvalidFormatError } from '../src/errors.js';
import { NamedTypesDecorator } from '../src/named-types.js';
import { compactedJournal, compactJournalIfNeeded } from '../src/transactions-compaction.js';
import {
  importLinksJournal,
  isLinksJournal,
  makeLegacyTransitionsFilename,
  makeTransitionsLogFilename,
  openTransitionsJournal,
} from '../src/transactions-journal-file.js';
import { compactionPath, FileTransitionLog } from '../src/transactions-log.js';
import { parseJournal } from '../src/transactions-recovery.js';
import { NamedTransitionLog } from '../src/transactions-storage.js';
import { parseRetentionPolicy } from '../src/transactions-types.js';
import { memoryLog, openTx } from './helpers/transactions.mjs';
import { NO_POSIX_PERMISSIONS, scratchDirectory } from './helpers/storage.mjs';

const sized = (n) => parseRetentionPolicy(`sized:${n}`);

/** Entries of the journal file at `path`. */
const linesOf = (path) => readFileSync(path, 'utf8').split('\n').slice(0, -1);

/** A capturing stderr. */
const capture = () => ({
  text: '',
  write(chunk) {
    this.text += chunk;
  },
});

describe('FileTransitionLog compaction', () => {
  it('replaces the journal atomically and keeps appending', () => {
    const path = join(scratchDirectory(), 'journal.log');
    const log = FileTransitionLog.open(path);
    try {
      assert.equal(log.supportsCompaction(), true);
      for (const entry of ['a', 'b', 'c']) log.appendLogEntry(entry);
      log.compactLogEntries(['b']);
      assert.equal(existsSync(compactionPath(path)), false);
      assert.deepEqual(log.readLogEntries(), ['b']);
      log.appendLogEntry('d');
      assert.equal(readFileSync(path, 'utf8'), 'b\nd\n');
      assert.equal(log.replaceLogEntries(['b', 'd']), false);
      assert.equal(log.replaceLogEntries(['b']), true);
      assert.equal(log.replaceLogEntries(['e', 'f']), true);
      assert.deepEqual(log.readLogEntries(), ['e', 'f']);
      log.compactLogEntries([]);
      assert.equal(readFileSync(path, 'utf8'), '');
    } finally {
      log.close();
    }
  });

  it('refuses empty and multi-line entries without touching the log', () => {
    const path = join(scratchDirectory(), 'journal.log');
    const log = FileTransitionLog.open(path);
    try {
      log.appendLogEntry('kept');
      for (const bad of ['', 'a\nb', 'a\rb', 'a\0b']) {
        assert.throws(() => log.compactLogEntries(['x', bad]), InvalidFormatError);
      }
      assert.deepEqual(log.readLogEntries(), ['kept']);
    } finally {
      log.close();
    }
  });

  it('discards a stale compaction copy on open', () => {
    const path = join(scratchDirectory(), 'journal.log');
    writeFileSync(path, 'complete\n');
    writeFileSync(compactionPath(path), 'partial');
    const log = FileTransitionLog.open(path);
    log.close();
    assert.equal(existsSync(compactionPath(path)), false);
    assert.equal(readFileSync(path, 'utf8'), 'complete\n');
  });

  it('keeps the old journal when the copy cannot be written', { skip: NO_POSIX_PERMISSIONS }, () => {
    const directory = join(scratchDirectory(), 'locked');
    const path = join(directory, 'journal.log');
    const log = FileTransitionLog.open(path);
    try {
      log.appendLogEntry('kept');
      chmodSync(directory, 0o500);
      try {
        assert.throws(
          () => log.compactLogEntries(['new']),
          (error) => error.message === 'I/O error: Permission denied (os error 13)',
        );
      } finally {
        chmodSync(directory, 0o700);
      }
      assert.equal(log.tailMayBeTorn, false);
      log.appendLogEntry('appended');
      assert.deepEqual(log.readLogEntries(), ['kept', 'appended']);
    } finally {
      log.close();
    }
  });

  it(
    'reopens its writer after a compaction whose directory sync failed',
    { skip: NO_POSIX_PERMISSIONS },
    () => {
      const directory = join(scratchDirectory(), 'unreadable');
      const path = join(directory, 'journal.log');
      const log = FileTransitionLog.open(path);
      log.appendLogEntry('old');
      chmodSync(directory, 0o300);
      try {
        assert.throws(
          () => log.compactLogEntries(['new']),
          (error) => error.message === 'I/O error: Permission denied (os error 13)',
        );
      } finally {
        chmodSync(directory, 0o700);
      }
      // The rename was published; the writer is released until repair.
      assert.equal(log.fd, null);
      assert.equal(log.tailMayBeTorn, true);
      log.appendLogEntry('next');
      assert.deepEqual(log.readLogEntries(), ['new', 'next']);
      log.compactLogEntries(['next']);
      chmodSync(directory, 0o300);
      try {
        assert.throws(() => log.compactLogEntries(['last']));
      } finally {
        chmodSync(directory, 0o700);
      }
      // A log whose writer is released still closes cleanly.
      log.close();
      assert.deepEqual(linesOf(path), ['last']);
    },
  );
});

describe('journal compaction', () => {
  it('parses the sequence watermark and remembers the newest decision', () => {
    const owner = '0'.repeat(31);
    const journal = parseJournal([
      '__transactions:sequence:41',
      `__transactions:commit:${owner}1`,
      `__transactions:rollback:${owner}2`,
      'unrelated',
    ]);
    assert.equal(journal.sequence, 41n);
    assert.equal(journal.lastDecision, `__transactions:rollback:${owner}2`);
    assert.equal(journal.entryCount, 4);
    assert.equal(parseJournal(['__transactions:sequence:1', 'x:9']).sequence, 1n);
    for (const bad of ['0', '-3', 'x', '']) {
      assert.throws(
        () => parseJournal([`__transactions:sequence:${bad}`]),
        (error) => error instanceof InvalidFormatError && /invalid sequence marker$/.test(error.message),
      );
    }
  });

  it('keeps the watermark, retained records, decisions and applied markers', () => {
    const log = memoryLog();
    const tx = openTx({ log, retention: sized(2) });
    assert.deepEqual(compactedJournal(tx), []);
    for (let i = 1; i <= 4; i++) tx.create(i, i);
    for (const n of [8, 9]) {
      const rolled = tx.beginTransaction();
      tx.create(n, n);
      tx.rollback(rolled);
    }
    const entries = compactedJournal(tx);
    assert.deepEqual(
      tx.records.map((record) => record.sequence),
      [5n, 6n],
    );
    assert.equal(entries[0], '__transactions:sequence:6');
    // The highest applied record (4) left the window, so its marker stays.
    assert.equal(entries[1], '__transactions:applied:4');
    assert.equal(entries.filter((entry) => entry.startsWith('__transactions:transition:')).length, 2);
    assert.equal(entries.at(-1), tx.lastDecision);
    assert.ok(!entries.some((entry) => entry.startsWith('__transactions:commit:')));
    // Reopening the compacted journal restores the same state.
    const reopened = openTx({ log: memoryLog(entries), retention: sized(2) });
    assert.equal(reopened.lastLoggedSequence(), tx.lastLoggedSequence());
    assert.equal(reopened.appliedSequence(), tx.appliedSequence());
    assert.deepEqual(reopened.log(), tx.log());
    assert.equal(reopened.lastDecision, tx.lastDecision);
    // A newest decision whose owner left the window is kept as well.
    tx.lastDecision = `__transactions:commit:${'0'.repeat(31)}1`;
    assert.equal(compactedJournal(tx).at(-1), tx.lastDecision);
  });

  it('marks retained applied records and keeps a retained newest decision once', () => {
    const tx = openTx({ retention: sized(3) });
    tx.create(1, 1);
    tx.create(2, 2);
    const both = tx.beginTransaction();
    tx.create(3, 3);
    tx.create(4, 4);
    tx.commit(both);
    const entries = compactedJournal(tx);
    // One decision per owner, however many of its records are retained.
    assert.equal(entries.filter((entry) => entry === tx.lastDecision).length, 1);
    assert.deepEqual(
      entries.filter((entry) => entry.startsWith('__transactions:applied:')),
      ['__transactions:applied:2', '__transactions:applied:3', '__transactions:applied:4'],
    );
  });

  it('leaves a journal with an undecided owner or an open transaction alone', () => {
    const log = memoryLog();
    const tx = openTx({ log, retention: sized(1) });
    const handle = tx.beginTransaction();
    tx.create(1, 1);
    assert.equal(compactedJournal(tx), null);
    tx.journalEntries = 10_000;
    compactJournalIfNeeded(tx);
    tx.commit(handle);
    // An undecided owner left by a direct record change is never compacted.
    tx.records.push({ ...tx.records[0], transactionId: 7n, sequence: 99n });
    tx.retention = sized(1);
    const fileLog = FileTransitionLog.open(join(scratchDirectory(), 'journal.log'));
    try {
      tx.transitionLog = fileLog;
      tx.journalEntries = 10_000;
      compactJournalIfNeeded(tx);
      assert.equal(tx.journalEntries, 10_000);
    } finally {
      fileLog.close();
    }
  });

  it('runs only for a bounded policy over a compactable log', () => {
    const calls = [];
    const log = {
      ...memoryLog(),
      supportsCompaction: () => false,
      compactLogEntries: (entries) => calls.push(entries),
    };
    const tx = openTx({ log, retention: sized(1) });
    tx.create(1, 1);
    tx.journalEntries = 10_000;
    compactJournalIfNeeded(tx);
    log.supportsCompaction = () => true;
    tx.retention = parseRetentionPolicy('infinite');
    compactJournalIfNeeded(tx);
    assert.equal(calls.length, 0);
    tx.retention = sized(1);
    tx.journalEntries = 10;
    compactJournalIfNeeded(tx);
    assert.equal(calls.length, 0);
    tx.journalEntries = 10_000;
    compactJournalIfNeeded(tx);
    assert.equal(calls.length, 1);
    assert.equal(tx.journalEntryCount(), calls[0].length);
  });

  it('bounds the journal file under sized and chunked retention (issue #195)', () => {
    for (const spec of ['sized:8', `chunked:8:${join(scratchDirectory(), 'archive')}`]) {
      const path = join(scratchDirectory(), 'journal.log');
      const stderr = capture();
      let log = FileTransitionLog.open(path);
      let tx = openTx({ log, retention: parseRetentionPolicy(spec), trace: true, stderr });
      let largest = 0;
      for (let i = 1; i <= 200; i++) {
        tx.create(i, i);
        largest = Math.max(largest, linesOf(path).length);
      }
      assert.ok(largest <= 2 * (3 * 8 + 3) + 256 + 3, `${spec}: ${largest} entries`);
      assert.match(stderr.text, /Compacted journal from \d+ to \d+ entries\./);
      const sequence = tx.lastLoggedSequence();
      const view = tx.log();
      log.close();
      log = FileTransitionLog.open(path);
      tx = openTx({ log, retention: parseRetentionPolicy(spec) });
      assert.equal(tx.lastLoggedSequence(), sequence);
      // Retention trims after the next decision; the window is intact.
      const reopened = tx.log();
      assert.deepEqual(reopened.slice(reopened.length - view.length), view);
      tx.create(1, 1);
      assert.equal(tx.lastLoggedSequence(), sequence + 1n);
      log.close();
    }
  });

  it('keeps the whole journal under infinite retention', () => {
    const path = join(scratchDirectory(), 'journal.log');
    const log = FileTransitionLog.open(path);
    try {
      const tx = openTx({ log });
      for (let i = 1; i <= 300; i++) tx.create(i, i);
      assert.equal(tx.journalEntryCount(), linesOf(path).length);
      assert.equal(tx.journal().length, 300);
    } finally {
      log.close();
    }
  });
});

/** A links-backed journal at `path` holding `entries`, as older versions wrote. */
function legacyJournal(path, entries) {
  const store = NamedTypesDecorator.open(path);
  const log = new NamedTransitionLog(store);
  for (const entry of entries) log.appendLogEntry(entry);
  store.save();
}

describe('journal file', () => {
  it('names the journal beside the database', () => {
    assert.equal(makeTransitionsLogFilename('/d/db.links'), '/d/db.transitions.log');
    assert.equal(makeLegacyTransitionsFilename('/d/db.links'), '/d/db.transitions.links');
  });

  it('tells a links store from a text journal', () => {
    const directory = scratchDirectory();
    const at = (name, contents) => {
      const path = join(directory, name);
      if (contents !== null) writeFileSync(path, contents);
      return path;
    };
    assert.equal(isLinksJournal(at('missing', null)), false);
    assert.equal(isLinksJournal(at('empty', '')), false);
    assert.equal(isLinksJournal(at('journal', '__transactions:commit:1\n')), false);
    assert.equal(isLinksJournal(at('short', '__trans')), false);
    assert.equal(isLinksJournal(at('links', '(1 0 0)\n')), true);
  });

  it('imports the legacy journal once and removes it', () => {
    const directory = scratchDirectory();
    const db = join(directory, 'db.links');
    const legacy = makeLegacyTransitionsFilename(db);
    legacyJournal(legacy, ['__transactions:sequence:5', 'other']);
    const stderr = capture();
    const log = openTransitionsJournal(db, null, { trace: true, stderr });
    log.close();
    assert.equal(existsSync(legacy), false);
    assert.equal(existsSync(join(directory, 'db.transitions.names.links')), false);
    assert.deepEqual(linesOf(makeTransitionsLogFilename(db)), [
      '__transactions:sequence:5',
      'other',
    ]);
    assert.match(stderr.text, /Imported 2 entries from legacy journal/);
    // Without a legacy store nothing is imported again.
    openTransitionsJournal(db, undefined).close();
    assert.equal(linesOf(makeTransitionsLogFilename(db)).length, 2);
  });

  it('converts an explicit links-backed journal in place', () => {
    const directory = scratchDirectory();
    const explicit = join(directory, 'custom.links');
    const entries = ['__transactions:sequence:3', '__transactions:applied:2'];
    legacyJournal(explicit, entries);
    openTransitionsJournal(join(directory, 'db.links'), explicit).close();
    assert.deepEqual(linesOf(explicit), entries);
    assert.equal(existsSync(join(directory, 'custom.names.links')), false);
    // A text journal at the explicit path is opened as is.
    openTransitionsJournal(join(directory, 'db.links'), explicit).close();
    assert.deepEqual(linesOf(explicit), entries);
  });

  it('removes a legacy store left by a crash only when it was fully imported', () => {
    const directory = scratchDirectory();
    const db = join(directory, 'db.links');
    const legacy = makeLegacyTransitionsFilename(db);
    const target = makeTransitionsLogFilename(db);
    legacyJournal(legacy, ['a', 'b']);
    writeFileSync(target, 'a\n');
    const stderr = capture();
    assert.equal(importLinksJournal(legacy, target, { trace: true, stderr }), false);
    assert.match(stderr.text, /Kept legacy journal .*: .* already exists\./);
    assert.equal(importLinksJournal(legacy, target), false);
    assert.equal(existsSync(legacy), true);
    writeFileSync(target, 'a\nb\nc\n');
    openTransitionsJournal(db, null).close();
    assert.equal(existsSync(legacy), false);
    assert.deepEqual(linesOf(target), ['a', 'b', 'c']);
  });

  it('reports a journal it cannot read', { skip: NO_POSIX_PERMISSIONS }, () => {
    const directory = join(scratchDirectory(), 'sub');
    mkdirSync(directory);
    const path = join(directory, 'journal.log');
    writeFileSync(path, 'x');
    chmodSync(path, 0o000);
    try {
      assert.throws(
        () => isLinksJournal(path),
        (error) => error.message === 'I/O error: Permission denied (os error 13)',
      );
    } finally {
      chmodSync(path, 0o600);
    }
  });
});
