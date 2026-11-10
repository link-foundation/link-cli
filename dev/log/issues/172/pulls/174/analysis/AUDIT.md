# Issue #172 audit: false positives, false negatives, warnings and errors in CI/CD

Baseline: `main` at `199a07b`, the run table in the issue, and the flattened annotations in [`annotations-main-199a07b.txt`](annotations-main-199a07b.txt). In this audit:

- A **false negative** is a defect CI should report but doesn't. Examples: a skipped check, a warning that never fails, or a failure that is silently recovered.
- A **false positive** is a red run that does not mean the code is broken. Examples: a timeout from a slow cache save, or a runner that never started.

## Requirements

| # | Requirement (issue text) | Where it is met |
| --- | --- | --- |
| R1 | Find and fix every false positive, false negative, warning and error in CI/CD | Findings F1–F20 below |
| R2 | Compare every file of the five pipeline templates and reuse their best practices | [Template comparison](#template-comparison) |
| R3 | When a defect also exists in a template, report it there | [Upstream reports](upstream-reports/SUMMARY.md) |
| R4 | Follow `CI-CD-BEST-PRACTICES.md` (hive-mind) | [Best practices](#best-practices-cross-check) |
| R5 | Everything in one PR, nothing deferred | Only F1's credential change needs a maintainer, see [Blocked](#blocked-on-a-maintainer) |
| R6 (task) | Collect logs and data into `dev/log/issues/172/pulls/174`, reconstruct the timeline, find root causes, propose solutions, research existing components | This directory, [timeline](timeline.md) |
| R7 (task) | Add debug output where the root cause is unclear, off by default | Not needed: every root cause below was established from logs, upstream source or a reproduction |

## Findings

| # | Kind | Symptom (evidence) | Root cause | Fix (commit) | Regression test |
| --- | --- | --- | --- | --- | --- |
| F1 | error (needs a maintainer) | C# `Release authentication preflight` fails on `main` with "Fallback verification HTTP 403 … may require PackageVerify scope" (run `42877436995`) | `NUGET_USER` is unset, so trusted publishing is unavailable, and `NUGET_API_KEY` authenticates but lacks push scope. In NuGetGallery, `create-verification-key` needs `[ApiScopeRequired(PackagePush, PackagePushVersion)]` (`research/nuget-gallery/`), so a 403 means "no push scope". The old message pointed at a scope that does not exist on that route. | The message now says the key authenticates but cannot push (`9b462e60`). The credential itself must be changed by a maintainer. | `.github/scripts/nuget-auth.test.mjs` |
| F2 | false negative | In that run, C# `lint`, `test` (3 OS) and `build` were **skipped**, so no code check ran on `main` | The code-check jobs listed `release-preflight` in `needs:` and required its success, and the same applied in `rust.yml` and to `unit`/`conformance` in `js-client.yml`. A registry credential problem hid the code's status. | Only publishing jobs wait on the preflight. `Pipeline Status` still needs it, so `main` still fails with the explicit message (`2568dead`). | `workflow-policy.test.mjs`: "a failed release preflight blocks publishing but never the code checks" (fails before, passes after) |
| F3 | false positive | Rust `Test (windows-2025)` is cancelled at 45 min after all tests passed in ~41.5 min (run `42877437350`) | The cargo cache always missed. PR caches, scoped to `refs/pull/N/merge` and unrestorable elsewhere, evicted `main`'s caches under the 10 GB quota (`github/actions-caches-2026-11-10.tsv`). `test` shared its key with `lint`, whose clippy-only cache won the save race. The ~3 GB post-step save then counted against `timeout-minutes`. | `actions/cache/restore` runs everywhere. `actions/cache/save` runs only on `main` pushes, on a miss. `test` has its own key and saves before the long test run (`1f286358`). | `workflow-policy.test.mjs`: "caches are restored everywhere but saved only by main pushes on a miss", "the Rust test cache is saved before the long test run" |
| F4 | warning | Four `File has 9xx lines (approaching limit of 1000)` warnings on every Rust run | The files had grown into the 900–1000 warning band | Split along cohesive boundaries (`283d6c4e`) | Existing `rust/scripts/check-file-size.rs` gate; it now emits no warning |
| F5 | false negative | The C# size check was silent until the hard 1000-line failure: `Program.cs` had 986 lines and a test file 930 | `csharp/scripts/check-file-size.mjs` had no warning band; the Rust checker did | Warning band added, `--lang csharp` scoping, two files split (`db30ae27`) | `csharp/scripts/check-file-size.test.mjs` |
| F6 | false negative | The JS size check (ESLint `max-lines`) was silent before 1000 lines | No warning band for JavaScript | Same band and annotation format (`2605260d`, shared helper `be94ffa1`) | `csharp/scripts/check-file-size.test.mjs`: "the JavaScript quality check warns in the same band" |
| F7 | false negative | A lychee report with a recovered 503 plus a **timed-out** link was declared `all_recovered=true` | lychee writes timeouts under `## Timeouts per input` (`research/lychee/markdown.rs`). The scripts read only `## Errors per input`. | Both sections are read and validated against their own counts (`90d9a675`) | `link-failures.test.mjs`: "timed-out links are failures that must be rechecked" |
| F8 | false negative | Any non-zero lychee exit was treated as "links failed", so the recovery path could pass a run in which lychee itself crashed (exit 1) or was misconfigured (exit 3) | lychee exits with 2 for link failures, 1 for runtime errors and 3 for config errors | Only exit code 2 can be recovered (`bd40ec63`) | `link-failures.test.mjs` (exit codes 1 and 3 throw) |
| F9 | false negative | Link rot went undetected while no Markdown changed | `links.yml` had no `schedule:` | Weekly schedule (`bd40ec63`) | n/a (configuration) |
| F10 | false negative / false positive | `* [404]` followed by a URL on the next line was accepted, and a truncated `* [ERROR` swallowed the next record | The record patterns used `\s`, which matches a newline | Horizontal whitespace only, as in the csharp template (`a2592ddf`) | `link-failures.test.mjs`: "a lychee record never borrows text from the next line" (fails before) |
| F11 | false negative (security) | Secrets in committed `.tar.gz`/`.json.gz` evidence were never scanned | `check-secrets.mjs` decompressed only `.log.gz`. secretlint received other archives as binary. | Every `.gz` is decompressed under a 128 MiB cap. Tarballs are unpacked and their files scanned. Symlinks and option-like names are handled (`cd975442`). 499 files inside tarballs are now scanned. | `check-secrets.test.mjs` (a planted token in a `.tar.gz` failed to be detected before) |
| F12 | false negative (security) | CodeQL SARIF text was printed raw. A newline followed by `::` in a message could forge a workflow command. Only the first malformed diagnostic was reported, and an empty `note` was accepted. | Unquoted untrusted output; throw-on-first | All SARIF text is JSON-quoted and all problems are collected (`a4d61615`) | `check-codeql-results.test.mjs` |
| F13 | false positive | A `Crash tests` job that never got a macOS/Windows runner stayed red, with no retry | `rerun-infrastructure-failures.yml` did not list `Crash tests` | Added (`26133a71`) | `pipeline-status.test.mjs`: the rerun list must equal the set of workflows with a `pipeline-status` gate |
| F14 | warning | `Dependencies` warned that new releases exist: System.CommandLine, Microsoft.Data.Sqlite, Microsoft.NET.Test.Sdk, cargo-audit, rustls-pki-types | Releases after the last update | Updated. The C# build has 0 warnings and all 3439 tests pass (`a2de1e79`). cargo-audit is now held at 0.22.2 (see F19). | The Dependencies workflow itself |
| F15 | warning | `cargo install wasm-bindgen-cli` printed a future-incompatibility warning (`buf_redux`, `multipart` via `rouille`) and compiled for ~1.5 min | Upstream wasm-bindgen/wasm-bindgen#3356 (already reported upstream) | Prebuilt, pinned binaries via `taiki-e/install-action` (SHA-pinned, already used in `security.yml`) (`5b729db4`) | actionlint and zizmor in `Workflows` |
| F16 | notice (not actionable here) | "`ubuntu-latest` will migrate to Ubuntu 26" | Comes only from GitHub's own Dependabot jobs (`42145172271`, `42877436505`). Every workflow in this repository already pins `ubuntu-24.04`. | None needed. `workflow-policy.test.mjs` "hosted runners use pinned OS labels" already enforces the pin. | existing |
| F17 | notice (infrastructure) | "jobs targeting macOS arm64 runners may experience longer queue times" | GitHub capacity | Covered by the infrastructure rerun workflow (F13) | existing |
| F18 | false negative | Found while verifying this PR. The Rust CI/CD Pipeline run for head `067cb2c3` (`42898138563`) succeeded, but Rust lint and test were **skipped** (`rs-changed=false`) although the PR changes many Rust files | `rust/scripts/detect-code-changes.rs` diffed only the last commit of the PR head (`HEAD^2^..HEAD^2`). That commit changed only a Python helper, and the runs for the earlier Rust commits had been superseded | `pull_request` merge commits now diff `HEAD^1..HEAD`, the whole PR against its base. This matches the C# detector, which already compares base...head (`38ef331a`). | `rust/scripts/detect-code-changes.rs` test `main_merge_and_pr_both_check_every_change_of_the_branch`, run by `rust/scripts/test-scripts.sh` (fails on the old code with `["README.md"]`) |
| F19 | warning | Found while verifying this PR. `Security` run `42898137431` warned that `cargo-audit@0.23.0` had no verified manifest in `taiki-e/install-action` and fell back to an unverified `cargo-binstall` download | F14 bumped cargo-audit to a crates.io release newer than the pinned install-action's manifest | Pinned `cargo-audit@0.22.2`, the newest version the action verifies, with a blocker comment that points at tracking issue #176. `check-latest-dependencies.mjs` reports it as *held*, not outdated, while #176 is open, and flags it again once #176 is closed (`b373d4bb`). | Dependencies workflow (held-dependency mechanism) and the `Security` annotations |
| F20 | warning (needs an organisation admin) | Found while verifying this PR. The Codecov PR comment starts with ":warning: Please install the codecov app to ensure uploads and comments are reliably processed" | The comment is posted by the shared `codecov-commenter` bot account, which Codecov uses when the Codecov GitHub App is not installed for the organisation. The upload itself works through OIDC (`csharp.yml`, `use_oidc: true`), and the report says that all modified lines are covered. | None possible in code: installing https://github.com/apps/codecov needs an organisation admin, and the workflow `GITHUB_TOKEN` cannot read app installations, so CI cannot detect it or gate on it. Listed under [Blocked on a maintainer](#blocked-on-a-maintainer). | n/a (not a check result) |

Formatting fallout from the new tests was fixed in `6fbe4167`: the `Workflows` lint job rejected their prettier formatting.

## Blocked on a maintainer

**NuGet publishing credentials (F1).** CI cannot fix this, and no secret value was inspected. One of the following is needed:

1. Configure trusted publishing: set the repository variable `NUGET_USER` to the nuget.org account that owns `clink`, and add a nuget.org trusted-publisher policy for this repository and workflow `csharp.yml`.
2. Or renew `NUGET_API_KEY` with the **Push** scope (new packages and package versions) for `clink`, created by the package owner.

The PR is not blocked by this: the preflight is advisory on pull requests. `main` keeps failing with that explicit remediation message until a maintainer acts. Since F2, lint, tests and build still report the code's own status in the meantime.

**Codecov GitHub App (F20).** An organisation admin should install the [Codecov GitHub App](https://github.com/apps/codecov) for `link-foundation`, with access to this repository. Until then, coverage still uploads and is reported, but the PR comment carries Codecov's "Please install the codecov app" warning. This is not a check, so there is nothing for `main` to fail on. `.github/scripts/coverage.mjs` already makes uploads mandatory once Codecov reports the repository as activated.

## Template comparison

The five templates were compared at these commits (see `templates/*-source.json`):

| Template | Commit |
| --- | --- |
| rust | `88fd935f` |
| csharp | `1f3b92f7` |
| js | `34415b00` |
| java | `0d3a4e3e` |
| python | `f28c9d83` |

The trees in `templates/*-tree.txt` were compared file by file with this repository's `.github/` and language `scripts/`. Issue #165 (PR #166) had already done a full inventory of the rust, csharp and js templates. Only their changes since then are in `templates/*-since-pr166.diff`.

Changes in those diffs that were adopted:

- Horizontal-whitespace lychee record parsing, from the csharp template (F10).
- Timeout-section parsing (F7). The js template adopted it independently in its `lychee-report.mjs`; this PR went further and also validates the exit code (F8).
- CodeQL invocation-notification checking. This repository already had it.

Changes considered but not adopted:

- **js/python replaced `lycheeverse/lychee-action@v2` with `scripts/install-lychee.sh` (pinned `v0.24.2`, download retries).** Here the action's default is already `v0.24.2`, according to the run log. The report format is validated loudly (`validateLycheeReport` throws on an unknown structure). A failed download fails the step, so it is not silent.
- **Exact `# vX.Y.Z` comments on SHA pins.** This repository's SHA pins already carry exact versions. Trusted namespaces stay on major tags by the deliberate `.github/zizmor.yml` policy.
- **Rust template: `cancel-in-progress: true` everywhere.** Already true for every read-only check here; writer groups intentionally never cancel.

Defects that exist in the templates themselves are listed in [upstream-reports/SUMMARY.md](upstream-reports/SUMMARY.md). One issue was filed per template:

- rust: link-foundation/rust-ai-driven-development-pipeline-template#236. F18 was already open there as #235, so the link-cli evidence and fix were [added as a comment](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/issues/235#issuecomment-6779540622).
- csharp: link-foundation/csharp-ai-driven-development-pipeline-template#100
- js: link-foundation/js-ai-driven-development-pipeline-template#297
- java: link-foundation/java-ai-driven-development-pipeline-template#22
- python: link-foundation/python-ai-driven-development-pipeline-template#145

The `wasm-bindgen-cli` warning (F15) was already reported upstream as wasm-bindgen/wasm-bindgen#3356, so no new report was filed.

## Best-practices cross-check

Checked against `research/CI-CD-BEST-PRACTICES.md`:

- **Fail fast on a real cause, never on infrastructure.** F3 and F13 remove false positives. F2, F7, F8, F10 and F18 remove false negatives.
- **Make warnings visible and actionable.** F4–F6 and F14 bring the warnings of `199a07b` to zero, with tests. F15 removes the compiler warning.
- **Pin, cache and keep secrets safe.** F3 splits cache restore from save. F11 and F12 harden scanning. F15 uses verified prebuilt binaries.
- **Human actions are explicit.** F1 makes the message precise; the PR passes and `main` fails with remediation.
- **Regression tests for each rule.** Every fix above has a test, except configuration-only F9.

## Existing components used

| Component | Used for |
| --- | --- |
| `actions/cache/restore` and `actions/cache/save` | F3 |
| `taiki-e/install-action` | Prebuilt `cargo-llvm-cov` and `wasm-bindgen` (F15) |
| lychee's documented exit codes and report sections | F7, F8 |
| secretlint | F11; reused rather than replaced |
| actionlint and zizmor (already in `Workflows`) | Validating every workflow change |

## Verification

These were run locally on the final tree:

- `node --test .github/scripts/*.test.mjs`: 264 pass, 0 fail.
- `node --test csharp/scripts/*.test.mjs`: 47 pass, 0 fail.
- `rust-script --test rust/scripts/detect-code-changes.rs`: 5 pass, 0 fail.
- `npm --prefix js run lint`: 299 files, 0 findings, 0 files near the line limit.
- actionlint: exit 0.
- C#: `dotnet build` (0 warnings) and `dotnet test`: 3439 pass.

Each regression test above was also run against the pre-fix code and failed there. CI results for the final head are on PR #174.
