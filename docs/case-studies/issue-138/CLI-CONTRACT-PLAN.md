# Native CLI transaction and configuration contracts

Requirement mapping: R1, R3, R8. Preserve the existing command surface.

- [x] Inspect official native LLVM gaps, the complete main pipeline and existing
  argument, TCP, version-control and error-reporting tests before adding cases.
- [x] Exercise terminal help/version without creating stores; valid and invalid
  transaction policies; before/change/after rendering; branch, tag and checkout
  commands with persistent state and rejected-command invariants.
- [x] Exercise network configuration errors before connection and invalid server
  policies/query combinations/bind errors with finite child cleanup. Do not count
  killed server children as measured execution without a written profile.
- [x] Run focused contracts, full native coverage, strict Clippy and formatting;
  report official totals without exclusions or source-union substitutions.
- [x] Commit, push and verify all exact-SHA workflows; preserve failure logs.

This is additional contract coverage, not a claimed reproduction of a previously
reported CLI defect. A newly discovered failing behavior needs its own before/fix
record. Test helpers must reap child processes on success, failure and deadline.

Exact head `77bd2b7b95f8a39a0c7dd03ed301069af3877f3d` passes all nine
workflows, including Windows/Linux/macOS and fresh-merge checks. See
`dev/log/issues/138/pulls/139/version-owner-ci.txt`. Later changes require new CI.
