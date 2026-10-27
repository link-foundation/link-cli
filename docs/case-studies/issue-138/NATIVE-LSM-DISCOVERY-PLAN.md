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
- [x] Implement exclusive outer directory ownership before acquiring any WAL
      or table handle. Reject stale fences and foreign, malformed or symbolic
      names; never steal another owner's fence.
- [x] Bound directory enumeration before collecting it. Validate every
      selected logical image from existing regular files before calling a
      lower-level constructor that can create missing files. A valid surviving
      metadata slot or mirrored copy may suffice; do not require both copies.
- [x] Validate the inspected semantic WAL and all selected tables through a
      read-only WAL and namespace view. Suppress repair completion and orphan
      deletion in this validation pass. Close its temporary table leases before
      opening the ordinary checkpoint WAL and actual physical store.
- [x] Handle a completely uninitialized WAL only with an empty namespace.
      Initialize after discovery succeeds; a nonempty unrooted bank is corrupt.
- [x] Synchronize successful native file creation and retirement according to
      each platform's supported contract. State the exact guarantee separately
      from file-content flushing and from physical power-loss certification.
- [x] Close all child handles before releasing the outer fence. Retain the
      fence on uncertain cleanup; do not retry an unsafe raw descriptor close.
- [x] Verify finite before/after mutation cuts, actual process exits, missing
      selected files and preservation of corrupt evidence before failure.
- [x] Run complete native Rust/C#/Node/Bun/Deno whole-store file interchange,
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
- [x] Port native discovery, ownership, cleanup and process controls to Rust/C#.
- [x] Verify complete whole-store interchange across all five native runtimes.

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

## Native Rust port and C# CI investigation

- [x] Preserve the missing native Rust API reproduction, then port bounded
      directory ownership and read-only validation using the existing layers.
- [x] Pass 408 finite compaction operation cuts and 730 finite opening cuts,
      with explicit externally authorized cleanup of retained uncertain handles.
- [x] Finish all actual Rust process-exit cases, complete native integration
      controls, native/WASM full suites and coverage, and final consumer matrix.
- [x] List fresh runs with timestamps and SHA after publishing C# commit 6d134e8.
      Eight workflows pass; C# has formatter and Windows test failures.
- [x] Download C# run 40447379413 and inspect actual error excerpts from its
      30,873-line log rather than treating the successful Linux job as proof.
- [x] Identify formatting mismatch: local SDK 10.0.112 versus CI SDK 10.0.403.
      Apply exact-SDK formatting and enforce its selection in local validation.
- [x] Identify lost timeout diagnostics and directory-cleanup masking in the
      Windows process test harness. Add finite controls before implementing
      bounded capture, child reaping and primary-plus-cleanup error retention.
- [x] Run the C# controls and full local checks, publish the atomic diagnostic
      correction, then inspect fresh Windows phase evidence on the exact head.
      The underlying Windows timeout cause remains unestablished; deadlines and
      persistence guarantees remain unchanged during this investigation.
- [ ] Publish the Rust step with receipts, review its diff and inspect fresh
      exact-head CI before changing completion claims for the broader issue.

C# diagnostic head 78e2c2a passes every C# job, including all 2,412 tests on
Windows, macOS, Ubuntu and the fresh merge. Eight workflows pass; Rust Windows
instead exposes an aggregate ten-second deadline in version-process parents.
Actual child phases approach that deadline while continuing to progress through
separate durable operations. Reuse the existing finite movement supervisor,
preserving its ten-second idle bound and strict phase/cleanup controls. Exact
errors, timings and successful C# jobs are preserved in native-lsm-csharp-ci-after.txt
and version-progress-ci-before.txt. No underlying Windows storage-latency cause
or whole-issue completion follows from this narrower harness diagnosis.


## Complete native Rust local validation, 2026-10-27

The Rust facade now passes 21 native unit controls, ten integration controls,
408 finite compaction cuts, 730 finite opening cuts and all 408 actual abrupt
before/after child exits. Each child is killed/reaped on failure; external test
cleanup alone releases retained uncertain handles/fences. Three real cluster
consumers and all 50 complete-store handoffs among Rust/C#/Node/Bun/Deno pass,
including physical hash preservation on read-only opens. Full sequential local
validation finishes at 05:18:14Z: 1,138 native tests plus six doctests, 698 actual
WASM tests, strict native/WASM Clippy and all eight SQL-optional configurations.
Every coverage denominator remains included and below the whole-project target.
native-lsm-rust{,-before}.txt retain exact commands, counts and limitations.
Publish this verified step, then inspect its own fresh CI. Earlier unchecked
publication gates stay open until that evidence exists.

Published supervision head 3dfbc74 has eight successful workflows, including all
Rust Windows tests. C# Windows alone fails at one movement child after 15.107s.
The retained phases show row creation at 5.312s, original naming at 9.812s and
changed naming at 14.640s. This establishes progress until an aggregate deadline;
it does not establish the underlying filesystem-latency cause. Add deterministic
finite controls before using phase-aware idle supervision with the same 15-second
idle bound, at most 64 consecutive phases, bounded captures and mandatory reap.
