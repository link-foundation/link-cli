// The transactions decorator: rust/src/transactions/contract_tests.rs, the
// shared vectors docs/protocol/transaction-undo-vectors.json (as
// rust/tests/transaction_row_wal_tests.rs replays them) and
// docs/protocol/transaction-name-vectors.json (as
// rust/tests/transaction_name_failure_tests.rs does), and the public API of
// rust/tests/transactions_decorator_tests.rs.
import assert from 'node:assert/strict';
import { readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, it } from 'node:test';
import { LinkStorage } from '../src/link-storage.js';
import {
  CommitMode,
  doublet,
  EMPTY_DOUBLET,
  FileTransitionLog,
  INFINITE_RETENTION,
  journalRecord,
  parseRecord,
  TransactionsDecorator,
  TransitionKind,
} from '../src/transactions.js';
import { capture, scratchDirectory } from './helpers/storage.mjs';
import {
  entriesWith,
  memoryLog,
  namedMemory,
  openTx,
  protocolVectors,
} from './helpers/transactions.mjs';

const transition = (kind, before, after) => ({
  transactionId: 1n,
  sequence: 1n,
  timestampMs: 0n,
  kind,
  before,
  after,
});
const row = (index) => doublet(index, 7, 8);
const rows = (tx) => tx.allLinks().map(({ index, source, target }) => [index, source, target]);

/** Three committed rows whose applied markers were lost, as a crash could leave. */
function committedWithoutMarkers() {
  const tx = openTx();
  for (let i = 0; i < 3; i++) tx.create(10, 20);
  tx.applied.clear();
  return tx;
}

const sized = (maxTransitions) => ({ kind: 'sized', maxTransitions });
const chunked = (chunkSize, archiveDirectory) => ({ kind: 'chunked', chunkSize, archiveDirectory });

describe('contract tests', () => {
  it('sized retention applies a committed head before dropping it', () => {
    const tx = committedWithoutMarkers();
    tx.setRetentionPolicy(sized(1n));
    tx.enforceRetention();
    assert.equal(tx.records.length, 1);
    assert.equal(tx.applied.size, 2);
    assert.equal(tx.all().length, 3);
  });

  it('sized retention keeps a pending head', () => {
    const tx = openTx();
    tx.beginTransaction();
    tx.create(10, 20);
    tx.create(10, 20);
    tx.setRetentionPolicy(sized(1n));
    tx.enforceRetention();
    assert.equal(tx.records.length, 2);
  });

  it('zero sized retention disables trimming', () => {
    const tx = committedWithoutMarkers();
    tx.setRetentionPolicy(sized(0n));
    tx.enforceRetention();
    assert.equal(tx.records.length, 3);
  });

  it('chunked retention applies committed records before archiving', () => {
    const directory = scratchDirectory();
    const tx = committedWithoutMarkers();
    tx.setRetentionPolicy(chunked(2n, directory));
    tx.enforceRetention();
    assert.equal(tx.records.length, 1);
    assert.equal(readdirSync(directory).length, 1);
    assert.equal(tx.all().length, 3);
  });

  it('chunked retention keeps short, disabled and pending chunks', () => {
    const directory = scratchDirectory();
    const tx = openTx();
    tx.beginTransaction();
    tx.create(10, 20);
    tx.create(10, 20);
    for (const chunkSize of [0n, 3n, 2n]) {
      tx.setRetentionPolicy(chunked(chunkSize, directory));
      tx.enforceRetention();
      assert.equal(tx.records.length, 2, `chunk size ${chunkSize}`);
    }
    assert.equal(readdirSync(directory).length, 0);
  });

  it('chunked retention reports an unusable archive path', () => {
    const blocked = join(scratchDirectory(), 'file');
    writeFileSync(blocked, '');
    const tx = committedWithoutMarkers();
    tx.setRetentionPolicy(chunked(1n, blocked));
    assert.throws(() => tx.enforceRetention(), {
      message: `Storage error: failed to create archive dir ${blocked}: File exists (os error 17)`,
    });
  });

  it('replay skips records with no address or no row', () => {
    const tx = openTx();
    for (const value of [
      transition(TransitionKind.Create, EMPTY_DOUBLET, EMPTY_DOUBLET),
      transition(TransitionKind.Update, EMPTY_DOUBLET, EMPTY_DOUBLET),
      transition(TransitionKind.Update, row(9), row(9)),
      transition(TransitionKind.Delete, EMPTY_DOUBLET, EMPTY_DOUBLET),
      transition(TransitionKind.Delete, row(9), EMPTY_DOUBLET),
    ]) {
      tx.applyTransition(value);
    }
    assert.deepEqual(tx.all(), []);
  });

  it('undo skips records with no address or no row', () => {
    const tx = openTx();
    for (const value of [
      transition(TransitionKind.Create, EMPTY_DOUBLET, EMPTY_DOUBLET),
      transition(TransitionKind.Create, EMPTY_DOUBLET, row(9)),
      transition(TransitionKind.Update, EMPTY_DOUBLET, EMPTY_DOUBLET),
      transition(TransitionKind.Update, row(9), row(9)),
      transition(TransitionKind.Delete, EMPTY_DOUBLET, EMPTY_DOUBLET),
    ]) {
      tx.revertTransition(value);
    }
    assert.deepEqual(tx.all(), []);
  });

  it('recording requires an open transaction', () => {
    const tx = openTx();
    const record = journalRecord(transition(TransitionKind.Create, EMPTY_DOUBLET, row(1)));
    assert.throws(() => tx.recordRecords([record], false), {
      message: 'Transaction error: internal: missing open transaction while recording transition',
    });
  });

  it('version replay requires ownership and no open transaction', () => {
    const tx = openTx();
    const record = journalRecord(transition(TransitionKind.Create, EMPTY_DOUBLET, row(1)));
    assert.throws(() => tx.replayVersionTransition(record, false), {
      message: 'Transaction error: Version movement does not own replay.',
    });
    assert.throws(() => tx.flushVersionReplay());
    tx.beginTransaction();
    assert.throws(() => tx.beginVersionReplay(), {
      message: 'Transaction error: A transaction still owns the data store.',
    });
    tx.rollback();
    tx.beginVersionReplay();
    tx.replayVersionTransition(record, false);
    assert.deepEqual(tx.get(1), { index: 1, source: 7, target: 8 });
    tx.replayVersionTransition(record, true);
    assert.equal(tx.get(1), null);
    tx.finishVersionReplay();
  });

  it('commit without an owner changes nothing', () => {
    const tx = openTx();
    tx.commitCurrent();
    assert.deepEqual(tx.journal(), []);
  });
});

describe('shared undo vectors (transaction-undo-vectors.json)', () => {
  const vectors = protocolVectors('transaction-undo-vectors.json');

  /** A names store seeded with `(9: 91 92)` and a decorator over `journal`. */
  function open(journal) {
    const store = namedMemory();
    store.ensureCreated(9);
    store.update(9, 91, 92);
    const log = memoryLog(journal);
    return { store, log, tx: new TransactionsDecorator(store, log) };
  }

  it('recovers canonical undo images and reserves their sequence floors', () => {
    for (const vector of vectors.records) {
      assert.throws(() => parseRecord(vector.wire));
      const { store, log, tx } = open([`__transactions:transition:${vector.wire}`]);
      if (vector.name === 'holes and external names') {
        assert.deepEqual(rows(store), [
          [1, 11, 12],
          [3, 1, 3],
        ]);
        assert.deepEqual(
          store.logicalNames(),
          new Map([
            [0, 'zero'],
            [1, '雪|\0'],
            [77, ''],
          ]),
        );
      } else {
        assert.deepEqual(store.allLinks(), []);
      }
      assert.equal(entriesWith(log, '__transactions:rollback:').length, 1, vector.name);
      const reopened = new TransactionsDecorator(namedMemory(), memoryLog([...log.entries]));
      assert.equal(reopened.lastLoggedSequence(), tx.lastLoggedSequence());
      if (vector.name === 'last sequence') {
        assert.equal(tx.lastLoggedSequence(), 9223372036854775807n);
        assert.throws(() => tx.create(1, 2), {
          message: 'Transaction error: Transaction sequence space is exhausted.',
        });
      } else {
        tx.create(31, 32);
      }
    }
  });

  it('refuses invalid images, keeping rows, history and the recovery fence', () => {
    for (const wire of [...vectors.invalid, vectors.wide, vectors.unpairedUtf16]) {
      const store = namedMemory();
      const log = memoryLog();
      const tx = new TransactionsDecorator(store, log);
      const id = tx.create(11, 12);
      tx.setName(id, 'seed');
      const history = tx.journal();
      log.entries.push(`__transactions:transition:${wire}`);
      for (let attempt = 0; attempt < 2; attempt++) {
        assert.throws(() => tx.recover(), undefined, wire);
        assert.deepEqual(rows(store), [[1, 11, 12]]);
        assert.deepEqual(store.logicalNames(), new Map([[1, 'seed']]));
        assert.deepEqual(tx.journal(), history);
        assert.throws(() => tx.create(21, 22), {
          message: 'Transaction error: Retry recovery before further transaction writes or replay.',
        });
      }
      log.entries.pop();
      tx.recover();
      tx.create(31, 32);
      tx.recover();
    }
  });
});

describe('shared name vectors (transaction-name-vectors.json)', () => {
  const vectors = protocolVectors('transaction-name-vectors.json');
  const first = vectors.records[0].wire;

  it('refuses invalid or conflicting name records before anything changes', () => {
    const bad = vectors.invalid.map((wire) => [`__transactions:transition:${wire}`]);
    bad.push([
      `__transactions:transition:${first}`,
      `__transactions:transition:${first.replace('1,-,', '1,-,0078')}`,
    ]);
    for (const entries of bad) {
      const store = namedMemory();
      const log = memoryLog();
      const tx = new TransactionsDecorator(store, log);
      const id = tx.create(11, 12);
      tx.setName(id, 'original');
      const history = tx.journal();
      log.entries = [...entries];
      for (let attempt = 0; attempt < 2; attempt++) {
        assert.throws(() => tx.recover());
        assert.deepEqual(tx.journal(), history);
        assert.deepEqual(store.getLink(id), { index: id, source: 11, target: 12 });
        assert.deepEqual(store.logicalNames(), new Map([[id, 'original']]));
        assert.deepEqual(log.entries, entries);
        assert.throws(() => tx.setName(id, 'blocked'));
      }
    }
  });

  it('collapses equal duplicate records without losing their names', () => {
    const store = namedMemory();
    const tx = new TransactionsDecorator(
      store,
      memoryLog([
        `__transactions:transition:${first}`,
        `__transactions:transition:${first}`,
        '__transactions:commit:00000000000000000000000000000001',
      ]),
    );
    assert.deepEqual(tx.journal(), [parseRecord(first)]);
    assert.equal(store.logicalNames().get(1), '');
  });

  it('rejects complete named replay over an unnamed adapter before row effects', () => {
    const wire = vectors.records[5].wire;
    const record = parseRecord(wire);
    const tx = openTx();
    const id = tx.create(11, 12);
    const message = 'Storage error: journal requires a named storage adapter';
    assert.throws(() => tx.applyRecord(record), { message });
    assert.throws(() => tx.revertRecord(record), { message });
    assert.deepEqual(tx.getLink(id), { index: id, source: 11, target: 12 });
    assert.throws(() => openTx({ log: memoryLog([`__transactions:transition:${wire}`]) }), {
      message,
    });
  });

  it('replays and reverts every complete record', () => {
    for (const vector of vectors.records) {
      const record = parseRecord(vector.wire);
      const tx = openTx({ store: namedMemory() });
      tx.applyRecord(record);
      tx.revertRecord(record);
      for (const change of record.names) {
        assert.equal(tx.innerStore.logicalNames().get(change.index) ?? null, change.before);
      }
    }
  });
});

describe('TransactionsDecorator', () => {
  it('records creates, updates and deletes as auto-committed transactions', () => {
    const log = memoryLog();
    const tx = openTx({ log });
    assert.equal(tx.commitMode(), CommitMode.Sync);
    assert.equal(tx.retentionPolicy(), INFINITE_RETENTION);
    tx.setCommitMode(CommitMode.Async);
    assert.equal(tx.commitMode(), CommitMode.Async);
    const id = tx.create(1, 2);
    tx.update(id, 3, 4);
    tx.delete(id);
    assert.deepEqual(
      tx
        .log()
        .map(({ sequence, kind, before, after }) => [
          sequence,
          kind,
          [before.index, before.source, before.target],
          [after.index, after.source, after.target],
        ]),
      [
        [1n, TransitionKind.Create, [0, 0, 0], [1, 1, 2]],
        [2n, TransitionKind.Update, [1, 1, 2], [1, 3, 4]],
        [3n, TransitionKind.Delete, [1, 3, 4], [0, 0, 0]],
      ],
    );
    assert.equal(tx.lastLoggedSequence(), 3n);
    assert.equal(tx.appliedSequence(), 3n);
    const ids = new Set(tx.log().map((value) => value.transactionId));
    assert.equal(ids.size, 3);
    for (const owner of ids) {
      assert.equal(tx.isTransactionCommitted(owner), true);
      assert.equal(tx.isTransactionRolledBack(owner), false);
    }
    assert.equal(entriesWith(log, '__transactions:commit:').length, 3);
    assert.equal(entriesWith(log, '__transactions:applied:').length, 3);
    assert.equal(tx.logStore(), log);
    assert.equal(tx.inner(), tx.innerStore);
    assert.equal(
      TransactionsDecorator.makeTransitionsDatabaseFilename('db.links'),
      'db.transitions.links',
    );
  });

  it('commits and rolls back explicit transactions', () => {
    const errors = capture();
    const tx = openTx({ trace: true, stderr: errors });
    const handle = tx.beginTransaction();
    assert.equal(typeof handle.id, 'bigint');
    assert.equal(typeof handle.startedMs, 'bigint');
    assert.throws(() => tx.beginTransaction(), {
      message: 'Transaction error: Nested transactions are not supported.',
    });
    const id = tx.create(1, 1);
    tx.createAndUpdate(5, 6);
    tx.rollback();
    assert.deepEqual(tx.all(), []);
    assert.equal(tx.isTransactionRolledBack(handle.id), true);
    tx.rollback();
    tx.commit();
    tx.beginTransaction();
    assert.equal(tx.create(1, 1), id);
    tx.commit();
    assert.deepEqual(rows(tx), [[1, 1, 1]]);
    assert.match(
      errors.text,
      /^\[Transactions\] Recorded Create seq=1 tx=[0-9a-f]{32}: \(0,0,0\) -> \(1,1,1\)\.$/m,
    );
    assert.match(
      errors.text,
      /^\[Transactions\] Rolled back tx [0-9a-f]{32} \(3 transitions\)\.$/m,
    );
    assert.match(
      errors.text,
      /^\[Transactions\] Committed tx [0-9a-f]{32} \(mode=Sync, transitions=1\)\.$/m,
    );
  });

  it('serves the NamedTypeLinks bridge', () => {
    const store = namedMemory();
    const tx = openTx({ store });
    assert.equal(tx.getOrCreate(1, 1), 1);
    assert.equal(tx.tryGetOrCreate(1, 1), 1);
    assert.equal(tx.tryCreate(2, 2), 2);
    assert.equal(tx.exists(2), true);
    assert.equal(tx.search(2, 2), 2);
    assert.equal(tx.ensureCreated(0), 0);
    assert.equal(tx.ensureCreated(1), 1);
    assert.equal(tx.ensureCreated(4), 4);
    assert.deepEqual(rows(tx), [
      [1, 1, 1],
      [2, 2, 2],
      [4, 0, 0],
    ]);
    assert.deepEqual(
      tx.query(null, 2).map((value) => value.index),
      [2],
    );
    assert.deepEqual(tx.get(2), tx.getLink(2));
    assert.equal(typeof tx.setName(2, 'two'), 'number');
    assert.equal(tx.getName(2), 'two');
    assert.equal(tx.getByName('two'), 2);
    tx.removeName(2);
    assert.equal(tx.getName(2), null);
    const processor = { preview: (scratch) => scratch.create(9, 9) };
    assert.deepEqual(tx.reserveQueryAllocations(processor, [], []), new Set([1, 2, 3, 4]));
    assert.equal(tx.exists(3), false);
  });

  it('commits a create and update as one transaction', () => {
    const tx = openTx();
    assert.equal(tx.createAndUpdate(5, 6), 1);
    assert.deepEqual(rows(tx), [[1, 5, 6]]);
    assert.equal(new Set(tx.log().map((value) => value.transactionId)).size, 1);
  });

  it('ignores observations without an address', () => {
    const store = LinkStorage.inMemory();
    const tx = openTx({ store });
    store.updateObserved = (_id, _source, _target, observer) => {
      observer(EMPTY_DOUBLET, EMPTY_DOUBLET);
      return 0;
    };
    tx.update(1, 2, 2);
    assert.deepEqual(
      tx.log().map(({ kind, before, after }) => [kind, before.index, after.index]),
      [[TransitionKind.Update, 1, 1]],
    );
  });

  it('records an unchanged update and a delete the store did not report', () => {
    const tx = openTx();
    const id = tx.create(1, 2);
    tx.update(id, 1, 2);
    const store = tx.innerStore;
    const quiet = {
      updateObserved: (index) => store.getLink(index),
      deleteObserved: (index) => store.getLink(index),
    };
    tx.storage.store = new Proxy(store, {
      get: (target, key) =>
        quiet[key] ?? Reflect.get(target, key, target).bind?.(target) ?? target[key],
    });
    tx.update(9, 3, 4);
    tx.delete(id);
    assert.deepEqual(
      tx.log().map(({ kind, before, after }) => [kind, before.index, after.index, after.source]),
      [
        [TransitionKind.Create, 0, 1, 1],
        [TransitionKind.Update, 1, 1, 1],
        [TransitionKind.Update, 9, 9, 3],
        [TransitionKind.Delete, 1, 0, 0],
      ],
    );
  });

  it('flushes, saves and reloads a file-backed store and log', () => {
    const directory = scratchDirectory();
    const path = join(directory, 'db.links');
    const log = FileTransitionLog.open(join(directory, 'journal.log'));
    try {
      const tx = new TransactionsDecorator(new LinkStorage(path), log);
      tx.create(1, 1);
      tx.save();
      assert.equal(tx.hasExternalChanges(), false);
      writeFileSync(path, '(1 1 1)\n(2 2 2)\n');
      assert.equal(tx.hasExternalChanges(), true);
      tx.reload();
      assert.deepEqual(rows(tx), [
        [1, 1, 1],
        [2, 2, 2],
      ]);
      tx.beginTransaction();
      assert.throws(() => tx.reload(), {
        message: 'Transaction error: Cannot reload while a transaction is open.',
      });
      assert.throws(() => tx.recover(), {
        message: 'Transaction error: Cannot recover while a transaction is open.',
      });
      tx.rollback();
    } finally {
      log.close();
    }
  });

  it('selects row undo only while idle', () => {
    const tx = openTx().withRowUndo('WriteAheadImage');
    assert.equal(tx.rowUndoPolicy(), 'WriteAheadImage');
    tx.beginTransaction();
    assert.throws(() => tx.setRowUndoPolicy('Observed'), {
      message: 'Transaction error: Select row undo before opening a transaction.',
    });
    tx.rollback();
    tx.setRowUndoPolicy('Observed');
    assert.equal(tx.rowUndoPolicy(), 'Observed');
  });

  it('refuses unlogged replay during a transaction', () => {
    const tx = openTx();
    tx.beginTransaction();
    const message =
      'Transaction error: Complete the current transaction or its cleanup before unlogged replay.';
    const value = transition(TransitionKind.Create, EMPTY_DOUBLET, row(1));
    assert.throws(() => tx.applyTransition(value), { message });
    assert.throws(() => tx.revertTransition(value), { message });
    assert.throws(() => tx.applyRecord(journalRecord(value)), { message });
    assert.throws(() => tx.revertRecord(journalRecord(value)), { message });
  });

  it('fences writes and persistence during version replay', () => {
    const tx = openTx();
    tx.beginVersionReplay();
    const message =
      'Transaction error: Complete version movement before accessing transaction writes or persistence.';
    assert.throws(() => tx.create(1, 1), { message });
    assert.throws(() => tx.flush(), { message });
    assert.throws(() => tx.beginTransaction(), { message });
    assert.throws(() => tx.reload(), {
      message: 'Transaction error: Cannot reload while a transaction is open.',
    });
    tx.finishVersionReplay();
  });

  it('applies and reverts transitions of every kind', () => {
    const tx = openTx();
    tx.applyTransition(transition(TransitionKind.Create, EMPTY_DOUBLET, row(2)));
    assert.deepEqual(rows(tx), [[2, 7, 8]]);
    tx.applyTransition(transition(TransitionKind.Update, row(2), doublet(2, 1, 1)));
    assert.deepEqual(rows(tx), [[2, 1, 1]]);
    tx.revertTransition(transition(TransitionKind.Update, row(2), doublet(2, 1, 1)));
    assert.deepEqual(rows(tx), [[2, 7, 8]]);
    tx.applyTransition(transition(TransitionKind.Delete, row(2), EMPTY_DOUBLET));
    assert.deepEqual(rows(tx), []);
    tx.revertTransition(transition(TransitionKind.Delete, doublet(3, 3, 3), EMPTY_DOUBLET));
    assert.deepEqual(rows(tx), [[3, 3, 3]]);
    tx.revertTransition(transition(TransitionKind.Create, EMPTY_DOUBLET, doublet(3, 3, 3)));
    assert.deepEqual(rows(tx), []);
  });
});
