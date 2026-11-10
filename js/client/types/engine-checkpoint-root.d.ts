import type { PageDevice } from './engine-paged-log.js';
/** Caller owns two distinct raw roots. Selects checkpoint decisions, never data
 * pages. Repairs only after the checkpoint log validates its committed bank. */
export class MirroredCheckpointRoot implements PageDevice {
  constructor(primary: PageDevice, secondary: PageDevice, bankFlags: 0 | 1);
  readonly metadataFlags: 2 | 3;
  readonly supportsDurableFlush: boolean;
  readonly dataLength: number;
  newDatabaseId(): Uint8Array;
  readMetadata(slot: number): Uint8Array | undefined;
  writeMetadata(slot: number, bytes: Uint8Array): void;
  syncMetadata(slot: number): void;
  syncData(): void;
  completeRecovery(): void;
  /** Root data must remain empty; accepts only zero. */
  truncateData(length: number): void;
  /** Root devices have no pages; throws. */
  readPage(index: number): Uint8Array;
  /** Root devices have no pages; throws. */
  writePage(index: number, bytes: Uint8Array): void;
}
