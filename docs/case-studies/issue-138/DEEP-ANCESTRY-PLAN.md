# Bounded branch ancestry and path construction

R1/R3/R6/R7/R8: a finite public Rust probe constructs 2,049 valid branches, then
stack-overflows on checkout with a 256 KiB stack. Three bounded child tests per
native language reproduce overflow in checkout, switching and fork validation.
The original real-name probe times out during fixture construction; that result
is inconclusive. A metadata-only adapter isolates traversal from Unicode name
construction and claims no persistence. Real-name shallow semantic controls pass
before any production change in both native languages and actual WASM. The
2,048-parent actual-WASM control also passes before the fix under its own stack
configuration; do not claim a WASM overflow reproduction.

- [x] Reproduce native stack overflow in bounded children before production edits.
  Use finite 2,048-parent input, requested 256 KiB thread stacks, disabled Linux
  core dumps, a 15-second parent deadline, and kill/reap children on expiry.
  Linux Rust children also have a 512 MiB address-space cap; C# child GC is
  capped at 256 MiB without restricting the runtime's virtual reservations.
- [x] Add real-name public semantic controls before edits: nested fork cutoffs,
  per-branch heads, unrelated branches, exact undo/redo ordering, unavailable
  versions rejected unchanged, empty ancestry, following writes and two actual
  native reopenings. Actual WASM runs shared portable controls.
- [x] Replace recursive paths with an iterative chain and one attribution scan;
  preserve root-to-leaf grouping and ascending sequences within each branch.
- [x] Verify a shallow independent path oracle, existing malformed-topology and
  movement-fault controls, deep following writes, and actual WASM.
- [x] Document selected-wrapper costs and release metadata, run all local suites,
  coverage, strict lint, formatting, types and source-size checks sequentially.
- [x] Self-review, atomic commit, push only the prepared branch, update PR 139,
  read fresh feedback and verify all nine exact-head workflows and failure logs.

The descendant fork limit and each branch's own head have distinct roles:
ancestor heads must not truncate records inherited through that ancestor.
Legacy unknown/cyclic path truncation must remain compatible; movement validation
continues to reject broken topology before replay. Bare stores do no path work.
This finite audit does not finish broader issue requirements or complete coverage.

All nine workflows pass on `aa601e2`; see deep-ancestry-ci.txt. Later code needs
fresh exact-head checks.
