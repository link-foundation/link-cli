import type { AtomicJournalStoreProvider } from './index.js';
export interface AddressAllocatorOptions {
  maxEntries?: number;
  maxBytes?: number;
  requireDurableFlush?: boolean;
}
/** Numeric fields are canonical unsigned 64-bit decimal strings. */
export interface AddressReservation {
  author: string;
  token: string;
  count: string;
  first: string;
  last: string;
}
export interface AddressAllocationSnapshot {
  generation: string;
  next: string | null;
  reservations: AddressReservation[];
}
/** Selected permanent ranges over one immutable domain and linearizable key.
 * All fresh writers must share that key; initialize above all issued identities.
 * After uncertain I/O retry the exact author/token/count. O(retained journal)
 * per grant, finite admission, no recycling or author lineage fencing. */
export declare class AddressAllocator {
  constructor(store: AtomicJournalStoreProvider, key: string, first: string, last: string, options?: AddressAllocatorOptions);
  snapshot(): Promise<AddressAllocationSnapshot>;
  reserve(author: string, token: string, count: string): Promise<AddressReservation>;
}
