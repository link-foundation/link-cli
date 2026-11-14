# CI evidence for issue #111

Run IDs refer to https://github.com/link-foundation/link-cli/actions/runs/<id>.
Each section was recorded when its runs completed.

## First run of the gate (commit 4e540213)

| Workflow     | Run         | Result  | Cause                                                                                                                         |
| ------------ | ----------- | ------- | ----------------------------------------------------------------------------------------------------------------------------- |
| Workflows    | —           | failure | zizmor 1.32.0 exited 12, raising `self-repository` on every `uses: ./.github/workflows/javascript-stage.yml`. Fixed in eb278c5f (see root cause 5). |
| Dependencies | 43579423907 | failure | Issue #176 closed at 08:27 and three new releases appeared: `cargo-audit` 0.23.0, `taiki-e/install-action` v2.90.4 and `lucide-react` 1.73.0. Fixed in f3639fbe. |
| Rust CI/CD   | 43579423628 | failure | "Changelog Fragment Check": the fragment was not committed yet. The Test jobs need that check, so they were skipped. Lint, Rust coverage (Linux, golden fixture test included) and Simulate Fresh Merge passed. |
| C# CI/CD     | 43579423919 | failure | "Changeset Validation": the changeset was not committed yet. The Test jobs need that check, so they were skipped. Lint and Simulate Fresh Merge passed. |
| JavaScript, IPC, Docs, Security, Crash tests, DeepClient, Broken Link Checker | — | success | — |

In every one of these workflows, the `javascript / JavaScript stage` job ran
first, and the Rust and .NET jobs started only after it succeeded.

The Rust and C# Test jobs did not run here, so the golden fixture tests
first ran on Windows for b6eb70db (next section).

## Golden fixtures on Windows (commit b6eb70db)

This was the first run in which the Rust and C# Test jobs ran (they had
been skipped for 4e540213). Eleven workflows passed. Two failed only on `windows-2025`:

| Workflow   | Run         | Failed job          | Cause |
| ---------- | ----------- | ------------------- | ----- |
| C# CI/CD   | 43581837745 | Test (windows-2025) | `CSharpSuiteLoadsEveryGoldenFixture`: `docs/protocol/address-reservations-v1.json changed` |
| Rust CI/CD | 43581837858 | Test (windows-2025) | `rust_suite_loads_every_golden_fixture`: expected `b534de7c…`, read `4b099729…` |

`4b0997297406c9c1e337f423a6d0269d6d823c697af00851a689ebea10c995c7` is the
SHA-256 of the fixture with CRLF line endings
(`sed 's/$/\r/' docs/protocol/address-reservations-v1.json | sha256sum`).
Windows runners check out with `core.autocrlf=true`. A local clone with that
setting reproduced it: 39 of 41 fixture hashes differed before
`.gitattributes` marked `docs/protocol/**` as `-text`, and 0 differed after.
The fix is in 03e3b8f3 (root cause 8 of the case study).

## Gate demonstration (requirement 1, commit 2c57efc5)

Commit 2c57efc5 added `js/test/issue-111-gate-demo.test.mjs`, a test that
fails on purpose, and the next commit reverted it. In every workflow below,
the `javascript / JavaScript stage` job failed at its "Test JavaScript" step
(`npm run test:js`); "Check cross-language parity" was skipped. No Rust,
.NET or WASM job started:

| Workflow    | Run         | Skipped after the failed stage |
| ----------- | ----------- | ------------------------------ |
| Rust CI/CD  | 43589754262 | Detect Changes, Lint and Format Check, Test (all OSes), Rust coverage, Simulate Fresh Merge, Changelog Fragment Check, Build Package, Auto Release, Manual Release, Create Changelog PR, Verify crates.io trusted publishing |
| C# CI/CD    | 43589754138 | Detect Changes, Lint and Format Check, Test (all OSes), Simulate Fresh Merge, Changeset Validation, Build Package, Release, Instant Release, Create Changeset PR, Verify NuGet trusted publishing |
| WebAssembly | 43589754218 | Test, WASM coverage |
| Crash tests | 43589754147 | Kill and repair (Rust, C#, JS/WASM, docker:dind) |
| IPC         | 43589754411 | IPC interop |
| DeepClient  | 43589754144 | DeepClient (every implementation) |
| JS client   | 43589754120 | Conformance on Node.js, Bun and Deno; Release to npm |
| Docs        | 43589754270 | Build documentation, Deploy to GitHub Pages |
| Security    | 43589754152 | Cargo audit, CodeQL, Audit every NuGet dependency graph |
| Dependencies | 43589754132 | Dependencies are on their latest versions, Track a red check on main |
| JavaScript  | 43589754110 | (the stage itself) |

`Pipeline Status (javascript)`, `(rust)` and `(csharp)` all failed, so the
ruleset blocks the merge. Jobs that need neither toolchain still ran: the
release authentication preflights (Node.js and the registries' OIDC token
exchange), the npm and secret audits, the JS client's unit tests, `Workflows`
and the link checker.

## Protection of main (requirement 6)

Configured on 2026-11-14 at 09:39 UTC. The full state is in
[`branch-protection-after.json`](branch-protection-after.json).

| Check                                                           | Before                     | After                                                                 |
| --------------------------------------------------------------- | -------------------------- | --------------------------------------------------------------------- |
| `gh api repos/link-foundation/link-cli/branches/main/protection` | 404 "Branch not protected" | 200: force pushes and deletion forbidden                              |
| `gh api repos/link-foundation/link-cli/rulesets`                 | `[]`                       | ruleset 27257766, "main requires the JavaScript, Rust and C# stages" |
| `gh api repos/link-foundation/link-cli/rules/branches/main`      | `[]`                       | `deletion`, `non_fast_forward`, `required_status_checks`: `Pipeline Status (javascript)`, `Pipeline Status (rust)`, `Pipeline Status (csharp)`, strict |
| Pull request #180 merge state                                   | -                          | `BEHIND` right after the ruleset was created, because the ruleset requires an up-to-date branch |

Making GitHub Actions a bypass actor was refused with HTTP 422
([`actions-bypass-422.json`](actions-bypass-422.json)), so the release jobs
push with deploy key 169860720 from the `release` environment.

## Stale pull request triage (requirement 7)

Each pull request was closed with a comment that explains the triage and how
to restart the work. The branches were kept, and issues #35, #34, #33, #32,
#31, #30, #18 and #56 are still open.

| Pull request | State  | Closed at (UTC)      | Comment |
| ------------ | ------ | -------------------- | ------- |
| #36 | closed | 2026-11-14T09:42:53Z | [comment](https://github.com/link-foundation/link-cli/pull/36#issuecomment-6875389721) |
| #37 | closed | 2026-11-14T09:42:56Z | [comment](https://github.com/link-foundation/link-cli/pull/37#issuecomment-6875390459) |
| #38 | closed | 2026-11-14T09:42:59Z | [comment](https://github.com/link-foundation/link-cli/pull/38#issuecomment-6875391101) |
| #39 | closed | 2026-11-14T09:43:02Z | [comment](https://github.com/link-foundation/link-cli/pull/39#issuecomment-6875391731) |
| #40 | closed | 2026-11-14T09:43:05Z | [comment](https://github.com/link-foundation/link-cli/pull/40#issuecomment-6875392440) |
| #41 | closed | 2026-11-14T09:43:08Z | [comment](https://github.com/link-foundation/link-cli/pull/41#issuecomment-6875393018) |
| #49 | closed | 2026-11-14T09:43:11Z | [comment](https://github.com/link-foundation/link-cli/pull/49#issuecomment-6875393759) |
| #57 | closed | 2026-11-14T09:43:14Z | [comment](https://github.com/link-foundation/link-cli/pull/57#issuecomment-6875394501) |

## Two failures after the demonstration (commit 273a33e4)

| Workflow     | Run         | Failed job          | Cause |
| ------------ | ----------- | ------------------- | ----- |
| Dependencies | 43590164570 | Dependencies are on their latest versions | `links-notation` 0.31.5 was published during the run; `rust/` and `rust/wasm/` used 0.31.4. Updated in the next commit. |
| C# CI/CD     | 43590164676 | Test (windows-2025) | `VersionProcessExitTests.AbruptExitRecoversExactBranchAttribution(mode: "checkpoint", automatic: True, names: False)` hit its 30 s xUnit timeout. |

The C# timeout is not caused by this change. The test starts a child
`dotnet` process. On the three previous green Windows runs (43585591670,
43332420816, 43325870929) each of its 12 cases took 1 to 5 s. In this run the
whole Windows job was slower: its tests of 1 s or more took 1830 s, against
1596 s on 43585591670, and the `rollback` case of the same test took 19 s.
This pull request does not change the C# engine or that test; on the C# side
it only adds `GoldenFixturesTests.cs`, which reads 41 small files. The next
commit's run is the retry.
