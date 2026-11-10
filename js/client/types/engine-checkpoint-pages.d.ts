import type { CommitLog } from './index.js';
import type { PageDevice, PagedLogOptions } from './engine-paged-log.js';

/** Optional atomic replacement. Unknown outcomes require reopening the owner.
 * False means the ordered physical records were already identical. */
export interface ReplaceableCommitLog extends CommitLog {
  readonly supportsReplacement: boolean;
  replace(records: readonly string[]): boolean;
}
/** Two fixed banks and an alternating epoch-bound root. The three devices must
 * remain distinct, exclusive and caller-owned; this log never closes them.
 * The root uses raw metadata slots and an empty data file. Bank modes must match.
 * Replacement rejects pending physical records and prepares the inactive bank
 * completely before publishing the root. Failed I/O fences the owner. */
export class CheckpointPagedCommitLog implements ReplaceableCommitLog {
  /** Inspect owned committed records without initialization, truncation or repair completion.
   * Discard staged mirror wrappers if higher-level semantic validation fails. */
  static inspect(root: PageDevice, banks: readonly [PageDevice, PageDevice], options?: PagedLogOptions): string[];
  constructor(root: PageDevice, banks: readonly [PageDevice, PageDevice], options?: PagedLogOptions);
  readonly epoch: bigint;
  readonly supportsReplacement: true;
  readonly supportsDurableFlush: boolean;
  read(): string;
  append(record: string): void;
  flush(): void;
  replace(records: readonly string[]): boolean;
}
