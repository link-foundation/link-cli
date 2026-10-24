# C# legacy known-commit checkpoint ownership

Requirement mapping: R1/R3/R7/R8. The Rust fault matrix exposed a lifecycle
pattern that must be audited independently in the C# implementation. Current
C# synchronous commit marks the handle committed and releases `_current` before
data/applied-marker persistence. A later handle Commit is a no-op. Existing
version retry tests establish branch publication, not generic checkpoint retry.

- [x] Reproduce each checkpoint phase with explicit and automatic transactions,
      assert a persisted commit decision and an incomplete applied sequence.
- [x] Require handle Commit/Rollback/Dispose and decorator Rollback to finish
      that known outcome before admitting another foreground write.
- [x] Fence row and named write entry points without hiding the readable outcome.
- [x] Verify exact checkpoint completion, no new decision marker, following
      committed edits and persistent reopening.
- [x] Preserve asynchronous policy and shutdown contracts in the full suite.
- [x] Separately audit out-of-order applied-marker recovery after asynchronous
      checkpoint faults; see RECOVERY-ORDER-PLAN.md.
- [x] Run focused and complete suites, lint/format and coverage.
- [x] Commit, push and verify fresh exact-SHA CI.

Do not infer abrupt-exit sidecar atomicity or power-loss ordering from cleanup
retry tests. Tests need finite schedules and existing mapped-store fixtures.

Before-fix evidence: all 96 commit-checkpoint cases fail, plus sixteen named
write cases after failed rollback; sixteen existing commit/row controls pass.
The six Rust named transaction/version bridge cases also fail before their
shared admission guard. Tests verify repeated failure, exact readable outcome,
checkpoint completion without another decision, following edits and reopening.
Complete local verification passes: C# Debug/Release 1,106 each, native Rust
601 standard tests plus stdout harness, and actual WASM 315. Strict lint,
formatting, changeset and source-size checks pass. Exact coverage and retained
before-fix evidence are in `dev/log/issues/138/pulls/139/checkpoint-owner.txt`.
All nine workflows pass on SHA `8429c53`; exact run URLs are retained in
`dev/log/issues/138/pulls/139/checkpoint-owner-ci.txt`.
