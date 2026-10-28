# Commit-decision errors with uncertain delivery

Requirement mapping: R1/R3/R7/R8. A marker append or its first flush can persist
and then return an error. Until both calls return successfully, current legacy
owners do not report a known commit. Cleanup must not infer that the marker was
absent and undo a potentially durable decision.

- [x] Reproduce errors before/after commit-marker append and first journal flush
      in finite Rust durable-image and C# mapped-store fault fixtures.
- [x] Cover explicit and automatic owners, repeated failing cleanup, every
      write admission path, and commit/rollback/disposal/save entry points.
- [x] Retain a commit attempt before invoking the uncertain operation. Keep
      writes fenced and finish that attempt through cleanup; report a known
      commit only after the decision flush returns successfully.
- [x] Preserve the existing no-new-decision checkpoint retry contract. On an
      uncertain append retry, identify an existing marker before appending a new
      one, or document physical duplicate decisions with the same transaction ID.
- [x] Verify version cleanup publishes an inner decision that becomes known
      while Rollback or Save resolves uncertainty; preserve its admission if
      publication then fails.
- [x] Reopen from distinct durable images and mapped files, verify exact row and
      version state, then commit and reopen a following edit.
- [x] Run native/C#/actual-WASM suites, coverage, lint and metadata.
- [x] Verify fresh exact-head CI after publishing this batch.

A failed commit has an unknown outcome. Retrying cleanup may finish that commit;
it must not promise an abort that a persisted marker can later resurrect. These
contracts are separate from abrupt-exit atomicity of legacy branch sidecars and
from the legacy observer adapter's pre-journal mutation window.

Before-fix evidence: C# has 76 failures and four passing controls across 80
mapped-store cases. Shared Rust decision contracts have nine failures; real
filesystem version contracts have ten failures and one passing commit control.
Repeated cleanup can undo 21 back to 11, subsequent writes are admitted, and
version rollback/save leaves the committed branch head unpublished. The generic
append-only fixture also observes duplicate physical commit decisions. These
are behavioral failures, not harness or construction errors.

After: all 96 new C# cases pass; eleven shared Rust functions execute 88 finite
fault schedules and eleven filesystem version functions pass. Full C# Debug/
Release each pass 1,247 tests; native Rust passes 629 standard tests plus its
stdout harness; actual WASM passes 332 tests. Exact coverage and limits are in
commit-decision.txt beside the complete before-fix logs. All nine exact-head
workflows pass on `7c0da30`; see commit-decision-ci.txt for URLs/timestamps.

Automatic version-facade writes that fail before attribution still need their
own retained-publication audit. The explicit version contracts here do not
establish that separate path, abrupt-exit sidecar atomicity or complete coverage.
