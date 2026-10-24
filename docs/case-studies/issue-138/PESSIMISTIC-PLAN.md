# Optional pessimistic point-write strategy

Requirement mapping: R2, R5, R6, R7, R9, R11. Preserve all existing APIs/options.

Primary source inspected: facebook/rocksdb ea313605367281013c2c81158069e88411516357,
utilities/transactions/pessimistic_transaction.cc lines 1039-1255 and 60-90.
TryLock acquires a key, validates a configured snapshot after acquisition, and
releases a newly acquired lock on failed validation. Savepoint/rollback release
tracked locks; configurable wait/deadlock machinery has its own costs. Reuse the
strategy/design seam, never copy that implementation or claim RocksDB parity.

- [x] Read complete engine write/commit/branch and options/protocol paths in Rust,
   C# and WASM. Inspect source with specific pinned links before citing it.
- [x] Add initially failing native/public tests for a Pessimistic strategy: multiple
   sessions edit distinct addresses; competing writes fail before staging;
   missing/explicit addresses and names share a lock; repeated own writes work.
- [x] Use immediate busy errors (no waits, no deadlock detector). A session retains
   successful earlier writes after a rejected operation. Commit attempts,
   rollback/dispose/drop release all its locks, including constraint/log failures.
- [x] Validate snapshot staleness/ABA after acquiring or while checking the lock;
   preserve existing read committed/snapshot/serializable visibility. Pessimistic
   write locking alone is not serializability. Keep existing point-read/revision
   validation choices and conservative scan semantics.
- [x] Allocate ownership/key-lock metadata only for selected Pessimistic sessions;
   preserve lazy conflict-generation reclamation. Reject reuse of any existing
   locked key by an independently merging branch; disjoint merges may proceed.
- [x] Match native C# and Rust-backed JS/WASM options, operations and errors. Engine-
   local locks are not distributed locks: reject that strategy in ClusterEngine
   unless coordination supplies a real shared lock manager. Test that explicitly.
- [x] Cover fresh sessions after commit/conflict/fault, reverse key orders, dropped
   sessions, independent branches, allocator reservations, explicit addresses,
   retained snapshots, failed constraints and unknown journal outcomes.
- [x] Extend finite serial-history oracles/conformance and actual WASM shared tests.
   Collect coverage; all native suites, lint/format/typechecks and examples pass.
- [x] Add configuration/cost/retry docs and comparison with pinned RocksDB source,
   runnable cross-language examples and minor release fragments. Review complete
   PR diff/feedback/main ancestry.
- [x] Commit/push and verify fresh exact-SHA CI.

Commit 6087cfc3e01f8b8d953ce46b6c3184ae0d2859c1 passes all nine workflows
created 2026-10-24 21:24:47 UTC: 37 successful checks, ten skipped and one neutral.
Linux, Windows, macOS, actual WASM and shared etcd conformance pass.

Avoid a global mutex held for a session's lifetime or silently serializing
unrelated point writes. Keep selected strategy costs explicit. Do not add waiting
or distributed-lock claims without a separately verified protocol.
