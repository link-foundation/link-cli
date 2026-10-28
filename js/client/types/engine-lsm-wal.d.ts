import type { SortedJournalEntry } from './engine-sorted-table.js';
export interface LsmOptions {
  /** Total retained logical key capacity, 1..65536, default 4096. */
  maxKeys?: number;
  /** Logical UTF-8 capacity, 1..8 MiB, default 8 MiB. */
  maxBytes?: number;
  /** 1..maxKeys, default 64. */
  bufferKeys?: number;
  /** 1..maxBytes, default 1 MiB. */
  bufferBytes?: number;
  /** 1..64, default 4. */
  maxRuns?: number;
  /** 1..65536, default 1024. */
  maxWalRecords?: number;
  /** Encoded UTF-8 WAL admission, 1..64 MiB, default 16 MiB. */
  maxWalBytes?: number;
}
export interface LsmTableDescriptor {
  id: string;
  /** Exactly 32 lowercase hexadecimal characters. */
  identity: string;
  digest: string;
  count: number;
  storedBytes: number;
}
export interface LsmWalState {
  generation: string;
  coveredGeneration: string;
  lastReservedTableId: string;
  manifest: LsmTableDescriptor[];
  buffer: SortedJournalEntry[];
}
/** Normalizes and validates explicit finite limits without storage I/O. */
export function lsmOptions(options?: LsmOptions): Required<LsmOptions>;
/** Pure, owned semantic replay. Physical tables and combined logical capacity
 * require separate validation before a physical store may expose this state.
 * Providers must enforce their own allocation bounds before returning records. */
export class LsmWal {
  static recover(records: readonly string[], options?: LsmOptions): LsmWalState;
  static encode(state: LsmWalState, options?: LsmOptions): string;
}
