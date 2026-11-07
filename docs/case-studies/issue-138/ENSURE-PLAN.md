# Explicit-address transaction audit

Coverage identifies a public `ensure_created` path that bypasses journal
recording. Recovery already calls the inner store directly; query callers also
use this public path. Establish its behavior before changing it.

- [x] Reproduce rollback and auto-commit recovery for an explicit small address.
- [x] Check both map-backed and mapped stores, including intermediate placeholders.
- [x] Reproduce version checkout and branch attribution for explicit addresses.
- [x] Verify zero/existing addresses remain no-ops and later writes can commit.
- [x] Keep fallible named-query bridges from panicking on transaction I/O errors.
- [x] Verify error cleanup with bounded injected storage failures.
- [x] Run focused/full suites, coverage, Clippy and formatting.
- [x] Publish and inspect exact-SHA CI.

All probes use at most eight requested addresses. Existing allocator behavior
differs between backends: the map removes intermediate allocations, while the
mapped adapter retains placeholders. Transaction logging must preserve each
backend's behavior and undo every row that the operation actually creates.

Four initial tests fail on `e652c2c`: a map allocation survives rollback,
eight mapped placeholders survive rollback, auto allocation records no journal
entry, and a feature branch has no sequence for its allocation. Four independent
bridge cases additionally reproduce panics instead of returned I/O errors.
Three injected allocation/composite failures reproduce stuck automatic
transactions or a partially created row. The fixes preserve caller-owned
transactions for explicit rollback while releasing failed automatic operations.

Fresh exact-SHA checks for `b16332647ddacad22a999c741941a29a204b62f1`
finish with 37 successes, ten skipped and one neutral. Recent runs are created
at 2026-10-24T19:17:39Z and match that SHA.
