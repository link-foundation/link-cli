# Reclamation admission and provider failures

Objective: verify the uncovered public admission and failure paths for both
whole-object and chunked reclaiming journals, without changing their protocol
or reducing the measured production scope. This advances R1/R7/R8; the full
acceptance inventory remains open.

- [x] Inspect complete LLVM coverage and trace each uncovered predicate through
      the public API and maintenance-capable provider contract.
- [x] Add minimum native Rust, C# and JS controls for short/long guard and update
      arrays, asserting rejection before any provider request and following use.
- [x] Inject generation-zero payloads in catalog shards, epochs and objects;
      require failure without publication, deletion or premature object reads.
- [x] Reject uppercase hexadecimal digits in both epoch and hash fields of live
      pointers and unreferenced scanned objects before downloads or deletion.
      Retain an eligible orphan so the no-delete assertion is nonvacuous.
- [x] Change only the catalog generation while its referenced object disappears;
      require failure after one fresh catalog snapshot without retrying the same
      object. Restore the provider and verify following reads and writes.
- [x] Run focused contracts, full relevant suites, complete coverage and local
      repository checks, retaining precise totals and limitations.
- [x] Review, commit and publish the useful test step, inspect fresh exact-head
      CI, and update the PR without claiming wider completion.

These are coverage-driven controls for existing behavior. They do not establish
a new production bug, physical durability, unbounded scale or 100% project
coverage. Both layouts use the public APIs and the existing deterministic
maintenance-provider fixtures. Actual WASM executes the shared Rust contracts;
Node, Bun and Deno also execute the JS provider controls.

The first complete native and actual-WASM runs pass 1,163 and 723 tests. After
strengthening the eligible-orphan fixture, rerun the native maintenance target
while preserving the complete accumulated LLVM report, rerun all actual WASM,
and rebuild/rerun C# Debug. `validate-admission-orphans.sh` records that final
fixture verification separately. The primary driver also executes C# Release
and all three JS runtimes after the fixture change. Neither a source-coordinate
union nor a focused report substitutes for the complete project denominators.

Final local verification completes 2026-10-27T08:23:27Z. All 50 native
maintenance controls, all 723 actual-WASM tests and all 2,447 C# Debug tests
pass with the strengthened orphan fixture. Release passes 2,447; Node passes
644 with enforced production 100% lines/branches/functions. All four controls
also pass in Bun/Deno. All 221 repository checks, 658 source limits, strict
format/lint/types, changeset, 126 dependency declarations and workflow checks
pass. The local root-level CI-log layout failure and unchanged-check rerun are
recorded under PHASE-CONTEXT-PLAN.md. reclamation-admission.txt preserves all
five LLVM metrics and full C# denominators; none is excluded or declared 100%.

Published head 7e9799a passes all nine workflows, including all native Rust and
C# test platforms. `phase-context-ci-after.txt` records exact commit/run
timestamps and links. This closes verification for these controls; subsequent
source changes and every wider unmet criterion require their own evidence.
