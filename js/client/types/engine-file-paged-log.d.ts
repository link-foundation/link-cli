import { PagedCommitLog, type PageDevice, type PagedLogOptions } from './engine-paged-log.js';
/** Native Node/Bun/Deno file device. Parent must exist. Atomic exclusive
 * creation of path + '.writer' excludes all cooperating language providers.
 * Crash recovery requires fencing the old owner before explicit lock removal. */
export class FilePageDevice implements PageDevice {
  constructor(path: string);
  readonly supportsDurableFlush: true;
  readonly dataLength: number;
  newDatabaseId(): Uint8Array;
  readMetadata(slot: number): Uint8Array | undefined;
  writeMetadata(slot: number, bytes: Uint8Array): void;
  syncMetadata(slot: number): void;
  readPage(index: number): Uint8Array;
  writePage(index: number, bytes: Uint8Array): void;
  syncData(): void;
  truncateData(length: number): void;
  close(): void;
  [Symbol.dispose](): void;
}
/** Owns and closes its FilePageDevice. See the common native page protocol. */
export class FilePagedCommitLog extends PagedCommitLog {
  constructor(path: string, options?: PagedLogOptions);
  readonly supportsDurableFlush: true;
  close(): void;
  [Symbol.dispose](): void;
}
