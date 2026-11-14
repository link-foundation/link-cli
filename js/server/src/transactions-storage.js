// The storage seam of the transactions layer: the `LinksStorage` adapter of
// rust/src/storage/decorator_impls.rs over a `LinkStorage` or a
// `NamedTypesDecorator`, and the links-backed transitions log of
// rust/src/transactions/log.rs (`impl TransitionLogStore for NamedTypesDecorator`).
//
// Neither JS store has the Rust `flush`, so the adapter does what it does:
// save, `fsync` the database file and its directory, then remember the
// revision it wrote.
import { closeSync, fsyncSync, openSync } from 'node:fs';
import { dirname } from 'node:path';

import { errorChain, errorText, ioErrorText, isSystemError } from './error-report.js';
import { LinkError, StorageError } from './errors.js';
import { NULL_LINK } from './link.js';

/** `storage_error`: an `anyhow` error as `LinkError::StorageError("{error:#}")`. */
export const storageError = (error) =>
  new StorageError(errorChain(error).map(errorText).join(': '));

/** The `LinkError::Io` a `?` on an `io::Error` makes; other errors pass through. */
export const ioError = (error) =>
  isSystemError(error)
    ? new LinkError(`I/O error: ${ioErrorText(error)}`, { cause: error })
    : error;

/** Runs `action`, reporting its failure as `storage_error` does. */
export function guarded(action) {
  try {
    return action();
  } catch (error) {
    throw storageError(error);
  }
}

/** Runs `action`, reporting a system error as `LinkError::Io`. */
export function io(action) {
  try {
    return action();
  } catch (error) {
    throw ioError(error);
  }
}

/** `File::open(path)?.sync_all()?` with `flags`. */
export function syncPath(path, flags = 'r') {
  io(() => {
    const fd = openSync(path, flags);
    try {
      fsyncSync(fd);
    } finally {
      closeSync(fd);
    }
  });
}

/** The directory `Path::parent` names for a file, `.` when it has none. */
export const parentDirectory = (path) => dirname(path);

/**
 * `File::open(parent)?.sync_all()?` on the directory of `path`. Rust does it
 * only under `#[cfg(unix)]`: Windows opens no directory as a file.
 */
export function syncParentDirectory(path, platform = process.platform) {
  if (platform !== 'win32') syncPath(parentDirectory(path));
}

/** Whether `store` is a `NamedTypesDecorator` (it has the logical names). */
const isNamed = (store) => typeof store.logicalNames === 'function';

/** The `flush` of one `LinkStorage`: save, then make the file and directory durable. */
function flushLinkStorage(storage) {
  guarded(() => storage.save());
  if (!storage.isInMemory()) {
    syncPath(storage.databasePath(), 'r+');
    syncParentDirectory(storage.databasePath());
  }
  storage.refreshObservedRevision();
}

/**
 * The `LinksStorage<u32>` view the transactions layer writes through: a
 * `LinkStorage` or a `NamedTypesDecorator` (`store`).
 */
export class TransactionStorage {
  constructor(store) {
    this.store = store;
    this.named = isNamed(store);
  }

  /** `create_link_observed`: the created link, reported as a creation. */
  createLinkObserved(source, target, observer) {
    const index = this.store.create(source, target);
    observer(NULL_LINK, this.store.getLink(index));
    return index;
  }

  ensureLinkCreated(index) {
    return this.store.ensureCreated(index);
  }

  /** Only the requested address is reported; placeholders come and go. */
  ensureLinkCreatedObserved(index, observer) {
    const existing = this.store.exists(index);
    const created = this.store.ensureCreated(index);
    if (index !== 0 && !existing) observer(NULL_LINK, this.store.getLink(index));
    return created;
  }

  getLink(index) {
    return this.store.getLink(index) ?? null;
  }

  linkExists(index) {
    return this.store.exists(index);
  }

  updateLink(index, source, target) {
    return guarded(() => this.store.update(index, source, target));
  }

  /** Writes a state back without the uniqueness resolution (`update_raw`). */
  restoreLink(index, source, target) {
    const raw = this.named ? this.store.links : this.store;
    return guarded(() => raw.updateRaw(index, source, target));
  }

  deleteLink(index) {
    return guarded(() => this.store.delete(index));
  }

  updateLinkObserved(index, source, target, observer) {
    return guarded(() => this.store.updateObserved(index, source, target, observer));
  }

  deleteLinkObserved(index, observer) {
    return guarded(() => this.store.deleteObserved(index, observer));
  }

  allLinks() {
    return this.store.allLinks();
  }

  queryLinks(index = null, source = null, target = null) {
    return this.store.queryLinks(index, source, target);
  }

  searchLink(source, target) {
    return this.store.search(source, target);
  }

  flush() {
    if (this.named) {
      flushLinkStorage(this.store.links);
      flushLinkStorage(this.store.namesLinks);
    } else flushLinkStorage(this.store);
  }

  hasExternalChanges() {
    return this.store.hasExternalChanges();
  }

  reload() {
    if (this.named) {
      guarded(() => this.store.links.reloadFromDisk());
      guarded(() => this.store.namesLinks.reloadFromDisk());
    } else guarded(() => this.store.reloadFromDisk());
  }

  /** Every logical name by link, or `null` for a store without names. */
  captureNames() {
    return this.named ? guarded(() => this.store.logicalNames()) : null;
  }

  /** Sets (`name`) or removes (`null`) one name without recording it. */
  setLinkName(index, name) {
    if (!this.named) throw new StorageError('storage does not support names');
    if (name !== null) return guarded(() => this.store.setName(index, name));
    guarded(() => this.store.removeName(index));
    return 0;
  }
}

/** The links-backed transitions log: every entry names a fresh link. */
export class NamedTransitionLog {
  constructor(store) {
    this.store = store;
    this.storage = new TransactionStorage(store);
  }

  supportsDurableFlush() {
    return this.store.links.databasePath() !== '';
  }

  appendLogEntry(entry) {
    const created = this.store.create(0, 0);
    guarded(() => this.store.setName(created, entry));
  }

  readLogEntries() {
    const addresses = this.store
      .allLinks()
      .map((value) => value.index)
      .sort((left, right) => left - right);
    const entries = [];
    for (const address of addresses) {
      const name = guarded(() => this.store.getName(address));
      if (name !== null) entries.push(name);
    }
    return entries;
  }

  flushLog() {
    this.storage.flush();
  }
}

/** A `TransitionLogStore`: a log with the three methods, or a links store to wrap. */
export const transitionLogStore = (log) =>
  typeof log.appendLogEntry === 'function' ? log : new NamedTransitionLog(log);
