import { PagedCommitLog, type PagedLogOptions } from './engine-paged-log.js';
/** Owns native devices at path and path + '.mirror', including both exclusive
 * '.writer' files. Failed cleanup retains uncertain ownership fences. */
export class FileMirroredPagedCommitLog extends PagedCommitLog {
  constructor(path: string, options?: PagedLogOptions);
  readonly supportsDurableFlush: true;
  close(): void;
  [Symbol.dispose](): void;
}
