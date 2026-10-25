# Legacy names in transactions and history

R1/R3/R6/R7/R8: before this audit, the legacy transaction/version wrappers
forwarded `SetName` and `RemoveName` after checking admission, without retaining
name state. The reproductions below distinguish admission from ordinary rollback,
deletion undo and name-only version attribution.

- [x] Add minimal failing native contracts for rename, remove, moving a unique
      name, deleting a named row, cascade deletion and name-only history.
      Run the portable contracts through actual WASM too.
- [x] Trace recording, commit/rollback, retention, version publication and
      movement recovery. Preserve existing row codecs and public APIs; define
      an explicit portable representation for any added name metadata.
- [x] Implement matching name recording/replay in native Rust and C#. Preserve
      unique-name reassignment, Unicode and existing external-reference names.
      Keep bare stores free of optional transaction bookkeeping.
- [x] Test failures before/after recording, data/name flush and publication;
      retain retry ownership, reject malformed metadata before mutation and
      verify repeated actual reopening and following edits.
- [x] Cover automatic/explicit transactions, rollback, version movement,
      retention and compiled WASM. Document exact cost and compatibility.
- [ ] Run complete local checks and measured coverage sequentially, add release
      metadata, review, commit, push only the prepared branch and verify all
      exact-head workflows. Keep broader acceptance gaps open.

Existing in-place row mutation before its observer journal append is a separate
crash window. Fixing names does not imply that window or physical power-loss
behavior has been verified.


Initial native evidence: all six Rust contracts and all six C# contracts fail.
The old names are absent or still changed after rollback; name-only history
keeps its previous sequence. `named-history-{rust,csharp}-before.log` preserves
these real assertion failures. A first C# compile attempt lacked the namespace
for the existing `Exists` extension; after that fixture-only correction, all
six tests compile and fail for the intended behavior.

The implementation preserves the existing Rust `GenericTransition` value's
`Copy` contract and row serialization API. An additional complete `JournalRecord`
contains the row and optional name changes. Ordinary unnamed row records remain
v1. Recovery, archives and persisted version-movement intents retain the
complete record; row-only serialization never substitutes for it internally.
Shared JSON fixtures test canonical v2 fields, Unicode/delimiters/NULs, empty
names, absent rows, address zero, wider addresses and rejection paths. C# keeps
its existing unpaired UTF-16 domain; Rust rejects those inputs before mutation.

Twenty-four finite fault schedules per native implementation capture row, name and
journal images independently at the actual failure before cleanup. They verify
reopening twice and following edits. The C# fixture initially restored journal
rows using `Create(null, null)`, whose return is not an allocated row address;
using the production `CreateAndUpdate(0, 0)` pattern corrects that fixture.
Its failure output was retained separately from production regressions.

The zero-address/empty-name C# contract exposes a distinct production defect:
lookup conflated owner zero with absence, leaving two owners after a unique-name
move. A presence-returning lookup fixes it and preserves the existing public
sentinel-returning API. The before log contains the intended failed assertion.

Native movement failure matrices and bounded abrupt process exits now carry
complete names as well as rows. Additional name-only exit cases verify automatic
and explicit success, post-decision checkpoint failure, pending undo and rollback.
Sized/chunked tests verify reopening and independent replay of complete archives.
See [usage and costs](../../TRANSACTION-NAMES.md).

Actual WASM also reproduces all six assertion failures; its before log is
`named-history-wasm-before.log`.

The callback audit adds two actual-WASM failures: an inner row decorator's
resulting name is replayed incorrectly, and a required post-effect capture is
never attempted. C# adds four failures and one passing create control: update
and delete callback names are lost on checkout, and automatic/explicit owners
do not observe the injected post-effect capture failure. Preserve these logs
as `named-history-callback-{wasm,csharp}-before.txt` in PR evidence. Capture
actual resulting names; retain every observed undo before a fallible append
even if the capture fails. C# drains pending rows before reentrant name writes
to preserve their order. The initial callback draft instead recorded every
callback immediately; full validation caught fourteen lost cascade callbacks
under journal faults and one changed retention count. Keep ordinary callback
coalescing and delivery, and drain only for a nested name write. Failed capture
retains an explicit owner; automatic cleanup rolls back its recorded effects.
