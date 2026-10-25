# Automatic version writes need a retained publication owner

Requirement mapping: R1/R3/R7/R8. The baseline ordinary legacy versioned writes attributed
their transitions only after an inner automatic operation returns successfully.
An error during the commit decision, checkpoint or sidecar attribution could leave
no version transaction holding the original branch and sequence range. This is
a separate path from the already tested explicit version transactions.

- [x] Reproduce automatic update failures before/after C# commit-marker append,
      first journal flush, data checkpoint, applied marker, final journal flush
      and all three version-publication steps. Verify actual mapped reopening.
- [x] Reproduce real Rust filesystem failures during decision flush and data
      checkpoint, restoring moved directories before assertions or cleanup.
- [x] Retain an implicit version owner through all fallible commit/publication
      work. Keep row, named and metadata writes fenced until cleanup succeeds.
- [x] Cover repeated failures, every existing write path, facade/graph disposal
      and Rust commit/rollback/save, exact branch state, checkout, marker counts
      and a following persistent commit.
- [x] Preserve automatic undo for write/callback/transition-recording errors
      that occur before commit is attempted, including failed undo retry.
- [x] Preserve normal asynchronous policy, no-op operations, explicit ownership
      and existing query/CLI behavior in full native/C#/actual-WASM suites.
- [x] Run strict checks and measured coverage.
- [ ] Publish the atomic fix and verify all exact-head workflows.

Before-fix evidence: all 32 C# cases fail, including accepted writes after a
sidecar failure and missing cleanup through owner/graph disposal. All eleven
Rust functions fail at the first data-checkpoint schedule: commit/rollback find
no version owner, and save leaves head 2 rather than the committed head 3. The
complete normalized behavioral logs are retained under the PR evidence. After
the ownership change and added controls, 153 focused C# contracts and 59 focused
Rust contracts pass. Full C# Debug/Release each pass 1,307 tests, native Rust
passes 653 standard tests plus the stdout harness, and actual WASM passes 345.
Strict checks and measured coverage are preserved in automatic-version.txt.
Project-wide coverage remains below 100%; fresh exact-head CI is required.
Abrupt process exit after a durable decision still requires a persisted
publication intent and real finite child-process tests. Graceful cleanup of an
implicit owner does not establish that separate crash contract.
