// The transitions logs: the append-only `FileTransitionLog` and
// `truncate_torn_tail` of rust/src/transactions/log.rs, the links-backed log
// (`impl TransitionLogStore for NamedTypesDecorator`) and the storage seam of
// rust/src/storage/decorator_impls.rs the transactions layer writes through.
import assert from 'node:assert/strict';
import {
  chmodSync,
  closeSync,
  existsSync,
  mkdirSync,
  openSync,
  readFileSync,
  writeFileSync,
} from 'node:fs';
import { join } from 'node:path';
import { describe, it } from 'node:test';
import { InvalidFormatError, LinkError, LockError, StorageError } from '../src/errors.js';
import { LinkStorage } from '../src/link-storage.js';
import { NamedTypesDecorator } from '../src/named-types.js';
import { FileLock, lockBackend, lockFilePath, LockMode } from '../src/storage-lock.js';
import { FileTransitionLog, truncateTornTail } from '../src/transactions-log.js';
import {
  guarded,
  io,
  ioError,
  NamedTransitionLog,
  parentDirectory,
  storageError,
  syncPath,
  TransactionStorage,
  transitionLogStore,
} from '../src/transactions-storage.js';
import { memoryLog, namedMemory } from './helpers/transactions.mjs';
import { scratchDirectory } from './helpers/storage.mjs';

/** The bytes `truncate_torn_tail` keeps of a file holding `contents`. */
function repaired(contents) {
  const path = join(scratchDirectory(), 'log');
  writeFileSync(path, contents);
  const fd = openSync(path, 'r+');
  try {
    truncateTornTail(fd);
  } finally {
    closeSync(fd);
  }
  return readFileSync(path, 'latin1');
}

describe('truncateTornTail', () => {
  it('keeps complete lines and blank lines', () => {
    assert.equal(repaired(''), '');
    assert.equal(repaired('a\nb\n'), 'a\nb\n');
    assert.equal(repaired('a\n\n'), 'a\n\n');
    assert.equal(repaired('a\n\n\n'), 'a\n\n\n');
  });

  it('drops a fragment after the last newline', () => {
    assert.equal(repaired('a\nb'), 'a\n');
    assert.equal(repaired('abc'), '');
  });

  it('drops trailing lines holding NUL', () => {
    assert.equal(repaired('a\n\0\0\n'), 'a\n');
    assert.equal(repaired('a\n\0\n\0x\n'), 'a\n');
    assert.equal(repaired('\0\n'), '');
    assert.equal(repaired('a\nb\0\nfragment'), 'a\n');
  });

  it('keeps NUL before an intact line, as damage opening refuses', () => {
    assert.equal(repaired('\0x\na\n'), '\0x\na\n');
  });

  it('scans across 8 KiB chunks', () => {
    const long = 'x'.repeat(10000);
    assert.equal(repaired(`${long}\ntail`), `${long}\n`);
    assert.equal(repaired(`a\n${'y'.repeat(9000)}\n`), `a\n${'y'.repeat(9000)}\n`);
    assert.equal(repaired(`a\n${'\0'.repeat(9000)}\n`), 'a\n');
  });
});

describe('FileTransitionLog', () => {
  it('creates its parent directories and appends durable lines', () => {
    const path = join(scratchDirectory(), 'nested', 'deeper', 'journal.log');
    const log = FileTransitionLog.open(path);
    try {
      assert.equal(log.path(), path);
      assert.equal(log.syncOnAppend(), true);
      assert.equal(log.supportsDurableFlush(), true);
      log.appendLogEntry('first');
      log.setSyncOnAppend(false);
      assert.equal(log.syncOnAppend(), false);
      log.appendLogEntry('second');
      log.flushLog();
      assert.deepEqual(log.readLogEntries(), ['first', 'second']);
      assert.equal(readFileSync(path, 'utf8'), 'first\nsecond\n');
    } finally {
      log.close();
    }
  });

  const lockable = { skip: lockBackend().unavailable };

  it('holds the sidecar writer lock until closed', lockable, () => {
    const path = join(scratchDirectory(), 'journal.log');
    const log = FileTransitionLog.openExclusive(path);
    try {
      assert.ok(existsSync(`${path}.lock`));
      assert.throws(
        () => FileTransitionLog.openExclusive(path),
        (error) =>
          error instanceof LockError && error.message === 'Lock error: journal writer is busy',
      );
      assert.equal(FileLock.tryAcquire(lockFilePath(path), LockMode.Shared), null);
      log.appendLogEntry('locked');
    } finally {
      log.close();
    }
    const reopened = FileTransitionLog.openExclusive(path);
    assert.deepEqual(reopened.readLogEntries(), ['locked']);
    reopened.close();
  });

  it('releases the writer lock when the log cannot open', lockable, () => {
    const path = join(scratchDirectory(), 'journal.log');
    mkdirSync(path);
    assert.throws(() => FileTransitionLog.openExclusive(path), {
      message: /^I\/O error: Is a directory \(os error \d+\)$/,
    });
    const lock = FileLock.tryAcquire(lockFilePath(path), LockMode.Exclusive);
    assert.notEqual(lock, null);
    lock.release();
  });

  it('opens a log in the current directory', () => {
    const previous = process.cwd();
    process.chdir(scratchDirectory());
    try {
      const log = FileTransitionLog.open('relative.log');
      log.appendLogEntry('entry');
      log.close();
      assert.equal(readFileSync('relative.log', 'utf8'), 'entry\n');
      assert.equal(parentDirectory('relative.log'), '.');
    } finally {
      process.chdir(previous);
    }
  });

  it('discards a torn tail on open and reads only complete lines', () => {
    const path = join(scratchDirectory(), 'journal.log');
    writeFileSync(path, 'one\r\n\ntwo\npartial');
    const log = FileTransitionLog.open(path);
    try {
      assert.equal(readFileSync(path, 'utf8'), 'one\r\n\ntwo\n');
      assert.deepEqual(log.readLogEntries(), ['one', 'two']);
      log.appendLogEntry('three');
      assert.deepEqual(log.readLogEntries(), ['one', 'two', 'three']);
    } finally {
      log.close();
    }
  });

  it('refuses entries with line breaks or NUL', () => {
    const log = FileTransitionLog.open(join(scratchDirectory(), 'journal.log'));
    try {
      for (const entry of ['a\nb', 'a\rb', 'a\0b']) {
        assert.throws(
          () => log.appendLogEntry(entry),
          (error) =>
            error instanceof InvalidFormatError &&
            error.message ===
              'Invalid link format: transition log entries must not contain line breaks or NUL',
        );
      }
      assert.deepEqual(log.readLogEntries(), []);
    } finally {
      log.close();
    }
  });

  it('reports invalid UTF-8 as read_to_string does', () => {
    const path = join(scratchDirectory(), 'journal.log');
    writeFileSync(path, Buffer.from([0x61, 0xff, 0x0a]));
    const log = FileTransitionLog.open(path);
    try {
      assert.throws(
        () => log.readLogEntries(),
        (error) =>
          error instanceof LinkError &&
          error.message === 'I/O error: stream did not contain valid UTF-8',
      );
    } finally {
      log.close();
    }
  });

  it('repairs the tail after a failed write before the next operation', () => {
    const path = join(scratchDirectory(), 'journal.log');
    const log = FileTransitionLog.open(path);
    log.appendLogEntry('kept');
    // A fragment a short write left behind, and a writer whose next write fails.
    writeFileSync(path, 'kept\nfragm', { flag: 'w' });
    closeSync(log.fd);
    const badDescriptor = (error) =>
      error instanceof LinkError && error.message === 'I/O error: Bad file descriptor (os error 9)';
    assert.throws(() => log.appendLogEntry('lost'), badDescriptor);
    assert.equal(log.tailMayBeTorn, true);
    assert.deepEqual(log.readLogEntries(), ['kept']);
    assert.equal(log.tailMayBeTorn, false);
    assert.equal(readFileSync(path, 'utf8'), 'kept\n');
    assert.throws(() => log.flushLog(), badDescriptor);
    assert.equal(log.tailMayBeTorn, true);
    // A failed repair keeps the flag, so the next operation retries it.
    chmodSync(path, 0o000);
    try {
      assert.throws(
        () => log.readLogEntries(),
        (error) => error.message === 'I/O error: Permission denied (os error 13)',
      );
      assert.equal(log.tailMayBeTorn, true);
    } finally {
      chmodSync(path, 0o600);
    }
    assert.deepEqual(log.readLogEntries(), ['kept']);
  });

  it('reports a parent that is not a directory', () => {
    const directory = scratchDirectory();
    writeFileSync(join(directory, 'file'), '');
    assert.throws(
      () => FileTransitionLog.open(join(directory, 'file', 'sub', 'journal.log')),
      (error) => error.message === 'I/O error: Not a directory (os error 20)',
    );
    assert.throws(
      () => FileTransitionLog.open(join(directory, 'file', 'journal.log')),
      (error) => error.message === 'I/O error: Not a directory (os error 20)',
    );
  });

  it('closes the log when its directory cannot be synced', () => {
    const directory = join(scratchDirectory(), 'unreadable');
    FileTransitionLog.open(join(directory, 'journal.log')).close();
    chmodSync(directory, 0o300);
    try {
      assert.throws(
        () => FileTransitionLog.open(join(directory, 'journal.log')),
        (error) => error.message === 'I/O error: Permission denied (os error 13)',
      );
    } finally {
      chmodSync(directory, 0o700);
    }
  });
});

describe('error helpers', () => {
  it('reports storage errors with their whole cause chain', () => {
    const error = storageError(new Error('outer', { cause: new Error('inner') }));
    assert.ok(error instanceof StorageError);
    assert.equal(error.message, 'Storage error: outer: inner');
    assert.throws(
      () =>
        guarded(() => {
          throw new Error('broken');
        }),
      { message: 'Storage error: broken' },
    );
    assert.equal(
      guarded(() => 7),
      7,
    );
  });

  it('turns system errors into LinkError::Io and passes others through', () => {
    const other = new Error('plain');
    assert.equal(ioError(other), other);
    assert.throws(
      () =>
        io(() => {
          throw other;
        }),
      (error) => error === other,
    );
    assert.throws(
      () => syncPath(join(scratchDirectory(), 'missing')),
      (error) =>
        error instanceof LinkError &&
        error.message === 'I/O error: No such file or directory (os error 2)' &&
        error.cause.code === 'ENOENT',
    );
    const denoStyle = Object.assign(new Error('Bad file descriptor (os error 9)'), {
      code: 'EBADF',
    });
    const wrapped = ioError(denoStyle);
    assert.ok(wrapped instanceof LinkError);
    assert.equal(wrapped.message, 'I/O error: Bad file descriptor (os error 9)');
    assert.equal(wrapped.cause, denoStyle);
    assert.equal(ioError(null), null);
  });

  it('names the parent directory as Path::parent', () => {
    assert.equal(parentDirectory('/var/data/db.links'), '/var/data');
    assert.equal(parentDirectory('db.links'), '.');
  });
});

describe('TransactionStorage', () => {
  it('adapts a plain store', () => {
    const store = LinkStorage.inMemory();
    const storage = new TransactionStorage(store);
    const changes = [];
    const observe = (before, after) => changes.push([before, after]);
    assert.equal(storage.named, false);
    assert.equal(storage.createLinkObserved(1, 1, observe), 1);
    assert.deepEqual(changes.splice(0), [
      [
        { index: 0, source: 0, target: 0 },
        { index: 1, source: 1, target: 1 },
      ],
    ]);
    assert.equal(storage.ensureLinkCreatedObserved(0, observe), 0);
    assert.equal(storage.ensureLinkCreatedObserved(1, observe), 1);
    assert.equal(storage.ensureLinkCreatedObserved(3, observe), 3);
    assert.deepEqual(changes.splice(0), [
      [
        { index: 0, source: 0, target: 0 },
        { index: 3, source: 0, target: 0 },
      ],
    ]);
    assert.equal(storage.ensureLinkCreated(2), 2);
    assert.equal(storage.linkExists(2), true);
    assert.equal(storage.getLink(9), null);
    assert.deepEqual(storage.getLink(1), { index: 1, source: 1, target: 1 });
    assert.deepEqual(storage.updateLink(2, 1, 3), { index: 2, source: 0, target: 0 });
    assert.equal(storage.searchLink(1, 3), 2);
    assert.deepEqual(
      storage.queryLinks().map((value) => value.index),
      [1, 2, 3],
    );
    assert.deepEqual(
      storage.queryLinks(null, 1).map((value) => value.index),
      [1, 2],
    );
    assert.deepEqual(storage.restoreLink(2, 1, 1), { index: 2, source: 1, target: 3 });
    assert.deepEqual(storage.updateLinkObserved(3, 1, 2, observe), {
      index: 3,
      source: 0,
      target: 0,
    });
    assert.deepEqual(changes.splice(0), [
      [
        { index: 3, source: 0, target: 0 },
        { index: 3, source: 1, target: 2 },
      ],
    ]);
    assert.deepEqual(storage.deleteLinkObserved(3, observe), { index: 3, source: 1, target: 2 });
    assert.equal(changes.splice(0).length, 2);
    storage.deleteLink(2);
    assert.deepEqual(
      storage.allLinks().map((value) => value.index),
      [1],
    );
    assert.equal(storage.captureNames(), null);
    assert.equal(storage.hasExternalChanges(), false);
    storage.flush();
    storage.reload();
    assert.throws(
      () => storage.setLinkName(1, 'one'),
      (error) =>
        error instanceof StorageError &&
        error.message === 'Storage error: storage does not support names',
    );
    for (const action of [
      () => storage.updateLink(9, 1, 1),
      () => storage.restoreLink(9, 1, 1),
      () => storage.deleteLink(9),
      () => storage.updateLinkObserved(9, 1, 1, observe),
      () => storage.deleteLinkObserved(9, observe),
    ]) {
      assert.throws(
        action,
        (error) =>
          error instanceof StorageError && error.message === 'Storage error: Link not found: 9',
      );
    }
  });

  it('adapts a names store and restores links under their names', () => {
    const store = namedMemory();
    const storage = new TransactionStorage(store);
    assert.equal(storage.named, true);
    const id = storage.createLinkObserved(0, 0, () => {});
    assert.equal(typeof storage.setLinkName(id, 'alpha'), 'number');
    assert.deepEqual([...storage.captureNames()], [[id, 'alpha']]);
    assert.equal(storage.setLinkName(id, null), 0);
    assert.deepEqual([...storage.captureNames()], []);
    assert.deepEqual(storage.restoreLink(id, 1, 1), { index: id, source: 0, target: 0 });
    assert.deepEqual(store.links.getLink(id), { index: id, source: 1, target: 1 });
    storage.flush();
    storage.reload();
    assert.equal(storage.hasExternalChanges(), false);
  });

  it('flushes file stores durably and reloads them', () => {
    const directory = scratchDirectory();
    const plainPath = join(directory, 'plain.links');
    const plain = new TransactionStorage(new LinkStorage(plainPath));
    plain.createLinkObserved(1, 1, () => {});
    plain.flush();
    assert.equal(readFileSync(plainPath, 'utf8'), '(1 1 1)\n');
    assert.equal(plain.hasExternalChanges(), false);
    writeFileSync(plainPath, '(1 1 1)\n(2 2 2)\n');
    assert.equal(plain.hasExternalChanges(), true);
    plain.reload();
    assert.equal(plain.linkExists(2), true);

    const namedPath = join(directory, 'named.links');
    const store = NamedTypesDecorator.open(namedPath);
    const named = new TransactionStorage(store);
    const id = named.createLinkObserved(0, 0, () => {});
    named.setLinkName(id, 'kept');
    named.flush();
    assert.ok(existsSync(join(directory, 'named.names.links')));
    named.reload();
    assert.equal(store.getName(id), 'kept');
    assert.equal(named.hasExternalChanges(), false);
  });

  it('reports a failed save as a storage error', () => {
    const directory = join(scratchDirectory(), 'locked');
    mkdirSync(directory);
    const storage = new TransactionStorage(new LinkStorage(join(directory, 'db.links')));
    storage.createLinkObserved(1, 1, () => {});
    chmodSync(directory, 0o500);
    try {
      assert.throws(
        () => storage.flush(),
        (error) =>
          error instanceof StorageError &&
          error.message ===
            `Storage error: Failed to create database: ${join(directory, 'db.links')}.saving: ` +
              'Permission denied (os error 13)',
      );
    } finally {
      chmodSync(directory, 0o700);
    }
  });
});

describe('NamedTransitionLog', () => {
  it('names one fresh link per entry and reads them by address', () => {
    const store = namedMemory();
    const log = transitionLogStore(store);
    assert.ok(log instanceof NamedTransitionLog);
    assert.equal(log.supportsDurableFlush(), false);
    log.appendLogEntry('first');
    store.create(5, 5);
    log.appendLogEntry('second');
    assert.deepEqual(log.readLogEntries(), ['first', 'second']);
    log.flushLog();
  });

  it('is durable over a file store', () => {
    const path = join(scratchDirectory(), 'db.transitions.links');
    const log = new NamedTransitionLog(NamedTypesDecorator.open(path));
    assert.equal(log.supportsDurableFlush(), true);
    log.appendLogEntry('entry');
    log.flushLog();
    assert.deepEqual(new NamedTransitionLog(NamedTypesDecorator.open(path)).readLogEntries(), [
      'entry',
    ]);
  });

  it('passes logs that already append through', () => {
    const log = memoryLog();
    assert.equal(transitionLogStore(log), log);
  });
});
