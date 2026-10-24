# Portable legacy transaction timestamps

Requirement mapping: R1/R3/R7/R8. Execute the existing in-memory transaction
and version-control API in actual WASM as well as native Rust. Successful
compilation cannot establish support for a platform's clock functions.

- [x] Add shared finite transactions/version-history tests and reproduce both
  WASM failures before changing production code.
- [x] Replace the unsupported clock with a maintained portable component;
  preserve native timestamps, unique transaction ids and the public API.
- [x] Share existing ordinary commit/rollback/retention contracts across native
  file stores and actual-WASM memory stores; keep filesystem-only tests native.
- [x] Run focused tests, complete native/actual-WASM suites, strict lint and
  coverage; record exact measured totals without excluding linked sources.
- [ ] Commit, push and inspect fresh exact-SHA CI.

Before: both actual-WASM tests panic in `std::time::SystemTime::now` with
`time not implemented on this platform`. The preserved verbose log is
`experiments/issue-138/portable-clock-wasm-before.log`, lines 14583–14706.
One test checks real Unix milliseconds and distinct transaction ids through
commit and rollback. The other checks exact delimiter/Unicode branch and tag
names, metadata recovery, checkout and main/topic row membership.

Primary component references:
https://docs.rs/web-time/1.1.0/web_time/
https://doc.rust-lang.org/rustc/platform-support/wasm32-unknown-unknown.html
`web-time` reexports native time types outside browser WASM and uses the JS
clock on wasm32-unknown-unknown. It does not supply browser filesystem,
process, thread or distributed-clock guarantees; timestamps are wall time,
not a monotonic ordering or a transaction uniqueness proof by themselves.

Local verification totals

Full native Rust: 595 standard tests plus stdout harness; C#: 994 each in
Debug/Release; actual WASM: 315 tests. Both strict Clippy runs, formatting and
release metadata pass. Exact coverage and before/after evidence:
`dev/log/issues/138/pulls/139/version-owner-clock.txt`. CI remains pending.
