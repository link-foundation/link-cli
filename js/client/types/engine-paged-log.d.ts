import type { CommitLog } from './index.js';
export interface PagedLogOptions {
  /** Positive safe page limit; default floor((2^53 - 1) / 4096). */
  maxPages?: number;
}
/** Exclusive caller-owned synchronous device. Successful synchronization must
 * persist preceding writes. Failed mutations require reopening the log. */
export interface PageDevice {
  readonly supportsDurableFlush: boolean;
  readonly dataLength: number;
  newDatabaseId(): Uint8Array;
  readMetadata(slot: number): Uint8Array | undefined;
  writeMetadata(slot: number, bytes: Uint8Array): void;
  syncMetadata(slot: number): void;
  readPage(index: number): Uint8Array;
  writePage(index: number, bytes: Uint8Array): void;
  syncData(): void;
  truncateData(length: number): void;
}
/** Immutable committed pages and alternating durable roots. Does not own or
 * close the supplied device. read() returns ordered records as a JSON array. */
export class PagedCommitLog implements CommitLog {
  constructor(device: PageDevice, options?: PagedLogOptions);
  readonly supportsDurableFlush: boolean;
  read(): string;
  append(record: string): void;
  flush(): void;
}
