# Issue #111: a JavaScript-first CI gate, cross-language parity and repository governance

[Issue](https://github.com/link-foundation/link-cli/issues/111) ·
[Pull request](https://github.com/link-foundation/link-cli/pull/180)

The issue audits link-cli against the standard in link-assistant/router#759
(requirements 3, 4, 6 and 8), the shared pipeline templates, and
hive-mind's `docs/CI-CD-BEST-PRACTICES.md`. Its finding: the three stacks
were built in parallel with no gate between them, no check kept them at
parity, `main` was unprotected, and the open pull requests were stale or red.

## Data collected

The raw data is in [`data/`](data/). It was collected on 2026-11-14 with
`gh`, and it contains no account data beyond public GitHub metadata.

| File                                                       | What it holds                                                                    |
| ---------------------------------------------------------- | -------------------------------------------------------------------------------- |
| [`issue-111.json`](data/issue-111.json)                    | The issue body. It has no comments ([`issue-111-comments.json`](data/issue-111-comments.json) is `[]`). |
| [`pr-180-comments.json`](data/pr-180-comments.json)        | Pull request conversation comments. It was empty when collected.                 |
| [`branch-protection-before.json`](data/branch-protection-before.json) | `GET /branches/main/protection` before the change: 404 "Branch not protected". |
| [`open-prs-before.json`](data/open-prs-before.json)        | The open pull requests before triage: number, last update, linked issues, check results. |
| [`ci-evidence.md`](data/ci-evidence.md)                    | The CI runs this pull request relied on, including the failures it fixed and the gate demonstration. |

## Timeline

| When (UTC)       | Event                                                                                                                       |
| ---------------- | --------------------------------------------------------------------------------------------------------------------------- |
| 2025-09-10       | PRs #36-#41 and #49 open. Each duplicates an open issue (#35-#30, #18). None ever gets a CI result.                        |
| 2025-09-15       | PR #57 opens, duplicating issue #56. All eight stay open with no activity for 14 months.                                     |
| 2026-10-08/09    | The scheduled `Dependencies` run fails on `main` two days running. Nothing reports it.                                       |
| 2026-10-09 03:49 | On a Dependabot branch, `C# CI/CD Pipeline`, `Dependencies`, `Docs` and `Security` all start in the same second: no stage order. |
| 2026-10-10       | Issue #111 is filed.                                                                                                         |
| 2026-10-12       | PR #109 (the NuGet group) is closed without merging. Later Dependabot pull requests replace it.                             |
| 2026-10-24       | `workflows.yml` (actionlint with shellcheck, zizmor, policy tests), `.github/zizmor.yml` and the CodeQL config land on `main` (1b8551dd, fb1e88e7, cfd03518). |
| 2026-11-06       | ESLint, Prettier and the `lint` script for `js/` land on `main` (fd17aa4e).                                                 |
| 2026-11-10/13    | Scheduled and push runs of `Dependencies` on `main` are green.                                                               |
| 2026-11-14 08:27 | Issue #176 closes. It held back `cargo-audit` 0.22.2 until `taiki-e/install-action` shipped a 0.23.0 manifest (v2.90.4). |
| 2026-11-14       | PR #180 adds the parity manifest, the checker and golden fixtures (50fc778d), stack path detection (6baffe6c), the JavaScript stage gate (4e540213), the stale workflow and tracking issue (62802d79), the zizmor reconciliation (eb278c5f) and the dependency bumps (f3639fbe). |
| 2026-11-14       | CI for 4e540213 fails `Workflows`, because zizmor 1.32's `self-repository` audit flags every `uses: ./…/javascript-stage.yml`. It also fails `Dependencies`, because #176 closed and three upstream releases appeared that morning. Both are fixed before the next push. |

## Requirements and how each is met

| #  | Requirement                                                                                  | Solution in PR #180                                                                                                                                                                                                                                                                                                                                                       | Verification                                                                                                                         |
| -- | -------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| 1  | Rust and C# jobs run only after the JavaScript stage succeeds for the same commit.            | The reusable `javascript-stage.yml` runs lint, `test:js` and parity. `javascript.yml` is its entry workflow. Every workflow that runs Rust or .NET calls it as job `javascript`, and each Rust or .NET job lists it in `needs:`. `needs:` only starts a job after its dependencies succeeded, in the same run and for the same commit. Jobs with `!cancelled()` also check `needs.javascript.result == 'success'`, so a cancelled or skipped stage cannot start them. | `workflow-policy.test.mjs` (gate rule). The deliberate failing-test run is recorded in [`data/ci-evidence.md`](data/ci-evidence.md). |
| 2  | Move `npm run test:js` out of `wasm.yml` into a stage that needs no Rust.                     | `test:js` now runs only in `javascript-stage.yml`, which installs only Node.js. `wasm.yml` keeps `test:wasm` and the WASM build behind the gate.                                                                                                                                                                                                                           | The policy test checks that the stage installs no Rust or .NET tooling and that `wasm.yml` does not run `test:js`.                    |
| 3  | A parity manifest and a checker that fails on a one-sided feature change, run everywhere with no path filter. | `parity/features.json` covers 30 features in 3 languages. `parity/check-parity.mjs` checks schema, coverage, fingerprints, one-sided changes and golden fixtures; see [PARITY.md](../../PARITY.md). It runs in the JavaScript stage that every stack workflow calls. `rust.yml` and `csharp.yml` lost their `paths:` filters; `stack-paths.mjs` skips unaffected build jobs inside the run instead. | `parity/check-parity.test.mjs`, and `ci-governance.test.mjs` ("required stage checks report on every pull request").                |
| 4  | Shared golden fixtures that both suites must load, and a CI job that fails otherwise.         | `parity/golden-fixtures.json` lists 41 `docs/protocol` fixtures with SHA-256. `rust/tests/golden_fixtures_tests.rs` and `GoldenFixturesTests.cs` load and hash all of them and write a receipt. `--receipt` in `rust.yml` and `csharp.yml` fails if a receipt is missing or incomplete.                                                                                     | The receipt steps in CI. `ci-governance.test.mjs` checks that both workflows keep them.                                              |
| 5  | Document what the `js/` workbench implements and what it delegates.                           | [JS-WORKBENCH.md](../../JS-WORKBENCH.md). The per-feature truth is the `javascript` cell in `parity/features.json`.                                                                                                                                                                                                                                                      | The parity checker keeps that cell honest. Review of the page.                                                                       |
| 6  | Branch protection or a ruleset on `main` that requires the three stage checks; the protection API returns 200. | Classic branch protection on `main` requires `Pipeline Status (javascript)`, `Pipeline Status (rust)` and `Pipeline Status (csharp)`, which must be up to date with `main`. It applies to administrators and forbids force pushes and deletion.                                                                                                                              | `gh api repos/link-foundation/link-cli/branches/main/protection` returns 200. See [`data/ci-evidence.md`](data/ci-evidence.md).      |
| 7  | Triage PRs #36-#41, #49 and #57, and flag or close PRs idle for more than 60 days.           | The eight PRs are closed with a comment. Their branches are kept and their issues stay open, so the work can be restarted from a fresh branch. `stale.yml` (actions/stale v11) labels a PR `stale` after 60 idle days and closes it 14 days later; `keep-open` exempts a PR. Issues are never marked stale.                                                                     | `ci-governance.test.mjs` checks the stale configuration. The PR states are recorded in [`data/ci-evidence.md`](data/ci-evidence.md). |
| 8  | Fix or close PR #109, and make a red scheduled `Dependencies` on `main` open or update a tracking issue. | #109 was already closed on 2026-10-12. The new `tracking-issue` job in `dependencies.yml` runs on `main` after `latest`. It opens "Dependencies check is failing on main", comments on it while the check stays red, and closes it after the next green run. The held-back `cargo-audit` pin became due during this work and was updated.                                       | `dependency-tracking-issue.test.mjs`, and a dry run against the live repository (see [troubleshooting](../../ci-cd/troubleshooting.md#dependencies)). |
| 9  | `workflows.yml`, `.github/zizmor.yml`, the CodeQL config, and ESLint/Prettier for `js/`, with their findings fixed. | All of these already existed on `main` (see the timeline). This PR fixed zizmor's new `self-repository` finding (see root cause 5), and documented every linter and its local command in [troubleshooting](../../ci-cd/troubleshooting.md#workflow-lint-actionlint-and-zizmor). | `Workflows` and the JavaScript stage are green. Locally, actionlint 1.7.12 and zizmor 1.32.0 report nothing.                          |
| 10 | `CONTRIBUTING.md`, `AGENTS.md`/`CLAUDE.md` and `.github/CODEOWNERS` with the single-pusher rule. | [CONTRIBUTING.md](../../../CONTRIBUTING.md) and [AGENTS.md](../../../AGENTS.md) state the rule and give the command that confirms no run is in progress. `CLAUDE.md` imports `AGENTS.md`. `.github/CODEOWNERS` requests the maintainer's review.                                                                                                                         | Review. CODEOWNERS is valid when GitHub shows no error banner on the file.                                                            |
| 11 | A CI check that fails if a workflow can start Rust or C# without the JavaScript gate.         | `gateViolations()` in `workflow-policy.test.mjs` finds every job that installs or runs `cargo`, `rustup`, `wasm-pack`, `dotnet` or `setup-dotnet`. That includes jobs reached through nested reusable workflows. It fails unless the job transitively `needs` a job that calls `javascript-stage.yml`, and unless every `always()`/`!cancelled()` condition also requires that job's success. | Synthetic positive and negative cases, plus a check of every real workflow, in `Workflow Policy Tests`.                              |

## Root causes

1. **One workflow per stack, each with its own trigger.** GitHub Actions has no
   ordering between workflows that the same event triggers. Before this
   change, the only ordering mechanism, `needs:`, works inside one workflow
   run. Each stack's workflow was written separately, so nothing linked them.
2. **The JavaScript unit tests lived in the WASM workflow.** The workbench
   needs the WASM build to run, so its tests were placed where that build
   happens. That made the cheapest checks depend on the most expensive
   toolchain.
3. **Parity was prose.** `docs/REQUIREMENTS.md` described parity, but no
   machine-readable record existed that a script could compare with the code.
   The path filters also meant that a C#-only change never ran the Rust
   workflow at all.
4. **Governance was never set up.** The repository had no protection,
   rulesets, CODEOWNERS or stale policy. Red and idle pull requests stayed
   open, and a failing scheduled run produced only an email to whoever had
   notifications on.
5. **zizmor 1.32 versus actionlint 1.7.12.** The gate adds
   `uses: ./.github/workflows/javascript-stage.yml` to ten workflows. zizmor
   1.32's new `self-repository` audit asks for the `$/` syntax that GitHub
   introduced on 2026-07-30. actionlint does not parse that syntax yet
   (rhysd/actionlint#711, open), so following zizmor would break the other
   linter.
6. **A held-back pin that expired during the work.** Issue #176 was closed on
   2026-11-14 at 08:27, after `taiki-e/install-action` v2.90.4 shipped the
   `cargo-audit` 0.23.0 manifest. From then on, the Dependencies check
   correctly reported the pin as due.

## Solutions considered

| Problem                    | Options                                                                                                                                                       | Chosen, and why                                                                                                                                                                                                                                                                                                                                      |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Stage gate                 | (a) `workflow_run` chained on the JavaScript workflow; (b) one reusable JavaScript workflow called from each stack workflow as a `needs:` dependency.          | **(b).** `workflow_run` runs on the default branch's copy of the workflow, cannot report its checks on the pull request's head commit without extra API calls, and needs `head_sha` matching. With `needs:`, the gate is in the same run and on the same commit, the job is shown as skipped when the stage fails, and the policy test can check it statically. The cost is that each workflow runs the stage again, which takes about a minute of Node.js time. |
| Path filters               | Keep `paths:` and also run parity elsewhere, or remove `paths:` and decide inside the run.                                                                    | **Remove.** A required check from a workflow that a path filter skipped stays "Expected" forever and blocks the merge (GitHub docs: "Handling skipped but required checks"). `stack-paths.mjs` gives the same savings inside the run.                                                                                                                       |
| Parity manifest            | Hand-written checklists; a generated feature list; per-feature fingerprints with a one-sided-change rule.                                                     | **Fingerprints.** A checklist does not notice drift. A fingerprint of each language's sources makes any change visible, and the base-commit comparison tells a one-sided change from a parallel one. `--acknowledge` records deliberate exceptions, each with a reason and a link.                                                                         |
| Fixture enforcement        | Rely on the tests existing; parse test output; receipts.                                                                                                      | **Receipts.** A test that is filtered out, skipped or never built writes nothing, so a missing or short receipt proves the suite did not load the fixtures. Parsing logs would be fragile across `cargo test` and `dotnet test`.                                                                                                                          |
| Branch protection          | Classic branch protection; repository rulesets.                                                                                                               | **Classic protection.** Requirement 6 names the `branches/main/protection` endpoint as its test, and only classic protection makes it return 200. A ruleset can replace it later without changing the check names.                                                                                                                                       |
| Stale pull requests        | actions/stale; a custom script; manual triage.                                                                                                                | **actions/stale v11** for the policy, plus manual closing of the eight 2025 pull requests now. It is the maintained GitHub action for this job and has a dry-run mode (`debug-only`).                                                                                                                                                                    |
| Red `Dependencies` on main | GitHub's email notifications; an issue per failure; one tracking issue.                                                                                       | **One tracking issue**, opened, updated and closed by the workflow, found by a hidden marker. It does not depend on anyone's notification settings and never creates duplicates.                                                                                                                                                                         |
| zizmor versus actionlint   | Use `$/` and disable actionlint; ignore each finding inline; disable the audit with a reason and a replacement test.                                          | **Disable with a reason, plus a test.** Reusable workflows called through `./` are resolved from the same commit, so the risk the audit targets, unpinned local *actions* after a checkout, is covered by a policy test that forbids step-level `uses: ./`. Re-enable the audit when rhysd/actionlint#711 is fixed.                                               |

## Existing components used

- [`actions/stale`](https://github.com/actions/stale) v11.0.0 (released
  2026-07-28) for the stale policy.
- [actionlint](https://github.com/rhysd/actionlint) 1.7.12 with shellcheck
  through its Docker image, and [zizmor](https://docs.zizmor.sh/) 1.32.0.
  Both already ran in `workflows.yml`.
- GitHub reusable workflows (`workflow_call`) and `jobs.<id>.needs` for the
  gate. See the GitHub docs, "Reusing workflows".
- GitHub branch protection (REST: `PUT /repos/{owner}/{repo}/branches/{branch}/protection`).
- `taiki-e/install-action` for verified prebuilt Rust tools.

## Upstream reports

The one upstream defect found, actionlint's missing support for `$/`, is
already reported as rhysd/actionlint#711 (opened 2026-07-30 and still open),
so no new report was filed. The repository's workaround is recorded in
`.github/zizmor.yml`.

## Debugging aids added

Every new script has an opt-in verbose mode. CI turns each one on when a job
is re-run with debug logging. The switches are listed in
[troubleshooting](../../ci-cd/troubleshooting.md#debug-switches):

- `PARITY_DEBUG`
- `STACK_PATHS_DEBUG`
- `TRACKING_ISSUE_DEBUG`
- `TRACKING_ISSUE_DRY_RUN`
- the `debug_only` input of `stale.yml`
