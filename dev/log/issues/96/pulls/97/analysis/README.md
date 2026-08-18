# Issue #96 — Deep analysis

> "Check for all false positives, false negatives, warnings and errors in CI/CD and fix them all."
> — <https://github.com/link-foundation/link-cli/issues/96>

This folder contains the raw evidence (`../ci-logs/`, `../templates/`, `../workflows/`,
`../best-practices/`) and the analysis below.

## 1. Requirements extracted from the issue

| ID | Requirement (verbatim intent) |
|----|-------------------------------|
| R1 | Find and fix all **false positives** in CI/CD |
| R2 | Find and fix all **false negatives** in CI/CD |
| R3 | Find and fix all **warnings** in CI/CD |
| R4 | Find and fix all **errors** in CI/CD |
| R5 | Compare **the full file tree** of CI/CD scripts against the three AI-driven-development pipeline templates (`rust-`, `csharp-`, `js-ai-driven-development-pipeline-template`) and reuse all best practices |
| R6 | If the same defect exists in a template, **report an issue in that template repo** too |
| R7 | Follow the CI/CD best practices document from `link-assistant/hive-mind` (`docs/CI-CD-BEST-PRACTICES.md`) |
| R8 | Plan and execute **everything in this single pull request** until every requirement is fully addressed |

## 2. Timeline of events

| When | What | Evidence |
|------|------|----------|
| earlier | Windows C# tests started failing with `System.IO.IOException` on temp-file deletion | run 25760911270 (logs expired, HTTP 410) |
| — | Instead of fixing the tests, `continue-on-error: ${{ matrix.os == 'windows-latest' }}` was added to the `Run tests` step of `csharp.yml` with the comment *"Windows has pre-existing file locking issues with some tests"* | `.github/workflows/csharp.yml:186-187` |
| 2026-05-20 | Run 26176444325: the Windows job **reports success** while its log contains `Test Run Failed. Total tests: 212, Passed: 98, Failed: 114` and `Build FAILED` | `../ci-logs/run-26176444325.log` |
| same run | macOS job flakes on `SwapSourceAndTargetForAllLinksUsingVariablesTest` with `System.TimeoutException : Test exceeded 1 seconds timeout` | same log |
| same run | 2 distinct `CS1570` warnings emitted 8× across the matrix; a Node.js-20 deprecation warning is emitted by `actions/github-script` pulled in transitively by `codecov/codecov-action@v5` | same log |
| 2026-08 | Issue #96 filed asking for all of the above to be found and fixed | `../issue/issue-96.json` |

## 3. Findings, root causes and fixes

### R2 — false negatives (a real failure reported as success)

**F1. `continue-on-error` masks 114 failing Windows tests.**
`.github/workflows/csharp.yml:187` marks the whole `dotnet test` step as non-fatal on
`windows-latest`. GitHub Actions then reports the job green. This is the single most
severe defect: for months the Windows leg of the matrix has been decorative.
*Root cause:* a symptom was suppressed rather than diagnosed.
*Fix:* remove the suppression — but only after F2/F3 below are actually fixed,
otherwise CI just turns red.

**F17. `csharp/scripts/check-file-size.mjs` is dead code.** The script exists and
enforces a 1000-line limit, but no workflow ever invokes it, so the limit is not
enforced for C# at all (the Rust equivalent *is* wired up at `rust.yml:138`).
*Fix:* wire it into the C# lint job.

**F8/F9. C# warnings can never fail CI.** There is no `csharp/Directory.Build.props`
(all three templates ship one with `TreatWarningsAsErrors`, `EnableNETAnalyzers`,
`AnalysisLevel=latest-all`), and the lint job never runs
`dotnet format --verify-no-changes` (present at `csharp/release.yml:199` in the template).
*Fix:* add both.

### R4 — errors

**F2. 226 `System.IO.IOException` → 73 failing Windows tests.**
Every failure is `System.IO.FileSystem.DeleteFile`. The test helpers construct
`NamedTypesDecorator<uint>` / `NamedLinksDecorator<uint>` / `SimpleLinksDecorator<uint>`,
which own `FileMappedResizableDirectMemory` handles, and then `File.Delete` the backing
files **without disposing the decorator first**.
*Root cause:* POSIX allows unlinking a file that is still open; Windows uses mandatory
locking and refuses. The tests are therefore not a Windows bug — they are a resource-leak
bug that only Windows is strict enough to surface.
*Fix:* dispose every `IDisposable` decorator before deleting its files, in **all**
affected helpers.

**F3. 2 `Assert.Equal` failures on Windows.**
`NamedLinksDecoratorTests.MakeNamesDatabaseFilename_CorrectlyGeneratesFilename`
hard-codes `/`-separated expectations (`"/tmp/test.names.links"`), while the production
code builds the path with `Path.Combine`, which emits `\` on Windows.
*Root cause:* platform-dependent expectation in a platform-independent test.
*Fix:* build the expectation with the same platform-neutral primitives.

**F4. macOS flake: `Test exceeded 1 seconds timeout`.**
`RunTestWithLinks` wraps every test body in a `CancellationTokenSource(TimeSpan.FromSeconds(1))`.
*Root cause:* a hard-coded 1-second wall-clock budget is far too tight for a shared,
loaded CI runner; it measures runner load, not correctness.
*Fix:* raise the budget and make it overridable via an environment variable.

### R3 — warnings

**F5. `CS1570` ×2** at `ChangesSimplifier.cs:187` — `Link<uint>` written literally inside an
XML doc comment, so `<uint>` is parsed as a (never-closed) XML tag.
*Fix:* use the documentation-comment escape `Link{uint}`; sweep the whole codebase for
the same pattern.

**F6. Node.js 20 deprecation** for `actions/github-script@60a0d83…`. Not referenced by any
workflow in this repo — it comes transitively from `codecov/codecov-action@v5`. Nothing to
fix locally; reportable upstream (R6).

**F7. File-size warning:** `rust/src/query_processor.rs` is 994 lines, over the 900-line
warn threshold of `rust/scripts/check-file-size.rs`.

### R1 — false positives

A "false positive" here is CI failing (or warning) for something that is not a real defect
in the change under test. The 1-second test timeout (F4) is exactly that: a red build
caused by runner load. Fixing F4 removes it. `fail_ci_if_error: false` on the Codecov step
is retained deliberately — a Codecov outage must not fail a code change.

### R5/R7 — template + best-practice gaps

| ID | Gap | Templates that have it |
|----|-----|------------------------|
| F10 | no `security.yml` (CodeQL + `dependency-review-action`) | rust, csharp, js |
| F11 | no `links.yml` (lychee broken-link check) | rust, csharp, js |
| F12 | zero `timeout-minutes` in `csharp.yml` and `rust.yml` | all |
| F13 | no workflow-level least-privilege `permissions:` in `csharp.yml`/`rust.yml` | all |
| F14 | workflow-level `cancel-in-progress: true` on workflows that contain **release/write** jobs — an in-flight NuGet/crates.io publish or tag push can be cancelled | all (reader/writer split) |
| F15 | `always()` in job `if:` instead of `!cancelled()` — keeps running after the user cancels | best practice #12 |
| F16 | no "simulate fresh merge with the base branch" validation | best practice #7 |
| F18 | JS is ~11% of the repo but has no lint job | js template |

## 4. Existing components / libraries surveyed

* **`dotnet format`** (ships with the SDK) — whitespace/style verification; no third-party
  linter needed for C#.
* **Roslyn analyzers** via `EnableNETAnalyzers` + `AnalysisLevel=latest-all` — built in,
  preferred over adding StyleCop/SonarAnalyzer packages.
* **`github/codeql-action`** — first-party SAST, already the templates' choice.
* **`actions/dependency-review-action`** — first-party dependency-diff scanning.
* **`lycheeverse/lychee-action`** — the de-facto broken-link checker; used by the templates.
* **`Microsoft.NET.Test.Sdk` / xUnit `IAsyncLifetime`** — the idiomatic way to scope
  test resources; used here via `IDisposable` fixtures instead of ad-hoc `try/finally`.
* No third-party library solves the Windows file-locking problem: the correct fix is to
  dispose the handle, which is a plain resource-lifetime bug.

## 5. Verbose / debug mode

`RunTestWithLinks` already accepts `enableTracing` (default `false`). The timeout is now
also controllable without a code change via `LINK_CLI_TEST_TIMEOUT_SECONDS`, so a future
iteration can widen or narrow the budget from CI alone. Both default to off/generous.
