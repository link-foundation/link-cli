# Persisted initial checkpoints across native page providers

Requirement mapping: R1/R3/R6/R7/R8/R9/R12. Continue all wider unmet criteria.

- [x] Separate the preceding volatile checkpoint/delta/replica composition
      evidence from persisted cross-language reopening.
- [x] Author an independent engine3 seed at revision 40 with 32 named rows,
      references, duplicate pairs/names and allocator holes 33 through 40.
- [x] Run the new seed-selection control against the preceding examples;
      preserve their ordinary revision-zero output as a missing-example baseline.
- [x] Add explicit fixture selection to each existing native page example.
      Seed only an empty logical journal and reject a different existing prefix.
- [x] Verify exact checkpoint bytes, retained revision 40, unavailable revision
      39, Unicode/BOM/NUL names, references, pins, allocator gaps and following
      atomic groups in Rust, C# and actual WASM/JS.
- [x] Run all 48 single/mirrored, state, logical/physical encoding and constraint
      combinations through Rust, Node, Bun, C#, Deno and Rust again. Include
      deliberate closed-copy damage and repair before every mirrored handoff.
- [x] Preserve the ordinary revision-zero interoperability control, regenerate
      fixtures, check native examples/lint/format and add normal-CI execution.
- [ ] Record evidence, update the PR, commit and push only the prepared branch,
      then inspect all fresh exact-head workflows and native matrices.

The baseline does not establish a library defect: the old examples ignored the
new fixture selector and started ordinary journals at zero. The delta correction
is already implemented and verified separately in CHECKPOINT-DELTA-PLAN.md.
This batch adds a persistent integration contract without changing production
engine, page, file-device or physical delta code.

Import creates an independent database from supplied checkpoint bytes. This
neither replaces an existing acknowledged file nor coordinates future allocations
with the source database. Atomic local replacement, old object reclamation,
hardware power-loss verification, whole-project coverage and every wider unmet
criterion remain unfinished.
