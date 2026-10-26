import { CheckpointPagedCommitLog } from './engine-checkpoint-pages.js';
import type { PagedLogOptions } from './engine-paged-log.js';

/** Owns path, .bank0 and .bank1, with optional .mirror copies of both banks.
 * Acquires every writer fence before validation/repair. Close attempts all
 * devices in reverse order, root last, without flushing pending records.
 * Uncertain cleanup retains affected native writer fences. */
export class FileCheckpointPagedCommitLog extends CheckpointPagedCommitLog {
  constructor(path: string, options?: PagedLogOptions & { mirroredBanks?: boolean });
  readonly supportsDurableFlush: true;
  close(): void;
  [Symbol.dispose](): void;
}
