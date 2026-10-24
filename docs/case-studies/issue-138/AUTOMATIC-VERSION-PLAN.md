# Automatic version writes need a retained publication owner

Requirement mapping: R1/R3/R7/R8. Ordinary legacy versioned writes attribute
their transitions only after an inner automatic operation returns successfully.
An error during the commit decision, checkpoint or sidecar attribution can leave
no version transaction holding the original branch and sequence range. This is
a separate path from the already tested explicit version transactions.

- [ ] Reproduce automatic update failures before/after C# commit-marker append,
      first journal flush, data checkpoint, applied marker, final journal flush
      and all three version-publication steps. Verify actual mapped reopening.
- [ ] Reproduce real Rust filesystem failures during decision flush and data
      checkpoint, restoring moved directories before assertions or cleanup.
- [ ] Retain an implicit version owner through all fallible commit/publication
      work. Keep row, named and metadata writes fenced until cleanup succeeds.
- [ ] Cover repeated failures, every existing write path, facade/graph disposal
      and Rust commit/rollback/save, exact branch state, checkout, marker counts
      and a following persistent commit.
- [ ] Preserve automatic undo for write/callback/transition-recording errors
      that occur before commit is attempted, including failed undo retry.
- [ ] Preserve normal asynchronous policy, no-op operations, explicit ownership
      and existing query/CLI behavior in full native/C#/actual-WASM suites.
- [ ] Run strict checks and measured coverage, publish the atomic fix and verify
      all exact-head workflows.

Prepared fault fixtures remain ignored templates until the initial-history
batch finishes. They have not yet established a valid before-fix baseline.
Abrupt process exit after a durable decision still requires a persisted
publication intent and real finite child-process tests. Graceful cleanup of an
implicit owner does not establish that separate crash contract.
