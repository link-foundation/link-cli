# Missing legacy history during checkout

Requirement mapping: R1/R3/R7/R8. Bounded legacy retention can remove transition
payloads while branch metadata still references their sequences. A checkout
must establish that every changed transition is available before changing rows
or the selected branch. A missing shared prefix requires no replay.

- [x] Reproduce missing undo through checkout and branch switching, plus missing
      redo after an available undo. Use a one-transition retained log; preserve
      rows, selected branch, head and branch inventory on failure.
- [x] Resolve all changed undo and redo records before the first mutation.
      Avoid cloning/scanning the whole log once per changed sequence.
- [x] Preserve no-op checkout and available suffix moves over unavailable shared
      prefixes. Verify following edits and actual native reopening.
- [x] Run the same semantic regressions in actual WASM; run full local suites,
      coverage and strict checks before publishing an atomic commit.
- [x] Update retention documentation, release fragments and review evidence.
- [ ] Push only the prepared branch and verify fresh exact-head CI.

This audit concerns unavailable records. Atomic recovery from a failure while
applying an available diff is a separate requirement and remains unfinished.

All three regressions per native language first fail because checkout returns
success after skipping missing records. The Rust correction and thirteen
ordinary version controls pass (16 tests); C# focused verification also passes
(18 tests). Full native/C#/actual-WASM validation passes; measured coverage is recorded
separately from test counts in retained-history.txt.
