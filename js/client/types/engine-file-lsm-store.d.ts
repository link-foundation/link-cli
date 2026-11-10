import type { JournalVersion, JournalNamespaceEntry, JournalScanOptions, SynchronousJournalMaintenanceStore } from './index.js';
import type { LsmStoreOptions } from './engine-lsm-store.js';

export interface FileLsmOptions extends LsmStoreOptions {
  /** WAL bank page bound, 1..65536, default 16384; bounds discovery allocation.
   * Physical page admission also includes padding at each acknowledged flush. */
  maxPages?: number;
  mirroredBanks?: boolean;
  mirroredRoot?: boolean;
  mirroredTables?: boolean;
}
/** Native synchronous bounded LSM under one exclusive .lsm-writer directory owner.
 * Parent directory must exist and contain only this store. Modes cannot change
 * in place. Discovery validates all selected state read-only before constructor
 * repair, truncation, initialization or retirement. Close attempts every child
 * before releasing the outer fence. Uncertain handle cleanup retains that fence;
 * marker-removal errors can occur after every handle is closed.
 * Crash recovery requires external fencing before explicit marker removal.
 * Durable namespace capability is false on Windows; see the native LSM contract. */
export class FileLsmJournalStore implements SynchronousJournalMaintenanceStore {
  constructor(directory: string, options?: FileLsmOptions);
  readonly supportsDurableFlush: boolean;
  read(keys: readonly string[]): JournalVersion[];
  compareExchange(keys: readonly string[], expected: readonly JournalVersion[], updates: readonly (readonly string[] | null)[]): boolean;
  compareDelete(key: string, expectedGeneration: string): boolean;
  scanNamespace(options?: JournalScanOptions): JournalNamespaceEntry[];
  flush(): void;
  compact(): void;
  close(): void;
  [Symbol.dispose](): void;
}
