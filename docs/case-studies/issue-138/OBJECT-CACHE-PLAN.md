# Optional immutable journal object cache

Requirement mapping: R6/R7/R8/R9/R10/R11/R12. The partitioned decorator reads
fresh catalog pointers and then downloads complete immutable journal objects on
every begin, even when an object's address and generation are unchanged.

- [x] Preserve pre-feature contracts that demonstrate the absent opt-in API.
- [x] Add an optional bounded LRU cache in Rust, C# and JS/TS. Default zero keeps
      no cache. Key by partition, object address and exact object generation.
- [x] Keep every catalog read fresh. Cache only fully checked immutable objects;
      return independent arrays and preserve all selected-generation CAS checks.
      Never cache a failed lookup or bypass staging equality/collision checks.
- [x] Test default/zero/oversize budgets, exact UTF-8 accounting, empty objects,
      LRU promotion/eviction, partition/generation separation, modified returned
      arrays, changing/failed catalogs, and rejected/following publications.
- [x] Run portable native contracts in actual WASM, JS 100% thresholds,
      TypeScript, full native/C# checks and persistent cluster interoperability.
- [x] Document the immutable-namespace requirement, payload-budget accounting,
      selected costs and remaining full-journal write/replay/retention limits.
- [ ] Commit and push an atomic step, update PR 139 and verify fresh exact-head
      CI. Preserve every wider unfinished criterion.

This is a local cache of immutable data, not a catalog cache or distributed lock.
The bounded payload accounting is not a process-heap measurement. Content keys
and generation do not authenticate hostile data stores. Cache hits retain the
previously verified content under the existing no-rewrite/no-delete contract.
