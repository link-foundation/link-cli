# Next bounded strategy: serializable read-set validation

Whole-revision validation rejects disjoint point transactions. Study FoundationDB
read/write conflict ranges, then offer an optional local read-set validator with
conservative scan invalidation. Preserve the revision validator as the default.
This is independent of transport/physical log/history choices and is not
FoundationDB's distributed resolver, network service or range-index implementation.

- [x] Pin FoundationDB primary docs and source: NativeAPI.cpp get/addReadConflictRange,
   ConflictSet.cpp read snapshot + read/write ranges. Record costs and differences.
- [x] Write failing Rust/C#/actual-WASM tests for the missing configuration and disjoint
   point transactions. Keep API validation strict; define interaction with
   nonserializable isolation and exclusive admission.
- [x] Record point addresses, including missing rows, in selected sessions only.
   Mark all scans and name searches as predicates that invalidate on any revision.
- [x] Under the commit mutex, validate point read and write generations against the
   session's base before publication. Preserve ABA/tombstones, branch publications,
   last-session cleanup and unknown-outcome fencing. No transaction predates open.
- [x] Verify write skew/read-write cycles, missing-row insertion, read-only sessions,
   own writes, disjoint updates, phantoms/name searches and retries after conflict.
- [x] Expose equivalent Rust/C#/JS options and TS typing through the actual WASM core.
- [x] Add finite operation schedules and an independent finite serializability oracle. Run full
   suites, 100% JS checks, actual WASM cross-runtime and coverage, native coverage,
   format/Clippy, update comparison docs/examples and publish one atomic commit.

## Evidence before publication

On commit `6d240a9`, actual WASM rejects the missing option as an unknown JSON
field; native tests reject the missing enum/option at compile time. The selected
implementation passes 1,296 independently checked histories per native language.
A bounded actual-WASM acceptance comparison opens 32 concurrent sessions, each
editing its own profile. Revision validation accepts one and rejects 31; read-set
validation accepts all 32. This measures accepted independent work, not latency
or throughput. Both validators retain their documented scan conflict boundary.

Sources, costs and runnable examples are linked in
[ENGINE-VALIDATION.md](../../ENGINE-VALIDATION.md). Actual WASM tests also cover
invalid options, disjoint points, a read/write cycle, scans and missing rows;
Node/Bun/Deno schedules add ABA, branch and read-only cases. Selected bookkeeping
is verified through private native tests without expanding the public API.
