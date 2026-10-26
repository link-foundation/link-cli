# Optional logical journal checkpoints

Requirement mapping: R1/R3/R5/R6/R7/R8/R9/R10/R11/R12. Every wider criterion
remains active; this plan does not replace the complete acceptance inventory.

- [x] Inspect journal replay, retained snapshots, allocator publication, network
      CAS, partitioned immutable objects and all three language surfaces.
- [x] Create independent engine3 vectors and native/C# replay tests before
      implementation; preserve the missing-feature failure separately from bugs.
- [x] Export retained committed snapshots without changing the live engine/log;
      accept a checkpoint only as the first journal record, then require normal
      contiguous revisions, nondecreasing allocators and selected constraints.
- [x] Preserve None/Last/All history, Unicode names, holes and committed address
      reservations across all state representations and both revision encodings.
- [x] Add matching Rust/C#/WASM/JS APIs and a separately selected positive network
      interval. Defaults keep existing append behavior and avoid checkpoint work.
- [x] Publish checkpoints through the existing atomic multi-shard generation CAS.
      Detect changed work before compaction; never publish a read-only checkpoint.
- [x] Test shrinking/equal-length journals, stale writers/read-only dependencies,
      before/after-effect errors, closed groups and following fresh writes.
- [x] Test partitioned stores, real etcd and native/actual-WASM interchange;
      measure bounded record/UTF-8 byte counts against append-only controls.
- [x] Document protocol, upgrade requirements, costs, examples and release inputs.
- [ ] Run all local suites/coverage, review the diff, publish only the prepared
      branch and inspect every fresh exact-head workflow and native matrix.
- [ ] Extend the independently selected physical delta decorator to accept an
      initial checkpoint in all languages, with reproducing tests first.

The engine3 first record is an independent logical snapshot. Following records
use engine1/engine2 and retain their original revision numbers. Old readers reject
engine3. Export retains each revision's exact committed allocator metadata, so saved
branch descriptors remain usable while their base is retained. Pending
reservations remain private to the original live owner. History All intentionally remains unbounded.

Draft integration regressions first reproduce separate branch replay rejecting
engine3, then exact saved bases rejecting after their historical allocator was
replaced with the latest value. Both native implementations now retain each
selected historical allocator and accept an initial checkpoint during branch
replay. Positive All/Last controls and an evicted-base negative control pass;
independent malformed vectors and selected scan/indexed constraints also pass.
These are draft-feature corrections, distinct from the missing-feature baseline.

Network checkpoint selection replaces only the newly published journal value;
generation CAS still checks every selected shard. Partitioned immutable objects
remain immutable and old objects are not reclaimed. Local append-only files and
pages are not rewritten. Export alone adds no durability or publication guarantee.

Complete local validation passes 975 native tests plus six doctests, 2,180 C#
tests per configuration, 580 actual-WASM tests, 211 JS tests and 221 repository
checks. All 48 physical/policy combinations pass 288 shared-file writers; these
existing page/delta fixtures start with ordinary revision-one journals. Real
etcd passes default, checkpoint and checkpoint-plus-cache runs, including native
handoffs, unknown responses, data/leader/majority loss. Node/Bun/Deno produce
identical 36-case finite payload reports. journal-checkpoint.txt records complete
coverage and payload counts; complete-project 100% remains unmet.

Review found that the older physical delta codec rejects initial engine3 records.
That composition remains an immediate follow-up with tests before its correction;
the current logical export and coordinated-publication tests do not verify it.

Research: Redis describes multipart AOF base/increment files and atomic manifest
replacement ([persistence](https://redis.io/docs/latest/operate/oss_and_stack/management/persistence/)).
This implementation instead computes logical snapshots synchronously and relies
on the already selected atomic store to publish them. etcd documents atomic
multi-key comparison/update and modification revisions
([API](https://etcd.io/docs/v3.6/learning/api/)); journal length must never substitute
for that conflict token. These sources motivate the design, not compatibility
claims with either system's physical persistence format.
