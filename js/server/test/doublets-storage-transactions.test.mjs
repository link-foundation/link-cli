// The transactions layer over the file-backed `DoubletsStorage`, as
// rust/examples/embedded_store.rs runs `GenericTransactionsDecorator` over
// `DoubletsStorage<usize, FileMappedUnitStore<usize>>`: the decorator uses
// the storage as is (it already speaks the transaction storage protocol),
// commits survive a reopen, an abandoned transaction is rolled back by the
// recovery of the next open, and a rollback undoes updates and deletions.
import assert from 'node:assert/strict';
import { join } from 'node:path';
import { describe, it } from 'node:test';

import {
  CommitMode,
  DoubletsStorage,
  FileTransitionLog,
  INFINITE_RETENTION,
  TransactionsDecorator,
} from '../src/index.js';
import { lockBackend } from '../src/storage-lock.js';
import { scratchDirectory } from './helpers/storage.mjs';

const locks = { skip: lockBackend().unavailable };
const parts = (value) => [value.index, value.source, value.target];

/** The transactions layer over a storage at `database`, its log at `log`. */
function open(database, log, store = DoubletsStorage.open(database)) {
  const transactions = new TransactionsDecorator(
    store,
    FileTransitionLog.open(log),
    INFINITE_RETENTION,
    CommitMode.Sync,
    false,
  );
  return {
    store,
    transactions,
    close() {
      transactions.logStore().close();
      store.close();
    },
  };
}

/** A database and a log path in a fresh scratch directory. */
function paths() {
  const directory = scratchDirectory();
  return [join(directory, 'db.links'), join(directory, 'db.transitions.log')];
}

describe('transactions over a doublets storage', () => {
  it('use the storage itself rather than an adapter', () => {
    const [database, log] = paths();
    const first = open(database, log);
    try {
      assert.equal(first.transactions.storage, first.store);
      assert.equal(first.transactions.innerStore, first.store);
    } finally {
      first.close();
    }
  });

  it('keep a commit and roll an abandoned transaction back on reopen', () => {
    const [database, log] = paths();
    const first = open(database, log);
    first.transactions.beginTransaction();
    const committed = first.transactions.create(0, 0);
    first.transactions.commit();
    first.transactions.beginTransaction();
    const abandoned = first.transactions.create(committed, committed);
    assert.ok(first.store.linkExists(abandoned));
    first.close();

    const second = open(database, log);
    try {
      assert.ok(second.transactions.exists(committed));
      assert.ok(!second.transactions.exists(abandoned));
      assert.deepEqual(second.transactions.all().map(parts), [[1, 0, 0]]);
    } finally {
      second.close();
    }
  });

  it('undo updates and deletions on rollback', () => {
    const [database, log] = paths();
    const store = open(database, log);
    try {
      const { transactions } = store;
      transactions.beginTransaction();
      const point = transactions.create(0, 0);
      const pair = transactions.create(point, point);
      transactions.commit();

      transactions.beginTransaction();
      transactions.update(pair, pair, point);
      transactions.delete(point);
      assert.deepEqual(transactions.all().map(parts), [[pair, pair, point]]);
      transactions.rollback();

      assert.deepEqual(transactions.all().map(parts), [
        [point, 0, 0],
        [pair, point, point],
      ]);
      assert.deepEqual(parts(transactions.get(pair)), [pair, point, point]);
    } finally {
      store.close();
    }
  });

  it('lock a second writer out of an exclusive storage', locks, () => {
    const [database, log] = paths();
    const first = open(database, log, DoubletsStorage.openExclusive(database));
    try {
      assert.equal(DoubletsStorage.tryOpenExclusive(database), null);
    } finally {
      first.close();
    }
    const again = DoubletsStorage.tryOpenExclusive(database);
    assert.ok(again);
    again.close();
  });
});
