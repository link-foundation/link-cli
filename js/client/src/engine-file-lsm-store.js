// Optional native facade. Native imports stay out of portable/root entries.
import { CheckpointPagedCommitLog } from './engine-checkpoint-pages.js';
import { PhysicalLsmJournalStore } from './engine-lsm-store.js';
import { lsmOptions } from './engine-lsm-wal.js';
import { FileLsmFiles } from './engine-file-lsm-files.js';
import { readOnlyWal } from './engine-file-lsm-readonly.js';

export class FileLsmJournalStore {
  #files; #store; #closed = false;
  constructor(directory, { mirroredBanks = false, mirroredRoot = false, mirroredTables = false, maxPages = 16384, ...selected } = {}) {
    const options = { ...lsmOptions(selected), maxNamespaceEntries: selected.maxNamespaceEntries ?? 256,
      mirroredBanks, mirroredRoot, mirroredTables, maxPages };
    if ([mirroredBanks, mirroredRoot, mirroredTables].some(value => typeof value !== 'boolean')) throw TypeError('boolean physical modes');
    if (!Number.isSafeInteger(maxPages) || maxPages < 1 || maxPages > 65536) throw RangeError('finite native LSM page limit');
    if (!Number.isSafeInteger(options.maxNamespaceEntries) || options.maxNamespaceEntries < 2 || options.maxNamespaceEntries > 65536)
      throw RangeError('finite native LSM namespace limit');
    this.#files = new FileLsmFiles(directory, options);
    try {
      const names = this.#files.list(options.maxNamespaceEntries), readonly = this.#files.wal(true);
      const records = CheckpointPagedCommitLog.inspect(readonly.root, readonly.banks, { maxPages });
      if (records.length === 0) {
        if (names.length) throw Error('uninitialized namespace contains tables');
      } else {
        const validation = PhysicalLsmJournalStore.open(readOnlyWal(records), this.#files.provider(true), options);
        validation.close();
      }
      // Drop every temporary lease and staged mirror before acquiring devices
      // whose constructors can create files or complete physical recovery.
      this.#files.closeReaders();
      const writable = this.#files.wal(false);
      const wal = new CheckpointPagedCommitLog(writable.root, writable.banks, { maxPages });
      this.#store = PhysicalLsmJournalStore.open(wal, this.#files.provider(false), options);
    } catch (cause) {
      try { this.close(); }
      catch (cleanup) { throw new AggregateError([cause, cleanup], 'native LSM open and cleanup failed'); }
      throw cause;
    }
  }
  #check() { if (this.#closed) throw Error('native LSM is closed'); }
  get supportsDurableFlush() { this.#check(); return this.#store.supportsDurableFlush; }
  read(keys) { this.#check(); return this.#store.read(keys); }
  compareExchange(...args) { this.#check(); return this.#store.compareExchange(...args); }
  compareDelete(...args) { this.#check(); return this.#store.compareDelete(...args); }
  scanNamespace(...args) { this.#check(); return this.#store.scanNamespace(...args); }
  flush() { this.#check(); return this.#store.flush(); }
  compact() { this.#check(); return this.#store.compact(); }
  close() {
    if (this.#closed) { this.#files.close(); return; }
    this.#closed = true; const errors = [];
    try { this.#store?.close(); } catch (error) { errors.push(error); }
    this.#files.close(errors);
  }
  [Symbol.dispose]() { this.close(); }
}
