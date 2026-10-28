import type { PageDevice } from './engine-paged-log.js';
export interface SortedJournalEntry {
  key: string;
  /** Positive canonical decimal u64; tombstones retain their generation. */
  generation: string;
  /** Null is a tombstone; [] is a live empty journal. */
  entries: readonly string[] | null;
}
export interface SortedTableOptions {
  /** 1..65536, default 4096. */
  maxKeys?: number;
  /** Total encoded UTF-8 record bytes including marker. 1..64 MiB, default 8 MiB. */
  maxBytes?: number;
}
/** Immutable exclusive caller-owned native pages, fully validated on open.
 * Retains keys, physical record addresses and hashes, without a payload map.
 * Missing point reads perform no page I/O. Does not close the supplied device. */
export class SortedJournalTable {
  private constructor();
  static create(device: PageDevice, entries: readonly SortedJournalEntry[], options?: SortedTableOptions): SortedJournalTable;
  static open(device: PageDevice, options?: SortedTableOptions): SortedJournalTable;
  readonly count: number;
  readonly storedBytes: number;
  readonly identity: Uint8Array;
  /** Exact canonical decimal u64 hash. */
  readonly digest: string;
  readonly supportsDurableFlush: boolean;
  get(key: string): SortedJournalEntry | undefined;
  entries(): SortedJournalEntry[];
}
