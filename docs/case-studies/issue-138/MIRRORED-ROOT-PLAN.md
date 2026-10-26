# Selected checkpoint root mirroring

Requirement mapping: R1/R3/R5/R6/R7/R8/R9/R10/R11/R12. Continue the
complete acceptance inventory; this batch addresses root redundancy only.

- [x] Reproduce the missing portable adapter in actual Rust/C#/JS test targets.
- [x] Add an independently selected two-copy root device. Persist root-mode bit
      2 separately from bank-mode bit 1; reject mode conversion before mutation.
      Keep existing single-root formats, APIs and costs unchanged.
- [x] Validate both copies, slot parity, identity, epoch and unknown formats
      before selecting. Stage repair until the selected committed bank validates.
      Recheck repair sources and synchronize every repair; fence uncertain I/O.
- [x] Add independent format vectors, post-ack single-copy damage controls,
      contradictory/invalid copies, changed-source and invalid-bank controls.
      Enumerate finite before/after publication and recovery I/O faults, verify
      complete old/new recovery and following acknowledged groups.
- [x] Extend native owners to acquire all selected raw roots/banks before any
      repair. Preserve reverse cleanup, root-last release and uncertain fences.
- [x] Extend actual-WASM/runtime tests, native process cuts and cross-provider
      persistent handoffs. Keep SQLite optional and old controls passing.
- [x] Run full local checks and measured coverage sequentially; document mode
      compatibility, selected costs and remaining uncovered requirements.
- [ ] Review diffs/comments/main, commit to the prepared branch, update PR #139
      and verify all exact-head CI/native platforms. Keep the PR draft until
      every wider criterion is verified.

The preceding native ownership batch is committed as 600cf5f and its separate
test correction as 6a0f7c9. All nine workflows pass on the latter exact head;
native-checkpoint-ci-fix.txt records native platform results and timestamps.
That evidence is independent of this batch. Physical power loss and arbitrary damage to both
root copies remain outside a single-copy repair guarantee.

Portable missing-feature controls fail at 600cf5f; native factory controls fail
at 6a0f7c9. mirrored-root-before.txt records exact compiler/runtime errors and
the saved clean-baseline reproducers. Eleven new portable controls per language
cover independent vectors, every byte of either root copy in both bank modes,
finite publication/recovery faults and zero-mutation rejection. Focused native
tests also cover all four owning configurations, every selected writer fence,
reverse cleanup and bounded process cuts. The complete sequential validation
driver passes all seven stages, ending 2026-10-26T20:22:23Z. Native Rust passes
1,028 tests plus six doctests, actual WASM 631, C# 2,264 in each configuration,
JS 261 at enforced 100% production coverage and repository checks 221. Each
native language executes 258 before/after process exits. Compaction passes
96 configurations/576 writers; ordinary/imported controls retain 48/288 each.
mirrored-root.txt records complete coverage denominators and broader gaps.
Fresh exact-head CI is required after publication.
