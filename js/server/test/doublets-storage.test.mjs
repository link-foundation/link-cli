// `DoubletsStorage` of doublets-storage.js, the port of
// rust/src/storage/doublets_storage.rs: opening a database file (with and
// without the sidecar lock), adopting a store, every `LinksStorage` method,
// the resolution decorators, flushing and change detection.
import assert from 'node:assert/strict';
import { existsSync, readFileSync, statSync, utimesSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, it } from 'node:test';

import { DoubletsStorage } from '../src/doublets-storage.js';
import { LinkError, LockError, NotFoundError, StorageError } from '../src/errors.js';
import { NO_REVISION } from '../src/link-storage.js';
import { lockBackend, LockMode } from '../src/storage-lock.js';
import { UnitStore } from '../src/unit-store.js';
import { AddressType, MIN_FILE_SIZE } from '../src/unit-store-memory.js';
import { CascadeUniquenessAndUsagesResolver } from '../src/unit-store-resolution.js';
import { scratchDirectory } from './helpers/storage.mjs';

const locks = { skip: lockBackend().unavailable };
const parts = (value) => [value.index, value.source, value.target];
const all = (storage) => storage.allLinks().map(parts);

/** A database path in a fresh scratch directory. */
const databasePath = (name = 'db.links') => join(scratchDirectory(), name);

/** Every `(before, after)` an observed write reports. */
function recorder() {
  const changes = [];
  return {
    changes,
    observer: (before, after) => {
      changes.push([parts(before), parts(after)]);
    },
  };
}

/** A file whose header cannot be read: its allocated count is above 2^53. */
function unreadable() {
  const path = databasePath();
  const bytes = Buffer.alloc(MIN_FILE_SIZE);
  bytes.writeBigUInt64LE(2n ** 60n, 0);
  writeFileSync(path, bytes);
  return path;
}

describe('opening a doublets storage', () => {
  it('creates the file and the directories above it', () => {
    const path = join(scratchDirectory(), 'nested', 'deeper', 'db.links');
    const storage = DoubletsStorage.open(path);
    assert.equal(storage.path(), path);
    assert.ok(storage.store() instanceof UnitStore);
    assert.equal(storage.heldLock(), null);
    assert.equal(statSync(path).size, 64 * 2 ** 20);
    storage.close();
    const narrow = DoubletsStorage.open(databasePath(), { addressType: AddressType.U32 });
    assert.equal(narrow.store().type, AddressType.U32);
    assert.equal(statSync(narrow.path()).size, 32 * 2 ** 20);
    narrow.close();
  });

  it('reports a parent that cannot be created', () => {
    const blocker = databasePath('file');
    writeFileSync(blocker, '');
    assert.throws(
      () => DoubletsStorage.open(join(blocker, 'db.links')),
      (error) => {
        assert.ok(error instanceof LinkError);
        assert.match(error.message, /^I\/O error: /);
        return true;
      },
    );
  });

  it('refuses a directory in place of the database file', () => {
    assert.throws(() => DoubletsStorage.open(scratchDirectory()), {
      name: 'LinkError',
      message: /^I\/O error: Is a directory/,
    });
  });

  it('reports a file it cannot read', () => {
    assert.throws(() => DoubletsStorage.open(unreadable()), { name: 'AddressOverflowError' });
  });

  it('holds a shared or an exclusive lock until it is closed', locks, () => {
    const path = databasePath();
    const shared = DoubletsStorage.openShared(path);
    assert.equal(shared.heldLock().mode(), LockMode.Shared);
    const other = DoubletsStorage.openShared(path);
    assert.equal(DoubletsStorage.tryOpenExclusive(path), null);
    shared.close();
    other.close();
    const exclusive = DoubletsStorage.tryOpenExclusive(path);
    assert.equal(exclusive.heldLock().mode(), LockMode.Exclusive);
    assert.equal(DoubletsStorage.tryOpenExclusive(path), null);
    exclusive.close();
    exclusive.close();
    const again = DoubletsStorage.openExclusive(path);
    assert.equal(again.heldLock().mode(), LockMode.Exclusive);
    again.close();
  });

  it('releases the lock when the file cannot be read', locks, () => {
    const path = unreadable();
    assert.throws(() => DoubletsStorage.openExclusive(path), { name: 'AddressOverflowError' });
    const lock = DoubletsStorage.tryOpenExclusive(databasePath());
    assert.notEqual(lock, null);
    lock.close();
    writeFileSync(path, '');
    DoubletsStorage.tryOpenExclusive(path).close();
  });

  it('takes the sidecar lock on demand', locks, () => {
    const storage = DoubletsStorage.open(databasePath());
    const shared = storage.lockShared();
    assert.equal(shared.mode(), LockMode.Shared);
    shared.release();
    const exclusive = storage.lockExclusive();
    assert.equal(exclusive.mode(), LockMode.Exclusive);
    exclusive.release();
    storage.close();
  });
});

describe('adopting a store', () => {
  it('without a file has no lock, flush target or changes', () => {
    const storage = DoubletsStorage.wrap(UnitStore.inMemory());
    assert.equal(storage.path(), null);
    assert.throws(() => storage.lockShared(), LockError);
    assert.throws(() => storage.lockExclusive(), {
      message: 'Lock error: this doublets storage is not backed by a known file',
    });
    assert.equal(storage.createLink(1, 1), 1);
    storage.flush();
    assert.equal(storage.hasExternalChanges(), false);
    storage.reload();
    assert.deepEqual(all(storage), [[1, 1, 1]]);
    storage.close();
  });

  it('at a path flushes and watches that file', () => {
    const path = databasePath();
    const store = UnitStore.open(path);
    const storage = DoubletsStorage.wrapAt(store, path);
    assert.equal(storage.path(), path);
    assert.notDeepEqual(storage.knownRevision, NO_REVISION);
    storage.createLink(1, 1);
    storage.flush();
    assert.equal(storage.hasExternalChanges(), false);
    storage.close();
  });

  it('maps the store, moving the lock, the path and the revision', locks, () => {
    const path = databasePath();
    const storage = DoubletsStorage.openExclusive(path);
    const lock = storage.heldLock();
    const resolved = storage.withAutomaticUniquenessAndUsagesResolution();
    assert.ok(resolved.store() instanceof CascadeUniquenessAndUsagesResolver);
    assert.equal(resolved.heldLock(), lock);
    assert.equal(resolved.path(), path);
    assert.equal(resolved.owned, storage.store().memory);
    assert.equal(storage.heldLock(), null);
    assert.equal(storage.owned, null);
    const mapped = resolved.mapStore((store) => store);
    assert.equal(mapped.heldLock(), lock);
    mapped.close();
    assert.equal(lock.released, true);
  });
});

describe('the links of a doublets storage', () => {
  it('are created, read, updated and deleted', () => {
    const storage = DoubletsStorage.open(databasePath());
    assert.equal(storage.createLink(1, 1), 1);
    assert.equal(storage.createLink(1, 2), 2);
    assert.equal(storage.getOrCreateLink(1, 2), 2);
    assert.equal(storage.getOrCreateLink(2, 1), 3);
    assert.deepEqual(parts(storage.getLink(2)), [2, 1, 2]);
    assert.equal(storage.getLink(9), null);
    assert.equal(storage.linkExists(3), true);
    assert.equal(storage.linkExists(4), false);
    assert.throws(() => storage.existingLink(4), NotFoundError);
    assert.deepEqual(parts(storage.updateLink(3, 3, 3)), [3, 2, 1]);
    assert.deepEqual(parts(storage.restoreLink(3, 2, 1)), [3, 3, 3]);
    assert.throws(() => storage.updateLink(7, 1, 1), { message: 'Link not found: 7' });
    assert.equal(storage.searchLink(2, 1), 3);
    assert.equal(storage.searchLink(3, 3), null);
    assert.deepEqual(storage.queryLinks().map(parts), all(storage));
    assert.deepEqual(storage.queryLinks(null, 1).map(parts), [
      [2, 1, 2],
      [1, 1, 1],
    ]);
    assert.deepEqual(storage.queryLinks(null, null, 1).map(parts), [
      [3, 2, 1],
      [1, 1, 1],
    ]);
    assert.deepEqual(storage.queryLinks(2).map(parts), [[2, 1, 2]]);
    assert.deepEqual(parts(storage.deleteLink(2)), [2, 1, 2]);
    assert.throws(() => storage.deleteLink(2), NotFoundError);
    assert.equal(storage.linksCount(), 2);
    assert.equal(storage.captureNames(), null);
    assert.throws(() => storage.setLinkName(1, 'one'), {
      name: 'StorageError',
      message: 'Storage error: storage does not support names',
    });
    storage.close();
  });

  it('report every change of an observed write', () => {
    const storage =
      DoubletsStorage.open(databasePath()).withAutomaticUniquenessAndUsagesResolution();
    const created = recorder();
    assert.equal(storage.createLinkObserved(0, 0, created.observer), 1);
    assert.deepEqual(created.changes, [
      [
        [0, 0, 0],
        [1, 0, 0],
      ],
      [
        [1, 0, 0],
        [1, 0, 0],
      ],
    ]);
    storage.createLink(1, 1);
    storage.createLink(2, 1);
    const updated = recorder();
    assert.deepEqual(parts(storage.updateLinkObserved(3, 1, 1, updated.observer)), [3, 2, 1]);
    assert.deepEqual(updated.changes.at(-1), [
      [2, 1, 1],
      [2, 1, 1],
    ]);
    assert.deepEqual(all(storage), [
      [1, 0, 0],
      [2, 1, 1],
    ]);
    const deleted = recorder();
    assert.deepEqual(parts(storage.deleteLinkObserved(1, deleted.observer)), [1, 0, 0]);
    assert.deepEqual(deleted.changes, [
      [
        [2, 1, 1],
        [2, 0, 0],
      ],
      [
        [2, 0, 0],
        [0, 0, 0],
      ],
      [
        [1, 0, 0],
        [0, 0, 0],
      ],
    ]);
    assert.throws(() => storage.updateLinkObserved(1, 1, 1, () => {}), NotFoundError);
    assert.throws(() => storage.deleteLinkObserved(1, () => {}), NotFoundError);
    storage.close();
  });

  it('are reserved up to an address, reusing the free ones', () => {
    const storage = DoubletsStorage.wrap(UnitStore.inMemory());
    assert.equal(storage.ensureLinkCreated(3), 3);
    assert.equal(storage.ensureLinkCreated(3), 3);
    storage.deleteLink(1);
    storage.deleteLink(2);
    const { changes, observer } = recorder();
    assert.equal(storage.ensureLinkCreatedObserved(4, observer), 4);
    assert.deepEqual(changes, [
      [
        [0, 0, 0],
        [1, 0, 0],
      ],
      [
        [0, 0, 0],
        [2, 0, 0],
      ],
      [
        [0, 0, 0],
        [4, 0, 0],
      ],
    ]);
    storage.updateLink(4, 4, 4);
    assert.throws(() => storage.ensureLinkCreated(0), {
      name: 'StorageError',
      message: 'Storage error: could not reserve link address 0: the store allocated 5 instead',
    });
    const failed = recorder();
    assert.throws(() => storage.ensureLinkCreatedObserved(0, failed.observer), StorageError);
    assert.deepEqual(failed.changes, [
      [
        [0, 0, 0],
        [6, 0, 0],
      ],
    ]);
  });

  it('report the links a failed reservation changed or deleted', () => {
    const storage = DoubletsStorage.wrap(UnitStore.inMemory());
    storage.createLink(1, 1);
    storage.createLink(2, 2);
    storage.createLink(3, 3);
    const store = storage.store();
    store.create = () => {
      store.update(2, 5, 5);
      store.delete(1);
      return 9;
    };
    const { changes, observer } = recorder();
    assert.throws(() => storage.ensureLinkCreatedObserved(5, observer), {
      message: 'Storage error: could not reserve link address 5: the store allocated 9 instead',
    });
    assert.deepEqual(changes, [
      [
        [1, 1, 1],
        [0, 0, 0],
      ],
      [
        [2, 2, 2],
        [2, 5, 5],
      ],
    ]);
  });
});

describe('flushing a doublets storage', () => {
  it('syncs the file and publishes a new revision', () => {
    const path = databasePath();
    const storage = DoubletsStorage.open(path);
    const opened = storage.knownRevision;
    storage.createLink(1, 1);
    storage.flush();
    assert.equal(storage.hasExternalChanges(), false);
    assert.notDeepEqual(storage.knownRevision, opened);
    assert.equal(readFileSync(path).readUInt32LE(64), 1);
    storage.close();
  });

  it('publishes a later revision even within the millisecond of the last one', () => {
    const path = databasePath();
    const storage = DoubletsStorage.open(path);
    // Another holder published a revision this clock has not reached yet.
    const ahead = new Date(Date.now() + 60_000);
    utimesSync(path, ahead, ahead);
    const published = statSync(path, { bigint: true }).mtimeNs;
    storage.flush();
    assert.ok(storage.knownRevision.modified > published);
    storage.close();
  });

  it('sees what another storage published and reloads it', () => {
    const path = databasePath();
    const first = DoubletsStorage.open(path);
    const second = DoubletsStorage.open(path);
    second.createLink(5, 5);
    second.flush();
    assert.equal(first.hasExternalChanges(), true);
    first.reload();
    assert.equal(first.hasExternalChanges(), false);
    assert.deepEqual(all(first), [[1, 5, 5]]);
    first.close();
    second.close();
  });

  it('keeps the file when it is closed without a flush', () => {
    const path = databasePath();
    const storage = DoubletsStorage.open(path);
    storage.createLink(2, 2);
    storage.close();
    assert.ok(existsSync(path));
    const reopened = DoubletsStorage.open(path);
    assert.deepEqual(all(reopened), [[1, 2, 2]]);
    reopened.close();
  });
});
