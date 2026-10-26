# Consistent namespace maintenance capability

Objective: prepare safe selected immutable-object reclamation without changing
AtomicJournalStore implementations or existing partitioned-store behavior.

- [x] Reread issue/review inventory; preserve exact-head mirrored-root evidence.
- [x] Research etcd historical ranges, pagination and exact revision deletes.
- [x] Add minimal real-target tests and record missing API failures in Rust/C#/JS.
- [x] Add independent optional maintenance contract with explicit scan limits.
- [x] Implement revision-pinned complete etcd scans; restart entire read on failover.
- [x] Implement single-send compare-delete with strict acknowledged outcome parsing.
- [x] Test malformed/incomplete pages, ordering, limits, compaction/failover,
      invalid keys/revisions, canonical encodings, concurrent change and ABA.
- [x] Exercise actual etcd and native/Node/Bun/Deno capability parity.
- [x] Preserve full coverage denominators; run appropriate complete local checks.
- [x] Update docs/releases and review complete local/batch changes.
- [x] Commit, push only prepared branch and inspect fresh exact-head CI.
- [ ] Continue actual epoch-fenced reclamation afterward: this capability alone
      does not satisfy object GC or the wider unfinished issue criteria.

The pre-implementation baseline is df6e01c. The three actual targets fail with
missing public maintenance types/methods; the saved clean-baseline reproducer
preserves minimal fixtures. Ninety-four independent wire vectors are consumed
by every implementation. Real three-member etcd probes pass five language/runtime
examples, concurrent mutation between pinned pages, compaction with complete
restart, lost deletion response, leader loss and majority loss. The harness waits
for an elected live leader and a successful linearizable read before the leader
loss assertion; it preserves the original finite deadlines. This corrects a
premature harness precondition rather than extending production retries.

The sequential complete validation passes from 20:42:50Z through 21:13:57Z
on 2026-10-26: 1,032 native Rust plus six doctests, 2,268 C# tests in each
configuration, 631 actual-WASM tests, 359 JS tests and 221 repository tests.
Bun and Deno each pass all 98 maintenance cases; all production JS coverage
thresholds remain 100%. Full denominators and selected-module coverage are
recorded in maintenance.txt. No OOM event occurs. Fresh native example builds
and the final real-etcd run include the lazy-codec changes. The root-import
loader guard proves ordinary read/CAS avoids the selected JS helpers. The provider capability cannot establish object liveness:
epoch-fenced publication, complete root discovery and coherent reader refresh
remain required before immutable-object collection is implemented.

All nine workflows pass on 44e0fa8, including both native platform matrices.
maintenance-ci.txt preserves exact heads, timestamps and permanent run URLs.
Actual collection is the next implementation batch and remains unfinished.
