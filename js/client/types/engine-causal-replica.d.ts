import type { AtomicJournalStoreProvider } from './index.js';
import type { CausalValue, CausalPayload } from './engine-causal-version.js';
export interface CausalReplicaOptions {
  maxEntries?: number;
  maxBytes?: number;
  requireDurableFlush?: boolean;
}
export interface CausalSnapshot { author: string; lastCounter: string; versions: CausalValue[] }
/** Selected local ownership and retry receipts. The caller owns and closes
 * the coherent atomic store. Reserve one key for one unique author lineage. */
export declare class CausalReplica {
  constructor(store: AtomicJournalStoreProvider, key: string, author: string, options?: CausalReplicaOptions);
  snapshot(): Promise<CausalSnapshot>;
  read(address: string): Promise<CausalValue[]>;
  write(token: string, address: string, observed: readonly CausalValue[], payload: CausalPayload | null): Promise<CausalValue>;
  receive(incoming: readonly CausalValue[]): Promise<boolean>;
}
