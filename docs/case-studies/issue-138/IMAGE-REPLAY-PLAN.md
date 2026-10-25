# Undo image replay compatibility and remaining coverage

R1/R7/R8 require recovery to detect incomplete restoration, including adapters
which return success without applying a requested row or name. The official
row-image coverage run leaves that final verification failure unexecuted.

- [x] Inspect the complete native/actual-WASM LLVM and C# Coverlet reports.
- [x] Add finite row/name no-effect adapters. Fail cleanup twice, assert
      write/admission fences, recover independently persisted partial images,
      repair the adapter and verify cleanup plus a following edit.
- [x] Recover legal unnamed images while preserving unrelated external names.
- [x] Verify successful wide-address recovery with sparse compatible adapters;
      never reserve a dense 2^32-address fixture.
- [x] Exercise correction records for truly unreported allocations/deletions,
      rather than replacement at the same address alone.
- [x] Investigate any reproduced failure before changing production code.
      These new compatibility controls pass without a behavior correction.
- [x] Run focused checks then required complete suites and document exact coverage.
- [ ] Review, commit, push and inspect all fresh exact-head workflows.

Focused checks pass: 20 native Rust tests (one bounded child entry ignored in
direct enumeration), 112 C# cases and 19 actual-WASM cases. The Rust sparse
control also covers selected unnamed owners. Complete local suites pass: 823
native, 481 actual WASM, 1,909 C# per Debug/Release configuration, 111 JS and 217
repository tests. Formatting, lints, types, source sizes, release metadata, runtime
conformance and persistent examples pass. See image-replay.txt for official
coverage; exact-head publication and CI remain pending.

Logical images require compatible replay and readable storage. These tests do
not establish physical page repair or power-loss safety. Complete coverage,
efficient WAL/state structures, finer composition and wider strategies remain
active acceptance criteria.
