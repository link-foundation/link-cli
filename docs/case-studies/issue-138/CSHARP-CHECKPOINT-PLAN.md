# C# legacy known-commit checkpoint ownership

Requirement mapping: R1/R3/R7/R8. The Rust fault matrix exposed a lifecycle
pattern that must be audited independently in the C# implementation. Current
C# synchronous commit marks the handle committed and releases `_current` before
data/applied-marker persistence. A later handle Commit is a no-op. Existing
version retry tests establish branch publication, not generic checkpoint retry.

- [ ] Reproduce each checkpoint phase with explicit and automatic transactions,
      assert a persisted commit decision and an incomplete applied sequence.
- [ ] Require handle Commit/Rollback/Dispose and decorator Rollback to finish
      that known outcome before admitting another foreground write.
- [ ] Fence row and named write entry points without hiding the readable outcome.
- [ ] Verify exact checkpoint completion, no new decision marker, following
      committed edits and persistent reopening.
- [ ] Preserve asynchronous policy and shutdown contracts; separately audit
      out-of-order applied-marker recovery after asynchronous checkpoint faults.
- [ ] Run focused and complete suites, lint/format, coverage and exact-SHA CI.

Do not infer abrupt-exit sidecar atomicity or power-loss ordering from cleanup
retry tests. Tests need finite schedules and existing mapped-store fixtures.
