# Physical LSM composition

Objective: compose the selected immutable native doublets tables into a real
on-disk journal store with bounded write buffering, WAL replay, manifest
publication, recovery, compaction and retirement. This is required work under
R1/R5/R6/R7/R8/R9/R11/R12; the table primitive alone does not satisfy it.

- [x] Inventory existing atomic journal, replacement-log, maintenance and native
      ownership contracts; retain all existing implementations and defaults.
- [ ] Specify a versioned WAL/manifest protocol and independent finite oracle.
      Before production implementation, run missing-API controls in each
      language. Preserve source fixtures and actual failures.
- [ ] Implement matching portable Rust/C#/JS engines, with Rust controls running
      in actual WASM. WAL and table storage are independently selected and
      caller-owned. Require atomic complete-journal replacement for bounded WAL.
- [ ] Implement native exclusive directory ownership and synchronized table
      creation/retirement without SQLite. Never steal stale ownership or reuse
      a table name after an uncertain publication.
- [ ] Verify group CAS, absent/live-empty/tombstone generations, caller-owned
      values, conflict/no-op behavior, admission before I/O and finite limits.
- [ ] Verify every mutation failure before and after its effect, recovery of
      acknowledged groups, uncertain-outcome fencing, orphan handling, repeated
      compaction/checkpointing and following edits.
- [ ] Exercise actual process exits and shared physical files across native
      Rust/C#/Node/Bun/Deno. Compare immutable file hashes on read-only reopen.
- [ ] Demonstrate the existing cluster engine and named links API over this
      selected store. Document read/write amplification, buffering, replay,
      ownership, capacities and guarantees with runnable examples.
- [ ] Run complete local checks/coverage within the container budget; preserve
      complete coverage denominators and inspect fresh exact-head CI logs.
      Review and publish atomic useful steps only to the prepared PR branch.

Proposed publication order (to verify before implementation): an atomic
replacement WAL record contains the current manifest, nonreusable global
modification generation, covered generation, last reserved table identity and
bounded buffered updates.
Reserve a table identity durably before creating its physical files. Create
and sync the complete sealed table, then atomically replace the WAL checkpoint
to select it and discard the covered buffered updates. Only after that decision
may unreferenced old files be retired. Ordinary CAS groups append one WAL record
and flush before changing the visible buffer or returning success. Any uncertain
mutation fences the owner until actual reopening and discovery.

A table publication must include its file identity, native database identity,
digest and declared bounds. Recovery must validate the complete bounded WAL and
all selected tables before exposing any data. It must not substitute an old
table when the selected table is missing or damaged. A durable reservation makes
incomplete unselected files distinguishable without reusing their names.

The independent `experiments/issue-138/model-lsm-publication.py` oracle executes
twelve before/after reservation, creation, synchronization, publication and
retirement cuts. Each preserves the exact selected values, a positive-generation
tombstone, a live empty journal and following unique-name allocation. Two
additional controls reject a missing or incomplete selected table. This is a
finite decision-order model only; it does not substitute for production fault
tests, physical file tests or a complete WAL protocol.

Six additional model controls reject inconsistent covered/global generations,
unreserved or repeated IDs, and mutation after reserving the last u64 ID. The
last ID itself remains usable. These controls also precede production code.

The first implementation will have explicit finite total-key, encoded-byte,
buffer, run-count and replay bounds. Compaction must preflight the complete
replacement before publishing or deleting anything. Tombstones retain positive
nonreused generations through compaction. Complete maintenance scans either
return an owned snapshot or fail without partial success. Physical power-loss
certification is a separate unfinished criterion.


Implementation notes remain in `experiments/issue-138/lsm-protocol-design.md`;
`dev/log/issues/138/pulls/139/lsm-publication-model.txt` preserves the finite
oracle outcome. No production LSM code or verification is claimed by this plan.
A separate bounded V8 probe, `probe-json-key-ownership.mjs`, observes both a
JSON-decoded 64-character key and its UTF-8 copy as sequential strings with a
64 KiB payload. The suspected retained payload backing was not reproduced in
this V8 build; no production fix or universal runtime-memory guarantee follows.
Run it only with the heap/stack limits documented in the script.
