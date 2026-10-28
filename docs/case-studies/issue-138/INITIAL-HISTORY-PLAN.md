# Initial history must contain committed transitions only

Requirement mapping: R1/R3/R7/R8. The baseline initial branch attributed every
pre-existing journal record and selected LastLoggedSequence as its head. That
included explicit rollbacks and pending transactions undone during recovery.
The audit established these failures before changing attribution.

- [x] Reproduce all-aborted, trailing-abort and interleaved-abort histories in
      both native languages, with explicit rollback and pending recovery.
- [x] Reject constructing the version layer over an existing live transaction
      before writing sidecar metadata; preserve the borrowed C# owner.
- [x] Reject checkout and branch creation at aborted sequence numbers; retain
      exact committed rows, tags and heads across checkout and reopening.
- [x] Attribute only transitions with a known committed decision and choose the
      last such sequence, or zero if none exists. Preserve actual sparse gaps.
- [x] Execute portable contracts in actual WASM; use real files on native.
- [x] Audit sidecars written before this change: reject attributed records
      known to be uncommitted, including recovered pending records. Leave files
      intact; do not claim to repair already altered data. Missing retained
      history remains a separate unfinished audit.
- [x] Run the full relevant suites, strict checks and measured coverage.
- [x] Publish the atomic change and inspect every exact-head workflow.
- [x] Resolve C# Windows async scheduling failures and verify fresh CI; a bounded
      child proves worker starvation and all nine workflows pass on 24554b0.
      See ASYNC-WORKER-PLAN.md and async-worker-ci.txt.

This audit is independent of abrupt exit between a journal decision and legacy
sidecar publication. That durability window needs a persisted publication intent
and real process-exit tests, rather than an in-process cleanup claim.

Before-fix evidence: all nine C# cases and all nine Rust cases fail after fixture
compilation issues were corrected. All-aborted histories expose head 2 instead
of zero; trailing aborts expose 3 instead of committed head 2; interleaved aborts
are reachable. Existing invalid sidecars and construction over an open owner are
accepted. Complete normalized behavioral-failure logs are retained under
`dev/log/issues/138/pulls/139/`; fixture compilation errors remain experiment
logs and are not used as the baseline. All nine Rust cases now pass together
with known/uncertain checkpoint retry and signed sparse-history controls.

All nine new C# cases pass within 78 focused contracts. Full C# Debug/Release
each pass 1,256 tests, native Rust passes 638 standard tests plus the stdout
harness, and actual WASM passes 341. Strict checks pass. Measured coverage and
limits are preserved in initial-history.txt; complete project coverage remains
below 100%. Eight exact-head workflows pass at 1c1ef38; C# Windows fails two async waits.
See initial-history-ci.txt and ASYNC-WORKER-PLAN.md; this is not an all-green SHA.
