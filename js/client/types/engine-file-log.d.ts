import type { CommitLog } from './index.js';
/** Exclusive writer. Crash recovery requires fencing the old owner before
 * explicitly removing path + '.writer'. Never steal a live writer's lock. */
export class FileCommitLog implements CommitLog {
  constructor(path: string);
  readonly supportsDurableFlush: true;
  read(): string;
  append(record: string): void;
  flush(): void;
  close(): void;
  [Symbol.dispose](): void;
}
