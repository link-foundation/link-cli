# Optional SQLite page-WAL journal

This batch addresses R1/R5/R6/R7/R9/R11's physical storage component gap at the
existing replaceable ordered-log seam. It does not claim to repair legacy mapped
Doublets files, implement every physical storage algorithm, or complete issue 138.
The published incremental-record batch remains separate; its fresh exact-head CI
must complete before changing dependency/build graphs for this component.

## Work checklist

1. [x] Study SQLite WAL, synchronous and checkpoint contracts and pinned pager
   source; inspect maintained Rust and ADO.NET components and runtime providers.
2. [x] Run finite actual-provider probes in Node, Bun, Deno and C#. Verify exact
   BLOB bytes, own pending reads, independent committed reads, shared SQL commit,
   reader pins, blocked/successful truncation, stale-tail detection and rollback.
   Preserve Deno TEXT/NUL failure separately. Bound C# provider retries explicitly.
3. [x] Author executable JS adapter contracts before implementation; preserve
   the missing-adapter failure as a feature baseline, not an existing-code bug.
4. [x] Confirm all nine workflows pass on incremental-record head dd22de1.
5. [x] Specify a common owned database/schema, sequence and UTF-8 BLOB protocol.
   Add independently authored fixtures and Rust/C# adapter contracts before code.
6. [x] Implement an optional native Rust component using bundled rusqlite,
   provider-neutral C# DbConnection and separately exported synchronous JS adapter.
   Bare stores and unselected journals add no SQLite provider/build dependency.
7. [x] Enforce WAL/FULL, supported runtime versions, schema/sequence validation,
   exact pending/committed views, finite contention handling and stale-owner fences.
   Preserve unknown append/commit outcomes; close rolls pending work back.
8. [x] Expose explicit bounded checkpoint maintenance, including busy/frame counts
   and pinned-reader controls. Do not add logical journal truncation/compaction.
9. [x] Verify constructor cleanup, options, setting drift, corrupt schema/UTF-8/
   sequences, SQL errors, before/after COMMIT faults, close retry and reopening.
10. [x] Exercise staged engine pins, group commits, logical/physical encodings and
    replication. Use bounded native/C# actual process exits before/after commit,
    bounded storage-full failures and copied-WAL frame corruption controls.
11. [x] Run actual JS/WASM bridges on Node/Bun/Deno and persistent Rust -> C# ->
    JS examples over the same SQLite database with exact following writes.
12. [x] Document costs, provider versions, ownership, sidecars, checkpointing,
    same-host filesystem/VFS assumptions and the remaining physical limits.
13. [x] Add integration execution to existing CI, then complete focused/full local
    suites, formatting/lints/types/coverage sequentially within the memory budget.
14. [ ] Review source and PR diff, refresh feedback/main, commit useful atomic
    steps, push only the prepared branch, update PR 139 and verify exact-head CI.

## Concrete design constraints

A dedicated connection belongs to the log. Lazy BEGIN IMMEDIATE holds write
ownership across appended records; Flush COMMITs their SQL transaction. One
engine still coordinates live sessions and immutable state. SQLite's writer lock
alone is not a lifetime engine fence: compare the expected log tail under that
write transaction, and poison stale owners instead of silently resetting their
baseline. Independent cached engines do not acquire distributed MVCC semantics.

Record sequences are contiguous positive signed i64 values, separate from engine
revisions. UTF-8 payloads use BLOB parameters to retain embedded NUL bytes across
providers; reject malformed UTF-8 and physical line breaks consistently. Database
identity/schema versions reject foreign stores. Validate sequence continuity at
open without materializing all payloads; read validates every record. Pending
records are visible to their owner and never to independent readers before Flush.

WAL+FULL must be read back, not merely requested. Require SQLite >= 3.51.3 to
include the WAL-reset race fix, accepting newer runtimes. Rust/JS busy_timeout=0
can fail immediately. Microsoft.Data.Sqlite retries independently; use a positive
finite CommandTimeout on each explicit command (one second by default), never
zero, which means unlimited retry. Preserve provider differences in the recipe.

An SQL exception after component I/O may follow a committed group. Fence all
operations until close/reopen; a best-effort rollback releases a still-active
write transaction without deleting committed records. Maintenance rejects pending
appends and returns actual busy/frame counters instead of retrying indefinitely.

SQLite supplies the selected pager/WAL/checkpoint algorithms and their documented
VFS assumptions. Real process exits and finite frame-damage tests are distinct
from physical power loss. This batch does not add doublewrite, LSM layout,
automatic checkpoint truncation, Dynamo/Accord, automatic cluster balancing or
whole-project 100% coverage. Those acceptance criteria stay open.
