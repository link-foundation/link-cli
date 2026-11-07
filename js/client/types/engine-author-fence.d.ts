import type { AtomicJournalStore, AtomicJournalStoreProvider } from './index.js';
export interface AuthorFenceOptions { maxEntries?: number; maxBytes?: number; requireDurableFlush?: boolean; }
/** Epochs are canonical unsigned 64-bit decimal strings. */
export interface AuthorClaim { author: string; token: string; epoch: string; }
export interface AuthorFenceSnapshot { generation: string; epoch: string; claims: AuthorClaim[]; }
export interface GuardedJournalStore extends AtomicJournalStore { intoStore(): AtomicJournalStoreProvider; }
/** Explicit takeover; no TTL/election/permissions. One token per allocating
 * process lineage. Every writer must use the guard; bypasses are not fenced.
 * O(retained claims), permanent bounded receipts, 1..31 guarded data keys. */
export declare class AuthorFence {
  constructor(store: AtomicJournalStoreProvider, key: string, author: string, options?: AuthorFenceOptions);
  snapshot(): Promise<AuthorFenceSnapshot>;
  claim(expectedEpoch: string, token: string): Promise<AuthorClaim>;
  bind(claim: AuthorClaim): GuardedJournalStore;
}
