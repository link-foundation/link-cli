import type { PageDevice, RepairPageDevice } from './engine-paged-log.js';
/** Caller owns two distinct exclusive devices. Writes and syncs both copies;
 * repairs one damaged copy only after complete committed-prefix validation. */
export class MirroredPageDevice implements PageDevice {
  constructor(primary: RepairPageDevice, secondary: RepairPageDevice);
  readonly metadataFlags: 1;
  readonly supportsDurableFlush: boolean;
  readonly dataLength: number;
  newDatabaseId(): Uint8Array;
  readMetadata(slot: number): Uint8Array | undefined;
  writeMetadata(slot: number, bytes: Uint8Array): void;
  syncMetadata(slot: number): void;
  /** Direct reads require the expected checksum; this method throws. */
  readPage(index: number): Uint8Array;
  readCheckedPage(index: number, expected: bigint): Uint8Array;
  completeRecovery(): void;
  writePage(index: number, bytes: Uint8Array): void;
  syncData(): void;
  truncateData(length: number): void;
}
