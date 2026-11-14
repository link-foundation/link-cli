// A `LinksStorage` backed by a real doublets unit store: `DoubletsStorage`
// of rust/src/storage/doublets_storage.rs, with the provided methods of
// `LinksStorage` (rust/src/storage/traits.rs) it does not override.
//
// `DoubletsStorage.open` and friends open the database file as a
// `FileMappedUnitStore` (unit-store.js) that is byte for byte the one Rust
// writes: the links are changed in place, every operation is written through
// before it returns, the file only grows and is never renamed, so a Rust or
// JavaScript process holding the same file keeps a valid view of it.
// `DoubletsStorage.wrap` adopts a store the caller already owns.
//
// The method names are those of the `TransactionStorage` seam of
// transactions-storage.js, so a `TransactionsDecorator` can write through
// this storage directly.
//
// A doublets store has no concurrency control of its own: serialise writers
// with `openExclusive`, `openShared` or the `lockShared` / `lockExclusive`
// guards, and use `hasExternalChanges` to find out whether somebody else
// published a write since the last flush or reload.
import { closeSync, existsSync, fstatSync, futimesSync, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';

import { LockError, NotFoundError, StorageError } from './errors.js';
import { NULL_LINK, sameLink } from './link.js';
import { NO_REVISION, sameRevision, storageRevision } from './link-storage.js';
import { openFileSync } from './open-file.js';
import { FileLock, LockMode, lockFilePath } from './storage-lock.js';
import { io, syncPath } from './transactions-storage.js';
import { UnitStore } from './unit-store.js';
import { AddressType } from './unit-store-memory.js';
import { withAutomaticUniquenessAndUsagesResolution } from './unit-store-resolution.js';

const notBackedByFile = () => new LockError('this doublets storage is not backed by a known file');

/** Every row of `storage` by address, as the `BTreeMap` of the Rust default. */
const rowsByIndex = (storage) =>
  new Map(
    storage
      .allLinks()
      .sort((left, right) => left.index - right.index)
      .map((row) => [row.index, row]),
  );

/** A `LinksStorage` over a doublets store (a `UnitStore` or a decorator of one). */
export class DoubletsStorage {
  /**
   * Use the static constructors. `owned` is the memory of a store this
   * storage opened itself (the Rust `mapped_flush`), synchronised by `flush`.
   */
  constructor(store, { path = null, revision = NO_REVISION, lock = null, owned = null } = {}) {
    this.inner = store;
    this.storePath = path;
    this.knownRevision = revision;
    this.fileLock = lock;
    this.owned = owned;
  }

  /**
   * Opens (or creates) the database file `path` without a lock.
   * `addressType` is `AddressType.U64` (`usize`, the default) or
   * `AddressType.U32`.
   */
  static open(path, options) {
    return DoubletsStorage.openMapped(path, null, options);
  }

  /** Opens the database holding a **shared** advisory lock until `close`. */
  static openShared(path, options) {
    return DoubletsStorage.openMapped(
      path,
      FileLock.acquire(lockFilePath(path), LockMode.Shared),
      options,
    );
  }

  /** Opens the database holding an **exclusive** advisory lock until `close`. */
  static openExclusive(path, options) {
    return DoubletsStorage.openMapped(
      path,
      FileLock.acquire(lockFilePath(path), LockMode.Exclusive),
      options,
    );
  }

  /** Like `openExclusive`, but `null` instead of blocking while the lock is held. */
  static tryOpenExclusive(path, options) {
    const lock = FileLock.tryAcquire(lockFilePath(path), LockMode.Exclusive);
    return lock === null ? null : DoubletsStorage.openMapped(path, lock, options);
  }

  static openMapped(path, lock, { addressType = AddressType.U64 } = {}) {
    try {
      const parent = dirname(path);
      if (!existsSync(parent)) io(() => mkdirSync(parent, { recursive: true }));
      const store = io(() => UnitStore.open(path, { addressType }));
      return new DoubletsStorage(store, {
        path,
        revision: storageRevision(path),
        lock,
        owned: store.memory,
      });
    } catch (error) {
      lock?.release();
      throw error;
    }
  }

  /** Adopts a store the caller owns: no path, no lock, no change detection. */
  static wrap(store) {
    return new DoubletsStorage(store);
  }

  /** Adopts a store backed by the file `path`, for `flush`, locks and change detection. */
  static wrapAt(store, path) {
    return new DoubletsStorage(store, { path, revision: storageRevision(path) });
  }

  /**
   * A storage over `map(store)` that takes over the path, lock, flushed
   * memory and revision of this one, which must not be used afterwards.
   */
  mapStore(map) {
    const mapped = new DoubletsStorage(map(this.inner), {
      path: this.storePath,
      revision: this.knownRevision,
      lock: this.fileLock,
      owned: this.owned,
    });
    this.fileLock = null;
    this.owned = null;
    return mapped;
  }

  /**
   * The store wrapped in the decorators C# applies by default: `(source,
   * target)` pairs stay unique, and a deletion cascades to the usages.
   */
  withAutomaticUniquenessAndUsagesResolution() {
    return this.mapStore(withAutomaticUniquenessAndUsagesResolution);
  }

  /** The database file backing this storage, or `null`. */
  path() {
    return this.storePath;
  }

  /** The underlying doublets store. */
  store() {
    return this.inner;
  }

  /** Acquires a shared advisory lock on the sidecar lock file. */
  lockShared() {
    return FileLock.acquire(this.requireLockPath(), LockMode.Shared);
  }

  /** Acquires an exclusive advisory lock on the sidecar lock file. */
  lockExclusive() {
    return FileLock.acquire(this.requireLockPath(), LockMode.Exclusive);
  }

  /** The lock held until `close`, or `null`. */
  heldLock() {
    return this.fileLock;
  }

  requireLockPath() {
    if (this.storePath === null) throw notBackedByFile();
    return lockFilePath(this.storePath);
  }

  refreshRevision() {
    if (this.storePath !== null) this.knownRevision = storageRevision(this.storePath);
  }

  /** Releases the lock and closes the store. Unlike `flush`, nothing is `fsync`ed. */
  close() {
    this.fileLock?.release();
    this.fileLock = null;
    this.owned = null;
    this.inner.close();
  }

  // ----- LinksStorage ----------------------------------------------------

  createLink(source, target) {
    return this.inner.createLink(source, target);
  }

  /** `create_link_observed`: every change reported, the last address returned. */
  createLinkObserved(source, target, observer) {
    let index = 0;
    this.inner.createLinkWith(source, target, (before, after) => {
      index = after.index;
      observer(before, after);
    });
    return index;
  }

  /**
   * Creates empty links until `index` is one: the store hands out the
   * lowest free address first, so this walks up to (and reuses) `index`.
   */
  ensureLinkCreated(index) {
    if (this.linkExists(index)) return index;
    for (;;) {
      const created = this.inner.create();
      if (created === index) return index;
      if (created > index) {
        throw new StorageError(
          `could not reserve link address ${index}: the store allocated ${created} instead`,
        );
      }
    }
  }

  /** Reports every row `ensureLinkCreated` changed, even when it fails. */
  ensureLinkCreatedObserved(index, observer) {
    const before = rowsByIndex(this);
    let failure = null;
    let outcome;
    try {
      outcome = this.ensureLinkCreated(index);
    } catch (error) {
      failure = error;
    }
    const after = rowsByIndex(this);
    for (const [id, previous] of before) {
      const next = after.get(id) ?? NULL_LINK;
      after.delete(id);
      if (!sameLink(previous, next)) observer(previous, next);
    }
    for (const row of after.values()) observer(NULL_LINK, row);
    if (failure !== null) throw failure;
    return outcome;
  }

  /** The link `index`, or `null`. */
  getLink(index) {
    return this.inner.getLink(index);
  }

  linkExists(index) {
    return this.getLink(index) !== null;
  }

  existingLink(index) {
    const before = this.getLink(index);
    if (before === null) throw new NotFoundError(index);
    return before;
  }

  /** Sets the source and target of `index`, returning the link before. */
  updateLink(index, source, target) {
    const before = this.existingLink(index);
    this.inner.update(index, source, target);
    return before;
  }

  /** `restore_link`: the store has no resolution of its own to skip. */
  restoreLink(index, source, target) {
    return this.updateLink(index, source, target);
  }

  /** Deletes `index`, returning the link before. */
  deleteLink(index) {
    const before = this.existingLink(index);
    this.inner.delete(index);
    return before;
  }

  /** `updateLink`, reporting every `(before, after)` change, cascades included. */
  updateLinkObserved(index, source, target, observer) {
    const before = this.existingLink(index);
    this.inner.updateWith(index, source, target, observer);
    return before;
  }

  /** `deleteLink`, reporting every `(before, after)` change, cascades included. */
  deleteLinkObserved(index, observer) {
    const before = this.existingLink(index);
    this.inner.deleteWith(index, observer);
    return before;
  }

  /** Every link in address order. */
  allLinks() {
    return this.inner.eachIter([]);
  }

  /** The links matching the pattern; `null` matches anything. */
  queryLinks(index = null, source = null, target = null) {
    const { any } = this.inner;
    return this.inner.eachIter([index ?? any, source ?? any, target ?? any]);
  }

  /** The address of the link `(source, target)`, or `null`. */
  searchLink(source, target) {
    return this.inner.search(source, target);
  }

  getOrCreateLink(source, target) {
    return this.inner.getOrCreate(source, target);
  }

  linksCount() {
    return this.inner.count();
  }

  /** A doublets store has no names. */
  captureNames() {
    return null;
  }

  setLinkName() {
    throw new StorageError('storage does not support names');
  }

  /**
   * Makes every write durable, then publishes it by advancing the
   * modification time of the file, so other holders see a new revision.
   */
  flush() {
    if (this.owned !== null) io(() => this.owned.sync());
    if (this.storePath === null) return;
    if (this.owned === null) syncPath(this.storePath, 'r+');
    io(() => {
      const fd = openFileSync(this.storePath, 'r+');
      try {
        futimesSync(fd, fstatSync(fd).atime, new Date());
      } finally {
        closeSync(fd);
      }
    });
    this.refreshRevision();
  }

  /** Whether the file changed since this storage opened, flushed or reloaded it. */
  hasExternalChanges() {
    if (this.storePath === null) return false;
    return !sameRevision(storageRevision(this.storePath), this.knownRevision);
  }

  /** Reads the file afresh and refreshes the known revision. */
  reload() {
    this.inner.reload();
    this.refreshRevision();
  }
}
