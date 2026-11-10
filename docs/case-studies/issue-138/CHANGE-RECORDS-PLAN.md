# Next acceptance batch: incremental logical revisions

R6/R11's complete serialization cost remains after shared state removes row-map
copies. The byte-delta decorator receives already serialized complete snapshots.
This batch selects logical changed-row encoding at the existing publish seam.
The issue's wider coverage, physical storage and distributed criteria stay open.

1. [x] Preserve shared-state publication separately; all nine exact-head workflows
   pass on `3960c25`, with timestamps in shared-state-ci.txt.
2. [x] Audit pinned RocksDB write-batch/WAL source and SQLite page-WAL docs.
   Reuse the existing checksum, append/flush and uncertain-prefix contracts.
3. [x] Before implementation, record a finite complete-serialization baseline;
   author independent valid/invalid wire fixtures and runnable failing replay
   tests in Rust/C# plus a physical-composition JS regression.
4. [x] Select Snapshot(default)/Changes independently of state/concurrency/history;
   implement canonical engine2 records and mixed recovery against exact bases.
5. [x] Preserve branch attribution after history eviction, standalone merge bases,
   input-order group outcomes and physical delta composition. Keep the default
   full-image path free of the incremental encoder's address-list allocation.
6. [x] Exercise both encodings/maps, all admission/isolation/validation choices,
   constraints, histories, allocator edges, names, pins, branches and reopening.
   Add 32 durable-image group schedules and 40 bounded actual process exits per
   native language. Retain finite child deadlines and guaranteed reaping.
7. [x] Match public WASM/JS and strict TS options; consume independent vectors
   through the actual binding. Add cross-language example flags and byte controls.
8. [x] Run complete local compatibility/coverage suites, all runtime conformance
   and persistent eight-configuration cross-language examples sequentially.
9. [x] Review all changes, commit, push only the prepared branch, update PR 139,
   and verify every fresh exact-head workflow after publication.

Baseline: 256 named rows, sixteen one-row commits encode 4,096 rows / 966,063
bytes. Flat and Shared with Changes encode sixteen rows / 4,246 bytes. Focused
native/C#/actual-WASM-boundary tests preserve pins and exact reopening. This
counts serialized bytes and rows, without timing or physical durability claims.
The public binding uses u32 addresses; its exhausted-allocator control correctly
rejects creation while permitting following edits to existing rows.

Logical redo does not implement a pager, doublewrite, an LSM, torn-page repair,
compaction, physical power-loss verification or complete-project coverage.
Continue the broader requirements after this batch; passing these checks alone
does not complete issue #138.

Complete local validation passes; changes.txt records exact official totals,
all runtime/examples checks and the remaining acceptance gaps.

All nine workflows pass on dd22de1; changes-ci.txt preserves SHA, timestamps and links.
