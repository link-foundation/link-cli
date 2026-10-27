import type { LinksClient } from './index.js';
import type { CausalBatch, CausalBatchEdit } from './engine-causal-batch.js';
import type { CausalValue } from './engine-causal-version.js';
import type { CausalConflictPolicy, CausalProjectedRow, CausalProjectionOptions } from './engine-causal-projection.js';

export interface CausalWorkspaceOptions extends CausalProjectionOptions {
  stateStructure?: 'flat' | 'shared' | 'sorted-runs';
  constraintValidation?: 'scan' | 'indexed';
  constraints?: readonly ('references' | 'unique-pairs' | 'unique-names')[];
  trace?: boolean;
}
export interface CausalPreparedBatch {
  token: string;
  generation: string;
  edits: CausalBatchEdit[];
}
export interface CausalWorkspaceBinding {
  CausalLinksWorkspace: new (versionsJson: string, first: string, last: string, policy: string, optionsJson: string) => {
    request(text: string): string;
    finish(): string;
    free(): void;
  };
}
/** Owned raw u64 links/names workspace over the actual Rust engine. Addresses
 * are canonical decimal strings. Callers reserve a fresh inclusive address
 * range and a fenced author lineage. Preparation freezes queued mutations;
 * commit publishes one local batch with an explicit original retry token.
 * client() exposes the ordinary BigInt links/query API until preparation. */
export declare class CausalSession {
  private constructor();
  static open(binding: CausalWorkspaceBinding, batch: Pick<CausalBatch, 'snapshot' | 'write'>,
    first: string, last: string, policy: CausalConflictPolicy, options?: CausalWorkspaceOptions): Promise<CausalSession>;
  /** Shares the workspace; preparation ends requests and close rolls back. */
  client(): LinksClient;
  createLink(source: string, target: string): Promise<string>;
  ensureCreated(id: string): Promise<string>;
  getLink(id: string): Promise<Omit<CausalProjectedRow, 'name'> | null>;
  updateLink(id: string, source: string, target: string): Promise<Omit<CausalProjectedRow, 'name'>>;
  deleteLink(id: string): Promise<Omit<CausalProjectedRow, 'name'>>;
  rows(): Promise<CausalProjectedRow[]>;
  getName(id: string): Promise<string | null>;
  getByName(name: string): Promise<string | null>;
  setName(id: string, name: string): Promise<null>;
  removeName(id: string): Promise<null>;
  readonly prepared: CausalPreparedBatch | null;
  prepare(token: string): Promise<CausalPreparedBatch>;
  commit(token: string): Promise<CausalValue[]>;
  rollback(): Promise<void>;
  close(): Promise<void>;
}
