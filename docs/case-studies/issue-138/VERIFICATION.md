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
  Commit 126168f uses a writable handle; run 40109338737 passes on Windows,
  Linux and macOS, including both previously failing tests.
- Shared journal vectors reproduced language differences: Rust accepted a leading
  plus in numbers and hex names, while .NET accepted some whitespace/trailing NUL
  forms. ASCII decimal/hex validation now precedes numeric decoding. The independent
  generator is `experiments/issue-138/generate-vectors.py`.
- A mocked short filesystem read reproduced loss of complete JS journal records.
  `FileCommitLog` now loops until each requested read completes and rejects zero
  progress; tests also inject partial/zero writes, truncation and constructor flush
  failure. The before log is preserved alongside other regression excerpts.
- A malformed subprocess startup banner leaked its owned server. A real finite
  child process reproduced the leak; address validation now occurs inside the
  startup cleanup boundary. Failure, cancellation, forced termination, protocol
  mismatch and ownership tests also exercise the process adapter.

- Staged-engine branch tests initially fail to compile because `fork` and
  `resume_branch` are absent (`engine-branch-before.log`). The implemented
  row-level three-way merge preserves independent changes, rejects overlapping
  changes, and applies parent constraints and admission rules. Flush failure
  poisons the parent; reopen plus a saved merge base discovers the durable
  result without adding a duplicate revision. Retention eviction and allocator
  recovery are covered in native tests and actual WASM conformance.
- C# release metadata CI 40109831621 rejected two changeset files. They are
  consolidated into one minor-release changeset; run 40110429498 passes. WASM
  CI 40109831578 rejected the bootstrap root `.gitkeep`; removing it makes the
  repository layout tests pass in run 40110429487.

## Local measurements (2026-10-24)

| Surface | Check | Result and limits |
| --- | --- | --- |
| Rust | `cargo llvm-cov --all-features --json` | 394 pass, 2 subprocess entry points ignored; 7,728/8,950 lines (86.35%), 1,027/1,237 functions (83.02%). Branch instrumentation was not enabled; zero reported branches means unmeasured. |
| C# | `dotnet test --collect:"XPlat Code Coverage"` | 593 pass, 0 fail; 5,396/6,084 lines (88.69%), 2,186/2,760 branches (79.20%). Uncovered engine and legacy branches remain. |
| JS unit tests | `node --test --test-concurrency=1 --experimental-test-coverage --test-coverage-include='js/client/src/**'` | 69 pass; production modules: 100% lines, 99.61% branches, 99.20% functions. Uncovered branches/functions remain; this does not measure the WASM core. |
| Actual WASM | Existing conformance runner on Node, Bun and Deno | Shared vectors, staged request ordering, isolation, history, conflicts, rollback, Unicode, custom logs, replication partition recovery and branch merge/recovery pass. This is integration evidence, not a WASM coverage measurement. |

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

The 100% coverage requirement is unmet. Routed network clusters, consensus
failover and cross-shard coordination also remain unfinished. Fixed-primary replica acknowledgements do not establish consensus.
See REQUIREMENTS.md and ENGINE-STRATEGIES.md for the implementation boundary.
