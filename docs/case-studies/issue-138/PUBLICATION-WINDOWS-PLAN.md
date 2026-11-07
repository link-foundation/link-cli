# Windows publication-preservation fixtures

- [x] List fresh runs with timestamps and verify head SHA.
- [x] Download C# run 40148211459 to ci-logs/csharp-40148211459.log.
- [x] Read failure sections; identify the exact exception and affected fixtures.
- [x] Move binary snapshots outside all mapped-store and borrower lifetimes.
      Keep exclusive mapping and exact sidecar/names preservation assertions.
- [x] Run focused and full local C# suites, formatting and source-size checks.
- [x] Preserve evidence and prepare an independent fixture correction.
- [x] Push prepared branch and verify new exact-head Windows/Linux/macOS CI
      rather than rerunning stale code.

Run created 2026-10-25T00:49:47Z, head
2a1c9e3205046bdb6b22043903fc145b4f278647, finished 00:53:52Z.
Ubuntu, macOS, fresh-merge, formatting and changeset checks pass. Windows runs
1,440 tests: 1,430 pass and ten new preservation fixtures fail. Downloaded log
lines 15866-15876 and 16080-16091 show File.ReadAllBytes opening branches.links
while NamedTypesDecorator still holds its exclusive mapping, at fixture lines
66 and 98. All nine contradiction cases and the shared-vector fixture fail
before reaching their intended preservation assertion. Summary: 18016-18023.

The fixture now creates/corrupts and closes all stores, snapshots bytes, opens
fresh stores to validate recovery/rejection, closes them, then compares bytes.
This also tests actual reopening instead of inspecting only a live mapping.
No production locking behavior is weakened. Windows evidence is retained in
version-publication-windows-before.txt under the PR's committed log directory.

The corrected ten fixtures and three retained-history contracts pass together
(13 focused tests). Full local Debug/Release pass 1,443 tests each, zero skips;
formatting, changeset and source-size checks pass. The three extra tests belong
to the adjacent retained-history commit. Fresh Windows CI passes on exact head 1f742c5. All ten corrected fixtures
and the complete 1,443-test suite pass; Linux/macOS and package checks pass too.
See version-publication-windows-after.txt for exact run and original log lines.
