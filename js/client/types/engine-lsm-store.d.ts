import type { JournalVersion, JournalNamespaceEntry, JournalScanOptions, SynchronousJournalMaintenanceStore } from './index.js';
import type { PageDevice } from './engine-paged-log.js';
import type { ReplaceableCommitLog } from './engine-checkpoint-pages.js';
import type { LsmOptions } from './engine-lsm-wal.js';

/** Exclusive synchronous namespace. List returns all canonical positive decimal
 * u64 IDs in numeric order or fails within its bound. Open never creates files;
 * create never reuses names. Successful create/delete persist directory entries.
 * Close releases a lease without committing staged repair and permits retry.
 * The caller owns this provider and the WAL. */
export interface LsmTableNamespace {
  readonly supportsDurableFlush: boolean;
  list(maxEntries: number): string[];
  create(id: string): PageDevice;
  open(id: string): PageDevice;
  close(device: PageDevice): void;
  delete(id: string): void;
}
export interface LsmStoreOptions extends LsmOptions {
  /** Complete namespace admission, 2..65536, default 256. */
  maxNamespaceEntries?: number;
}
/** Optional bounded physical LSM. Operations are synchronous; async consumers
 * may await their results. Owns table leases and indexes only disk metadata.
 * Foreground flush/compaction publishes immutable tables through the WAL.
 * Unknown mutations fence until actual reopen. Close can retry failed cleanup.
 * Neither close nor constructor cleanup closes the supplied WAL/namespace. */
export class PhysicalLsmJournalStore implements SynchronousJournalMaintenanceStore {
  private constructor();
  static open(wal: ReplaceableCommitLog, namespace: LsmTableNamespace, options?: LsmStoreOptions): PhysicalLsmJournalStore;
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
