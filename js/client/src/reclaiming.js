// Selection alone loads the collector/codecs. Ordinary client imports allocate none.
export class ReclaimingPartitionedJournalStore {
  #inner;
  constructor(token, inner) {
    if (token !== ReclaimingPartitionedJournalStore) throw new TypeError('use ReclaimingPartitionedJournalStore.open');
    this.#inner = inner;
  }
  static async open(catalog, partitions, route, options) {
    const { openStore } = await import('./reclaiming-store.js');
    return new ReclaimingPartitionedJournalStore(ReclaimingPartitionedJournalStore, await openStore(catalog, partitions, route, options));
  }
  get supportsDurableFlush() { return this.#inner.supportsDurableFlush; }
  read(keys) { return this.#inner.read(keys); }
  compareExchange(keys, expected, updates) { return this.#inner.compareExchange(keys, expected, updates); }
  collectGarbage() { return this.#inner.collectGarbage(); }
}
