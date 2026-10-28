# Version-owner admission after failed rollback

Requirement mapping: R1/R3/R8. Follow the lifetime audit through the version owner,
whose rollback finally block currently drops its active transaction even when
the inner owner still requires rollback retry. Dispose also drops that reference
before attempting cleanup. Rust already clears its active owner only on success.

- [x] Trace native version rollback, explicit owner disposal and whole-graph
  disposal, including transaction rollback's eight existing fault phases.
- [x] First reproduce metadata admission and missing cleanup retry through all
  three C# entry points with finite mapped-store tests and preserved before logs.
- [x] Retain version admission until inner rollback reaches a terminal outcome;
  preserve original failures and allow cleanup retry through the owning facade.
- [x] Verify branch/tag/checkout/begin fencing, retry, exact baseline reopening
  and a following committed version transaction. Include terminal retention faults
  if their failure can happen after IsRolledBack is set.
- [x] Run focused/full Debug and Release, coverage, formatting and release metadata.
- [x] Commit/push separately from CLI-only coverage and verify fresh exact-SHA CI.

All 24 pre-fix cases fail at the first admission assertion: Branch("blocked")
unexpectedly succeeds after the injected rollback exception. The matrix covers
eight undo/persistence phases through explicit rollback, owner disposal and
whole-graph disposal. The first corrected run passes all 24 cases and six facade
lifetime controls. The version finally block now checks the inner terminal state;
Dispose retains its reference until that rollback operation clears it.

An additional real retention control leaves an older committed transition
uncheckpointed, then fails retention's retry after a newer rollback completes.
It checks that terminal errors release admission instead of leaving the facade
stuck. This is an actual mapped-store checkpoint failure, independent of archive
errors that retention catches. Rust already preserves its active owner with error propagation
before clearing; it needs no matching production change.

Exact head `77bd2b7b95f8a39a0c7dd03ed301069af3877f3d` passes all nine
workflows, including Windows/Linux/macOS and fresh-merge checks. See
`dev/log/issues/138/pulls/139/version-owner-ci.txt`. Later changes require new CI.
