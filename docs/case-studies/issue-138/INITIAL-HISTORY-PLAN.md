# Initial history must contain committed transitions only

Requirement mapping: R1/R3/R7/R8. The initial branch currently attributes every
pre-existing journal record and selects LastLoggedSequence as its head. That
includes explicit rollbacks and pending transactions undone during recovery.
The next audit must establish the failure before changing attribution.

- [ ] Reproduce all-aborted, trailing-abort and interleaved-abort histories in
      both native languages, with explicit rollback and pending recovery.
- [ ] Reject constructing the version layer over an existing live transaction
      before writing sidecar metadata; preserve the borrowed C# owner.
- [ ] Reject checkout and branch creation at aborted sequence numbers; retain
      exact committed rows, tags and heads across checkout and reopening.
- [ ] Attribute only transitions with a known committed decision and choose the
      last such sequence, or zero if none exists. Preserve actual sparse gaps.
- [ ] Execute portable contracts in actual WASM; use real files on native.
- [ ] Audit the behavior of sidecars written before this change separately.
      Preventing new incorrect attribution does not repair already published
      invalid metadata or data changed by an earlier aborted checkout.
- [ ] Run the full relevant suites, strict checks and measured coverage; publish
      the atomic change and verify every exact-head workflow.

This audit is independent of abrupt exit between a journal decision and legacy
sidecar publication. That durability window needs a persisted publication intent
and real process-exit tests, rather than an in-process cleanup claim.
