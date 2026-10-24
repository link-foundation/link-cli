# Native Rust version owner after a durable commit

Requirement mapping: R1/R3/R8. Audit the Rust explicit version owner separately
from the C# transaction/disposal matrix. A journal commit can become durable
before the data checkpoint returns an error; cleanup must finish publication
without undoing that known outcome or admitting another write.

- [x] Reproduce with a finite real filesystem fault, asserting the persisted
  commit marker rather than relying on an operating-system error message.
- [x] Exercise Commit, Rollback and Save cleanup plus every version write API.
- [x] Keep version ownership until publication succeeds; expose only the
  internal committed-state query needed for known-outcome recovery.
- [x] Verify exact reopened branch/head/tag and main/topic checkout rows, then
  commit and reopen a following transaction.
- [x] Run focused tests, full native/actual-WASM validation, lint/format and
  coverage.
- [ ] Commit, push and review fresh exact-SHA CI.

Moving only the data directory makes checkpoint file creation fail after the
separate journal's commit flush. The directory is restored before assertions
or cleanup. These native stores keep in-memory state and open paths on save;
this fixture does not rely on permissions or mapped-handle renaming behavior.
Abrupt process exit before legacy sidecar publication remains a separate audit.

The eleven native tests initially have ten failures and one Commit control.
The final cases also repeat the checkpoint failure, retain admission, complete
the applied markers, then retry successfully. Eight write entry points include
SetName and RemoveName. Both named-write bypasses are independently reproduced
and corrected in C#; its existing 48 phase/cleanup controls still pass.

The underlying generic Rust transaction owner has the same early-release bug.
Nine shared native/actual-WASM tests first all fail. The final matrix executes
108 finite schedules: six checkpoint phases (before/after data flush, applied
marker and final journal flush), explicit and automatic transactions, three
cleanup methods and six write methods. It retains the original pending batch,
retries the checkpoint without another commit marker, rejects writes/recovery
and verifies a later edit survives reopening from separate persisted images.
The normal commit path keeps its existing O(changes) checkpoint work; it does
not scan the complete log or clone the pending batch to support retries.

Separate native before tests demonstrate public recovery replacing live
transaction/version state. Recovery now rejects an active owner before clearing
any state, then remains available after the transaction finishes. The same
contracts execute in actual WASM. Both equivalent C# regressions first fail;
their recovery methods now guard the owner before clearing live state.

Local verification totals

Full native Rust: 595 standard tests plus stdout harness; C#: 994 each in
Debug/Release; actual WASM: 315 tests. Both strict Clippy runs, formatting and
release metadata pass. Exact coverage and before/after evidence:
`dev/log/issues/138/pulls/139/version-owner-clock.txt`. CI remains pending.
