import type { CommitLog, JournalRecovery } from './index.js';
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
/** Complete lines of a file journal; invalid UTF-8 becomes U+FFFD so the
 * checksum flags that record, and a missing file is empty. */
export function readJournalFileLossy(path: string): string[];
/** Rebuild `path` from the longest valid prefix among it and `copies`, after
 * keeping the original bytes in `<path>.damaged-<n>`. Fails while a writer owns
 * the journal. */
export function repairJournalFile(
  path: string,
  recover: (journals: string[][]) => JournalRecovery,
  options?: { copies?: string[] },
): { recovery: JournalRecovery; changed: boolean; preserved: string | null };
