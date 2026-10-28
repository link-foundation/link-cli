# Creation callback audit

Continue R1 with mapped stores that resolve duplicates or reject an update after
allocating the new row. A returned address does not prove that a new row remains.

- [x] Add minimal rollback/recovery and automatic/explicit rejection tests first.
- [x] Reproduce four failures; retain the two passing composite-creation controls.
- [x] Forward mapped creation callbacks and record all observed changes on error.
- [x] Verify equivalent C# creation/uniqueness transaction boundaries (four controls pass).
- [x] Run full native suites, coverage, formatting and actual WASM checks.
- [x] Publish, inspect exact-SHA CI and update PR evidence.

The duplicate fixture has three original rows and at most one temporary row.
Before the fix, rollback and pending-transaction recovery delete the duplicate's
surviving row and its dependent. Validator rejection leaves a temporary row even
after rollback. Composite creation with an existing empty row already passes;
no defect is claimed for those two controls.

Commit `23644f1385a4b7710ee5eb34a0e307e3f55b94dd` passes all exact-SHA CI
checks (37 successful, 10 skipped, one neutral). Runs created 2026-10-24
20:15:47 UTC include Rust 40128987034, C# 40128987067 and WASM 40128987143.
All three native platforms, fresh-merge checks and actual WASM checks pass.
