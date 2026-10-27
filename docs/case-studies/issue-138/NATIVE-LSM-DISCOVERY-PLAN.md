# Native LSM discovery and ownership

Objective: open a physical file namespace without repairing, truncating,
initializing or retiring files before the complete selected LSM decision has
been validated. This continues the unfinished native portion of
[the physical composition plan](PHYSICAL-LSM-PLAN.md).

- [x] Reproduce the missing read-only checkpoint inspection API in Rust, C#
      and JavaScript before implementing it. Keep the finite independent
      inspection experiment as an additional oracle.
- [x] Add matching bounded inspection APIs: Rust
      `CheckpointPagedLog::inspect(&mut root, &mut banks, max_pages)`, C#
      `CheckpointPagedCommitLog.Inspect(root, first, second, maxPages)` and
      JavaScript `CheckpointPagedCommitLog.inspect(root, banks, options)`.
      Return owned records without allocating a database identity or completing
      any repair. Do not change the existing opening constructors.
- [x] Verify empty/partially initialized devices, selected bank identity,
      conflicting metadata, complete page chains, ignored uncommitted suffixes,
      finite limits and staged mirror repairs. Run shared Rust controls in
      actual WASM, and preserve complete coverage denominators.
- [ ] Implement exclusive outer directory ownership before acquiring any WAL
      or table handle. Reject stale fences and foreign, malformed or symbolic
      names; never steal another owner's fence.
- [ ] Bound directory enumeration before collecting it. Validate every
      selected logical image from existing regular files before calling a
      lower-level constructor that can create missing files. A valid surviving
      metadata slot or mirrored copy may suffice; do not require both copies.
- [ ] Validate the inspected semantic WAL and all selected tables through a
      read-only WAL and namespace view. Suppress repair completion and orphan
      deletion in this validation pass. Close its temporary table leases before
      opening the ordinary checkpoint WAL and actual physical store.
- [ ] Handle a completely uninitialized WAL only with an empty namespace.
      Initialize after discovery succeeds; a nonempty unrooted bank is corrupt.
- [ ] Synchronize successful native file creation and retirement according to
      each platform's supported contract. State the exact guarantee separately
      from file-content flushing and from physical power-loss certification.
- [ ] Close all child handles before releasing the outer fence. Retain the
      fence on uncertain cleanup; do not retry an unsafe raw descriptor close.
- [ ] Verify finite before/after mutation cuts, actual process exits, missing
      selected files and preservation of corrupt evidence before failure.
- [ ] Run complete native Rust/C#/Node/Bun/Deno whole-store file interchange,
      verify read-only file hashes and demonstrate real cluster consumers.
- [ ] Run local gates, review the complete diff, publish an atomic verified
      step and inspect the resulting exact-head CI before updating evidence.

The extra validation pass performs bounded reads twice under the same exclusive
outer owner. It prevents lower-level WAL recovery effects from preceding LSM
semantic validation. It does not promise protection against an unrelated actor
modifying files outside the ownership contract. The existing generic physical
store and lower-level file providers alone do not implement this facade.

The inspection prerequisite passes complete local validation on 2026-10-27 at
02:37:37Z. checkpoint-inspect{,-before}.txt preserve the actual before failures,
full native/C#/actual-WASM/runtime results and every coverage denominator. The
remaining native facade work is active; no native production API follows from
this prerequisite alone.

## JavaScript native facade checkpoint

The seven directory/validation/ownership/mutation steps above now pass for the
JavaScript native facade. Their unchecked state continues to track the required
Rust/C# ports. The 41 selected controls pass on Node, Bun and Deno, with every
production JavaScript module at enforced 100% coverage in the 617-test full suite.
A fresh actual-WASM engine verifies direct, partitioned and reclaiming physical
consumers in all three runtimes. Reproductions and exact scope are preserved in
native-lsm-js{,-before}.txt. The inspection prerequisite has all nine green
exact-head workflows on 7baafed, recorded in checkpoint-inspect-ci.txt.

- [x] Implement and verify the JavaScript native facade and actual consumers.
- [x] Publish the verified JavaScript step and inspect fresh exact-head CI.
- [ ] Port native discovery, ownership, cleanup and process controls to Rust/C#.
- [ ] Verify complete whole-store interchange across all five native runtimes.

Marker-removal uncertainty is separate from a possibly live handle: removal is
attempted only after all known handles close. An error after removal can leave
the marker absent while the old facade is closed. Live-handle uncertainty keeps
the fence and never retries an unsafe raw descriptor close.

All nine workflows pass on native JavaScript publication commit 39a551d;
native-lsm-js-ci.txt records the exact SHA and workflow URLs. The C# native
facade now passes 34 native controls and both complete 2,409-test configurations.
They include 124 finite cuts at 62 compaction operation boundaries, ten reaped
abrupt-exit children and surviving-copy preservation before later validation.
Three real C# consumers and all 32 complete-store handoffs among C#/Node/Bun/Deno
pass. Full native C# coverage remains below 100%; native-lsm-csharp{,-before}.txt
retain exact scope and denominators. Publish after final gates and inspect fresh
exact-head CI. The Rust port and complete five-runtime matrix remain active.
