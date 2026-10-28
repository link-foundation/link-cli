# Portable public recovery and diagnostics

- [x] Run all shared journal vectors through the public engine on native/WASM.
- [x] Verify Unicode, deletion, empty names, final u32 allocation and invalid records.
- [x] Verify enabled diagnostics, admission, private writes and recovery under four policies.
- [x] Run focused/full native and actual WASM tests; collect official coverage.
- [x] Run native/WASM strict Clippy and formatting before an atomic commit.
- [x] Preserve evidence and update requirements/PR validation.
- [x] Verify fresh exact-SHA CI after publication (a236c49: all nine workflows pass).

The initial link-check attempt received GitHub HTTP 502 for the existing release
index in both language READMEs. A direct fresh request returned 200; retrying the
unchanged workflow passed. This was a transient upstream response, not a URL fix.

Evidence: `dev/log/issues/138/pulls/139/portable-recovery.txt`.
Complete native/WASM coverage remains below the requested 100%.
