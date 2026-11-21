// Recording of observed changes, explicit names and write-ahead row undo, and
// how failed writes end their owner: rust/src/transactions/recording.rs,
// names.rs and row_undo.rs, with the failing adapters of
// rust/tests/transaction_row_wal_tests.rs and transaction_name_failure_tests.rs.
import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { LinkStorage } from '../src/link-storage.js';
import {
  COMMIT_MARKER_PREFIX,
  doublet,
  EMPTY_DOUBLET,
  I64_MAX,
  ROLLBACK_MARKER_PREFIX,
  RowUndoPolicy,
  TransitionKind,
} from '../src/transactions.js';
import { entriesWith, faulty, memoryLog, namedMemory, openTx } from './helpers/transactions.mjs';

const { WriteAheadImage } = RowUndoPolicy;
const rows = (tx) => tx.allLinks().map(({ index, source, target }) => [index, source, target]);
const REQUIRES_ROLLBACK = 'Transaction error: Transaction requires rollback before further writes.';

describe('failed writes', () => {
  const writes = [
    ['create', (tx) => tx.create(5, 5), 'injected create'],
    ['updateObserved', (tx) => tx.update(1, 5, 5), 'Storage error: injected updateObserved'],
    ['deleteObserved', (tx) => tx.delete(1), 'Storage error: injected deleteObserved'],
    ['ensureCreated', (tx) => tx.ensureCreated(5), 'injected ensureCreated'],
    ['updateObserved', (tx) => tx.createAndUpdate(5, 5), 'Storage error: injected updateObserved'],
  ];

  it('end an auto-commit owner without changes', () => {
    for (const [method, write, message] of writes) {
      const store = faulty(LinkStorage.inMemory());
      const tx = openTx({ store });
      tx.create(1, 1);
      store.fail(method);
      assert.throws(() => write(tx), { message });
      assert.equal(tx.current, null);
      assert.deepEqual(rows(tx), [[1, 1, 1]]);
      tx.create(2, 2);
    }
  });

  it('keep an explicit owner open', () => {
    for (const [method, write, message] of writes) {
      const store = faulty(LinkStorage.inMemory());
      const tx = openTx({ store });
      tx.create(1, 1);
      const { id } = tx.beginTransaction();
      store.fail(method);
      assert.throws(() => write(tx), { message });
      assert.equal(tx.current.id, id);
      tx.commit();
    }
  });

  it('roll back changes reported before the failure', () => {
    const store = LinkStorage.inMemory();
    const tx = openTx({ store });
    tx.create(1, 1);
    for (const method of ['updateObserved', 'deleteObserved']) {
      const real = store[method].bind(store);
      store[method] = (...args) => {
        real(...args);
        throw new Error(`late ${method}`);
      };
    }
    assert.throws(() => tx.update(1, 2, 2), { message: 'Storage error: late updateObserved' });
    assert.deepEqual(rows(tx), [[1, 1, 1]]);
    assert.throws(() => tx.delete(1), { message: 'Storage error: late deleteObserved' });
    assert.deepEqual(rows(tx), [[1, 1, 1]]);
    assert.equal(entriesWith(tx.logStore(), ROLLBACK_MARKER_PREFIX).length, 2);
  });

  it('roll back an owner with a write-ahead image', () => {
    const store = faulty(LinkStorage.inMemory());
    const tx = openTx({ store }).withRowUndo(WriteAheadImage);
    tx.create(1, 1);
    store.fail('create');
    assert.throws(() => tx.create(2, 2), { message: 'injected create' });
    assert.equal(tx.current, null);
    assert.deepEqual(rows(tx), [[1, 1, 1]]);
    tx.beginTransaction();
    store.fail('create');
    assert.throws(() => tx.create(2, 2), { message: 'injected create' });
    assert.throws(() => tx.create(2, 2), { message: REQUIRES_ROLLBACK });
    tx.rollback();
  });

  it('roll back when the undo image cannot be written', () => {
    const log = faulty(memoryLog());
    const tx = openTx({ log }).withRowUndo(WriteAheadImage);
    log.fail('appendLogEntry');
    assert.throws(() => tx.create(1, 1), { message: 'injected appendLogEntry' });
    assert.equal(tx.current, null);
    tx.beginTransaction();
    log.fail('flushLog');
    assert.throws(() => tx.create(1, 1), { message: 'injected flushLog' });
    assert.throws(() => tx.create(1, 1), { message: REQUIRES_ROLLBACK });
    tx.rollback();
    assert.deepEqual(rows(tx), []);
  });

  it('fence an owner whose batch exceeds the sequence space', () => {
    const store = LinkStorage.inMemory();
    const tx = openTx({ store });
    tx.create(1, 1);
    const real = store.updateObserved.bind(store);
    store.updateObserved = (id, source, target, observer) =>
      real(id, source, target, (before, after) => {
        observer(before, after);
        observer(EMPTY_DOUBLET, doublet(7, 7, 7));
      });
    tx.sequenceCounter = I64_MAX - 1n;
    const message = 'Transaction error: Transaction sequence space is exhausted.';
    assert.throws(() => tx.update(1, 2, 2), { message });
    assert.deepEqual(rows(tx), [[1, 1, 1]]);
    assert.equal(tx.lastLoggedSequence(), I64_MAX - 1n);
    tx.beginTransaction();
    assert.throws(() => tx.update(1, 2, 2), { message });
    assert.throws(() => tx.update(1, 2, 2), { message: REQUIRES_ROLLBACK });
    tx.rollback();
    assert.deepEqual(rows(tx), [[1, 1, 1]]);
  });
});

describe('names', () => {
  it('fence an owner when names cannot be captured after a write', () => {
    for (const [answer, message] of [
      [() => null, 'Storage error: Named storage lost its name-capture capability.'],
      [
        () => {
          throw new Error('names lost');
        },
        'Storage error: names lost',
      ],
    ]) {
      const store = namedMemory();
      const tx = openTx({ store });
      const id = tx.create(1, 1);
      tx.setName(id, 'one');
      // The capture before a write succeeds; the one after it fails.
      const real = store.logicalNames.bind(store);
      let countdown = 0;
      store.logicalNames = () => (countdown > 0 && --countdown === 0 ? answer() : real());
      tx.beginTransaction();
      countdown = 2;
      assert.throws(() => tx.update(id, 2, 2), { message });
      assert.deepEqual(tx.current.transitions[0].names, [
        { index: id, before: 'one', after: 'one' },
      ]);
      assert.throws(() => tx.delete(id), { message: REQUIRES_ROLLBACK });
      tx.rollback();
      countdown = 2;
      assert.throws(() => tx.delete(id), { message });
      assert.deepEqual(tx.journal().at(-1).names, [{ index: id, before: 'one', after: null }]);
      assert.deepEqual(rows(tx), [[1, 1, 1]]);
      assert.equal(tx.getName(id), 'one');
    }
  });

  it('records a name the store moved without a row callback', () => {
    const store = namedMemory();
    const tx = openTx({ store });
    tx.create(1, 1);
    tx.create(2, 2);
    tx.setName(1, 'a');
    const real = store.updateObserved.bind(store);
    store.updateObserved = (...args) => {
      store.setName(1, 'b');
      return real(...args);
    };
    tx.update(2, 3, 3);
    const moved = tx.journal().at(-1);
    assert.equal(moved.nameOnly, true);
    assert.deepEqual(moved.names, [{ index: 1, before: 'a', after: 'b' }]);
  });

  it('records a name the store moved during a delete', () => {
    const store = namedMemory();
    const tx = openTx({ store });
    tx.create(1, 1);
    tx.create(2, 2);
    tx.setName(1, 'a');
    const real = store.deleteObserved.bind(store);
    store.deleteObserved = (...args) => {
      store.setName(1, 'b');
      return real(...args);
    };
    tx.delete(2);
    assert.deepEqual(tx.journal().at(-1).names, [{ index: 1, before: 'a', after: 'b' }]);
  });

  it('journals explicit name writes', () => {
    assert.throws(() => openTx().setName(1, 'a'), {
      message: 'Storage error: storage does not support names',
    });
    const tx = openTx({ store: namedMemory() });
    tx.create(1, 1);
    tx.create(2, 2);
    tx.setName(1, 'a');
    const length = tx.journal().length;
    tx.setName(1, 'a');
    assert.equal(tx.journal().length, length);
    tx.setName(2, 'a');
    assert.deepEqual(tx.journal().at(-1).names, [
      { index: 1, before: 'a', after: null },
      { index: 2, before: null, after: 'a' },
    ]);
    tx.removeName(2);
    assert.equal(tx.getName(2), null);
    tx.setRowUndoPolicy(WriteAheadImage);
    const commits = entriesWith(tx.logStore(), COMMIT_MARKER_PREFIX).length;
    tx.setName(2, 'b');
    tx.removeName(1);
    assert.equal(entriesWith(tx.logStore(), COMMIT_MARKER_PREFIX).length, commits + 2);
    assert.equal(tx.getName(2), 'b');
  });

  it('fence a name write whose undo record is not durable', () => {
    const log = faulty(memoryLog());
    const tx = openTx({ store: namedMemory(), log });
    tx.create(1, 1);
    log.fail('flushLog');
    assert.throws(() => tx.setName(1, 'a'), { message: 'injected flushLog' });
    assert.equal(tx.getName(1), null);
    tx.beginTransaction();
    log.fail('flushLog');
    assert.throws(() => tx.setName(1, 'a'), { message: 'injected flushLog' });
    assert.throws(() => tx.setName(1, 'a'), { message: REQUIRES_ROLLBACK });
    tx.rollback();
    assert.equal(tx.getName(1), null);
  });
});

describe('write-ahead row undo', () => {
  it('records effects the store did not report and undoes them', () => {
    const store = LinkStorage.inMemory();
    const tx = openTx({ store }).withRowUndo(WriteAheadImage);
    tx.create(1, 1);
    tx.create(2, 2);
    const real = store.create.bind(store);
    store.create = (source, target) => {
      const id = real(source, target);
      real(9, 9);
      store.updateRaw(1, 5, 5);
      store.delete(2);
      return id;
    };
    tx.beginTransaction();
    tx.create(3, 3);
    delete store.create;
    assert.deepEqual(
      tx.current.transitions.map(({ kind, before, after }) => [kind, before.index, after.index]),
      [
        [TransitionKind.Create, 0, 3],
        [TransitionKind.Update, 1, 1],
        [TransitionKind.Delete, 2, 0],
        [TransitionKind.Create, 0, 4],
      ],
    );
    tx.rollback();
    assert.deepEqual(rows(tx), [
      [1, 1, 1],
      [2, 2, 2],
    ]);
  });

  it('rejects an invalid row enumeration after a write', () => {
    for (const bad of [(row) => [{ ...row, index: 0 }], (row) => [row, row]]) {
      const store = LinkStorage.inMemory();
      const tx = openTx({ store }).withRowUndo(WriteAheadImage);
      const real = store.allLinks.bind(store);
      let call = 0;
      store.allLinks = () => (++call === 2 ? bad(real()[0]) : real());
      assert.throws(() => tx.create(1, 1), {
        message: 'Storage error: invalid row enumeration after a write',
      });
      assert.equal(tx.current, null);
      assert.deepEqual(rows(tx), []);
    }
  });

  it('ends the owner once when a correction cannot be journaled', () => {
    const store = LinkStorage.inMemory();
    const log = faulty(memoryLog());
    const tx = openTx({ store, log }).withRowUndo(WriteAheadImage);
    const real = store.create.bind(store);
    store.create = (source, target) => {
      const id = real(source, target);
      real(9, 9);
      return id;
    };
    log.fail('appendLogEntry', { after: 2 });
    assert.throws(() => tx.create(1, 1), { message: 'injected appendLogEntry' });
    assert.equal(tx.current, null);
    assert.deepEqual(rows(tx), []);
    assert.equal(entriesWith(log, ROLLBACK_MARKER_PREFIX).length, 1);
  });

  it('restores holes and empty rows', () => {
    const tx = openTx().withRowUndo(WriteAheadImage);
    for (let i = 1; i <= 3; i++) tx.create(i, i);
    tx.ensureCreated(4);
    tx.delete(2);
    tx.beginTransaction();
    tx.delete(3);
    tx.delete(1);
    tx.create(7, 7);
    tx.rollback();
    assert.deepEqual(rows(tx), [
      [1, 1, 1],
      [3, 3, 3],
      [4, 0, 0],
    ]);
  });

  it('fails closed when the store cannot restore the image', () => {
    const tx = openTx({ store: namedMemory() }).withRowUndo(WriteAheadImage);
    tx.create(1, 1);
    tx.setName(1, 'a');
    const message = 'Storage error: storage did not restore the complete undo image';
    const storage = tx.storage;
    for (const [method, broken] of [
      ['restoreLink', () => {}],
      ['setLinkName', () => 0],
      ['captureNames', () => null],
    ]) {
      tx.beginTransaction();
      tx.update(1, 2, 2);
      tx.setName(1, 'b');
      storage[method] = broken;
      assert.throws(
        () => tx.rollback(),
        method === 'captureNames'
          ? { message: 'Storage error: undo image requires a named storage adapter' }
          : { message },
      );
      delete storage[method];
      tx.rollback();
      assert.deepEqual(rows(tx), [[1, 1, 1]]);
      assert.equal(tx.getName(1), 'a');
    }
  });
});
