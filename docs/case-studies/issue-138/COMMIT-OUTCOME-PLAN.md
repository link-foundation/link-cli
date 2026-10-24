# Commit-decision errors with uncertain delivery

Requirement mapping: R1/R3/R7/R8. A marker append or its first flush can persist
and then return an error. Until both calls return successfully, current legacy
owners do not report a known commit. Cleanup must not infer that the marker was
absent and undo a potentially durable decision.

- [ ] Reproduce errors before/after commit-marker append and first journal flush
      in finite Rust durable-image and C# mapped-store fault fixtures.
- [ ] Cover explicit and automatic owners, repeated failing cleanup, every
      write admission path, and commit/rollback/disposal/save entry points.
- [ ] Retain a commit attempt before invoking the uncertain operation. Keep
      writes fenced and finish that attempt through cleanup; report a known
      commit only after the decision flush returns successfully.
- [ ] Preserve the existing no-new-decision checkpoint retry contract. On an
      uncertain append retry, identify an existing marker before appending a new
      one, or document physical duplicate decisions with the same transaction ID.
- [ ] Verify version cleanup publishes an inner decision that becomes known
      while Rollback or Save resolves uncertainty; preserve its admission if
      publication then fails.
- [ ] Reopen from distinct durable images and mapped files, verify exact row and
      version state, then commit and reopen a following edit.
- [ ] Run native/C#/actual-WASM suites, coverage, lint, metadata and exact-SHA CI.

A failed commit has an unknown outcome. Retrying cleanup may finish that commit;
it must not promise an abort that a persisted marker can later resurrect. These
contracts are separate from abrupt-exit atomicity of legacy branch sidecars and
from the legacy observer adapter's pre-journal mutation window.
