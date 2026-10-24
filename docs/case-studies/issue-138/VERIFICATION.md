# Verification record

This is evidence for [PR 139](https://github.com/link-foundation/link-cli/pull/139),
not a claim that the open-ended issue is complete. Commands run with one Cargo
build job and reduced debug information under a 2.92 GiB container limit.

## Reproductions and fixes

- `acid_readiness_tests.rs` and `AcidReadinessTests.cs` reproduce acknowledged
  commits lost without an explicit save, rollback persistence, checkpoint faults,
  invalid recovery markers and malformed transition records. Journal flush now
  precedes acknowledgement; data checkpoint flush precedes applied markers.
- C# Windows CI 40107157910 failed because checkpoint flush reopened an exclusively
  mapped file. The owned mapping/stream fix passes all three platforms in run
  40107962518 (562 tests per platform). Before/after excerpts are in
  `dev/log/issues/138/pulls/139/`.
- Rust Windows CI 40107962488 failed in two new durability tests with OS error 5
  (`Access is denied`). `File::open` supplied a read-only handle to `sync_all`.
  Commit 126168f uses a writable handle; new Windows CI must verify it.
- Shared journal vectors reproduced language differences: Rust accepted a leading
  plus in numbers and hex names, while .NET accepted some whitespace/trailing NUL
  forms. ASCII decimal/hex validation now precedes numeric decoding. The independent
  generator is `experiments/issue-138/generate-vectors.py`.
- A mocked short filesystem read reproduced loss of complete JS journal records.
  `FileCommitLog` now loops until each requested read completes and rejects zero
  progress; tests also inject partial/zero writes, truncation and constructor flush
  failure. The before log is preserved alongside other regression excerpts.

## Local measurements (2026-10-24)

| Surface | Check | Result and limits |
| --- | --- | --- |
| Rust | `cargo llvm-cov --all-features --json` | Full suite passes; 7,483/8,834 lines (84.71%), 992/1,230 functions (80.65%). Branch instrumentation was not enabled; zero reported branches means unmeasured. Native named-session tests added afterward require a new report. |
| C# | `dotnet test --collect:"XPlat Code Coverage"` | 588 pass, 0 fail; 5,339/6,027 lines (88.58%), 2,143/2,714 branches (78.96%). New engine classes have 97.67–100% line coverage; uncovered engine and legacy branches remain. |
| JS unit tests | `node --test --experimental-test-coverage` | 53 pass; loaded source files 96.06% lines, 91.42% branches, 87.91% functions. The process adapter is not loaded by this measurement, so this is not whole-package coverage. |
| Actual WASM | Existing conformance runner on Node, Bun and Deno | Shared vectors, staged request ordering, isolation, history, conflicts, rollback, Unicode, custom logs and replication partition recovery pass. This is integration evidence, not a WASM coverage measurement. |

The full Node conformance run against the actual Rust server, C# server and WASM
reports 32 passed, 0 failed, 4 skipped (the skips are process-only scenarios for
in-process WASM). Three additional native Rust session API tests pass. Rust and
WASM Clippy deny warnings; .NET formatting verification and TypeScript checking
pass.

Coverage reports are saved under `experiments/issue-138/` locally and the Rust
workflow now preserves a coverage artifact. Retention, named API, recovery,
concurrent schedules and process-exit probes use finite inputs. Process exit
demonstrates recovery without destructors; it does not simulate a power cut or
prove the filesystem/controller's persistence guarantees.

## Acceptance gaps

The 100% coverage requirement is unmet. Staged-engine branches/merges, routed
network clusters, consensus failover and cross-shard coordination also remain
unfinished. Fixed-primary replica acknowledgements do not establish consensus.
See REQUIREMENTS.md and ENGINE-STRATEGIES.md for the implementation boundary.
