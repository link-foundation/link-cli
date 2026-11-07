# Merkle leaf address admission coverage

Continue R1/R7/R8 after the complete Merkle validation on published 283b86d.
The complete native and actual-WASM reports retain four uncovered branch edges
in the Merkle module, including the empty and overlong decimal-address guards.
These are missing test controls, not evidence of a production admission bug.

- [x] Preserve complete baseline report hashes, component denominators and
      the actual address-guard counts before adding controls.
- [x] Add empty and 21-digit address mutations to every independently generated
      canonical leaf frame. Keep the valid original root/proof/version bytes;
      require explicit rejection with a useful diagnostic for each mutation.
      Share these contracts with the actual-WASM harness.
- [x] Run native and actual-WASM contracts, complete native/WASM suites and
      full production coverage sequentially with one compiler and bounded heaps.
      Preserve every denominator and distinguish unreachable closures from
      measured covered branches; do not fabricate values to reach invalid
      private states.
- [x] Run strict native/WASM lint, formatting and repository checks before a
      forward commit. Retain exact commands, test counts and before/after data.
- [ ] Push only the prepared branch and inspect fresh exact-head platform CI;
      download failed logs, compare commit/run timestamps and verify actual
      C# platform artifacts. Keep PR139 draft while any wider criterion is open.

This step does not change source behavior or reduce a coverage threshold.
Complete native, C# and linked-WASM project coverage, generic mapped ownership,
ordinary causal sessions, remaining algorithms/composition and scaling criteria
remain unfinished. The complete inventory in REQUIREMENTS.md remains active.
