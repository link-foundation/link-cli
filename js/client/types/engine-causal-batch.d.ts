import type { AtomicJournalStoreProvider } from './index.js';
import type { CausalPayload, CausalValue } from './engine-causal-version.js';
import type { CausalReplicaOptions, CausalSnapshot } from './engine-causal-replica.js';
export interface CausalBatchEdit {
  address: string;
  observed: readonly CausalValue[];
  payload: CausalPayload | null;
}
/** Generation is the underlying owner key's opaque guard. Receipt-only empty
 * batches may leave it unchanged; other keys may make the next value jump. */
export interface CausalBatchSnapshot extends CausalSnapshot { generation: string }
/** Selected bounded complete-journal composition. The caller owns the store
 * and reserves two distinct keys for one author. Edits are strictly ordered by
 * numeric address. Retry the original token/base/request after uncertain I/O.
 * This provides one local atomic publication, not remote batch atomicity. */
export declare class CausalBatch {
  constructor(store: AtomicJournalStoreProvider, ownerKey: string, receiptKey: string, author: string, options?: CausalReplicaOptions);
  snapshot(): Promise<CausalBatchSnapshot>;
  write(token: string, baseGeneration: string, edits: readonly CausalBatchEdit[]): Promise<CausalValue[]>;
}
