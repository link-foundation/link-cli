# Timeline

All times are UTC on 2026-11-10 unless stated otherwise. Source: `github/run-<id>.json`, `github/run-<id>-jobs.json` and the logs in `ci-logs/`.

| Time | Event |
| --- | --- |
| 2026-11-06 01:38 | Dependabot's `Graph Update: pip` run `42145172271` succeeds. It shows the `ubuntu-latest` migration notice, which comes from GitHub's own Dependabot job, not from this repository's workflows. |
| 18:39:39 | PR #139 (issue #138) merges as `199a07b`. Twelve push workflows and one Dependabot update start. |
| 18:39:54 | **C# `Release authentication preflight` fails**: trusted publishing is not configured (`NUGET_USER` unset) and the fallback `NUGET_API_KEY` verification returns HTTP 403. The message blames a non-existent "PackageVerify scope". |
| 18:39:54 | Because `lint` and `test` needed the preflight, **C# lint, all three test jobs and the build are skipped** (false negative: no code check ran on `main`). |
| 18:40:53 | `Pipeline Status (csharp)` fails: run `42877436995` ends with `failure`. |
| 18:41:12 | `Dependencies` succeeds, but with a `::warning` that new releases exist. This was a warning-only signal. |
| 18:41:28 | `Broken Link Checker` succeeds. Timed-out links would not have been rechecked: the scripts read only `## Errors per input`. |
| 18:42:50 | Rust `Lint and Format Check` starts and emits four `File has 9xx lines` warnings, for `query_processor.rs`, `permissions.rs`, `version_control/mod.rs` and `protocol_packet_tests.rs`. |
| 18:40:56–19:03:12 | `Rerun infrastructure failures` runs four times. It never watches `Crash tests`, although that workflow has a terminal gate too. |
| 18:46:34 | Rust `Test (windows-2025)` starts. Its cargo cache misses: pull request caches scoped to merge refs had evicted `main`'s caches, and the lint job, which shares the key, won the save race. |
| 18:56:25 | `WebAssembly CI` succeeds after compiling `wasm-bindgen-cli` from source for about 1.5 minutes. The compile prints a future-incompatibility warning (`buf_redux`, `multipart`). |
| 19:02:33 | Rust `Test (macos-26)` succeeds after a macOS capacity notice. |
| ~19:28 | Windows tests finish, about 41.5 minutes into the job. The `actions/cache` post step then starts compressing about 3 GB of `rust/target`. |
| 19:31:42 | **Windows test is cancelled** at its 45-minute limit while saving the cache. `Pipeline Status (rust)` fails, and run `42877437350` ends `cancelled`. |
| 19:32:04 | Issue #172 is opened with the run table. Its only comment records the final conclusions. |
| 19:34 → | PR #174 work. Evidence is collected first, then fixes are committed one per root cause (see [AUDIT.md](AUDIT.md)). |
