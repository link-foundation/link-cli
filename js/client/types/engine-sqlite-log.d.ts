import type { CommitLog } from './index.js';
export interface SqliteConnection {
  exec(sql: string): unknown;
  prepare(sql: string): {
    all(...parameters: unknown[]): unknown[];
    run(...parameters: unknown[]): unknown;
  };
  close(): void;
}
export interface SqliteLogOptions {
  /** SQLite busy timeout, integer milliseconds; default 0 (fail immediately). */
  busyTimeoutMs?: number;
  /** WAL auto-checkpoint page threshold; default 1000, zero selects manual. */
  autoCheckpointPages?: number;
}
export type SqliteCheckpointMode = 'passive' | 'full' | 'restart' | 'truncate';
export interface SqliteCheckpointResult { busy: number; logFrames: number; checkpointedFrames: number; }
/** Owns a dedicated opened synchronous SQLite >=3.51.3 file connection. */
export class SqliteCommitLog implements CommitLog {
  constructor(connection: SqliteConnection, options?: SqliteLogOptions);
  readonly supportsDurableFlush: true;
  read(): string;
  append(record: string): void;
  flush(): void;
  checkpoint(mode?: SqliteCheckpointMode): SqliteCheckpointResult;
  close(): void;
  [Symbol.dispose](): void;
}
