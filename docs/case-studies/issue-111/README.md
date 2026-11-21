# Issue #111: a JavaScript-first CI gate, cross-language parity and repository governance

[Issue](https://github.com/link-foundation/link-cli/issues/111) ·
[Pull request](https://github.com/link-foundation/link-cli/pull/180)

The issue audits link-cli against the standard in link-assistant/router#759
(requirements 3, 4, 6 and 8), the shared pipeline templates, and
hive-mind's `docs/CI-CD-BEST-PRACTICES.md`. Its finding: the three stacks
were built in parallel with no gate between them, no check kept them at
parity, `main` was unprotected, and the only active pull request and the default branch were red.

## Data collected

The evidence in [`data/`](data/) includes the initial audit on 2026-11-14 and
the follow-up after #182 merged on 2026-11-20. It contains public run and
repository metadata.

| File                                                       | What it holds                                                                    |
| ---------------------------------------------------------- | -------------------------------------------------------------------------------- |
| [`issue-111.json`](data/issue-111.json)                    | The issue body. It has no comments ([`issue-111-comments.json`](data/issue-111-comments.json) is `[]`). |
| [`pr-180-comments.json`](data/pr-180-comments.json)        | Pull request conversation comments, including the owner's review of 2026-11-14 16:13. |
| [`branch-protection-before.json`](data/branch-protection-before.json) | `GET /branches/main/protection` before the change: 404 "Branch not protected". |
| [`branch-protection-after.json`](data/branch-protection-after.json) | After the change: the 200 from `GET /branches/main/protection`, the ruleset, the rules in effect on `main`, the `release` environment and the deploy key's metadata. |
| [`actions-bypass-422.json`](data/actions-bypass-422.json)  | GitHub's 422 when GitHub Actions is made a ruleset bypass actor. `experiments/issue-111/probe-actions-bypass.sh` reproduces it. |
| [`open-prs-before.json`](data/open-prs-before.json)        | The open pull requests before triage: number, last update, linked issues, check results. |
| [`ci-evidence.md`](data/ci-evidence.md)                    | The CI runs this pull request relied on, including the failures it fixed and the gate demonstration. |
| [`original-failures-excerpt.txt`](data/original-failures-excerpt.txt) | The October dependency and C# test failures, with the original log line numbers. |
| [`rust-44689293297-excerpt.txt`](data/rust-44689293297-excerpt.txt) | The default-branch Rust release push rejection after #182 merged, with original log line numbers. |

## Timeline

| When (UTC)       | Event                                                                                                                       |
| ---------------- | --------------------------------------------------------------------------------------------------------------------------- |
| 2025-09-10       | PRs #36-#41 and #49 open. Each duplicates an open issue (#35-#30, #18). None ever gets a CI result.                        |
| 2025-09-15       | PR #57 opens, duplicating issue #56. All eight stay open with no activity for 14 months.                                     |
| 2026-10-08–10    | Three scheduled `Dependencies` runs fail on `main`. The number of outdated declarations rises from 3 to 11. No tracking issue is opened. |
| 2026-10-09 03:49 | On a Dependabot branch, `C# CI/CD Pipeline`, `Dependencies`, `Docs` and `Security` all start in the same second: no stage order. |
| 2026-10-10       | Issue #111 is filed.                                                                                                         |
| 2026-10-12       | PR #109 (the NuGet group) is closed without merging. Later Dependabot pull requests replace it.                             |
| 2026-10-24       | `workflows.yml` (actionlint with shellcheck, zizmor, policy tests), `.github/zizmor.yml` and the CodeQL config land on `main` (1b8551dd, fb1e88e7, cfd03518). |
| 2026-11-06       | ESLint, Prettier and the `lint` script for `js/` land on `main` (fd17aa4e).                                                 |
| 2026-11-10/13    | Scheduled and push runs of `Dependencies` on `main` are green.                                                               |
| 2026-11-14 08:27 | Issue #176 closes. It held back `cargo-audit` 0.22.2 until `taiki-e/install-action` shipped a 0.23.0 manifest (v2.90.4). |
| 2026-11-14       | PR #180 adds the parity manifest, the checker and golden fixtures (50fc778d), stack path detection (6baffe6c), the JavaScript stage gate (4e540213), a stale workflow (later removed) and the tracking issue (62802d79), the zizmor reconciliation (eb278c5f) and the dependency bumps (f3639fbe). |
| 2026-11-14       | CI for 4e540213 fails `Workflows`, because zizmor 1.32's `self-repository` audit flags every `uses: ./…/javascript-stage.yml`. It also fails `Dependencies`, because #176 closed and three upstream releases appeared that morning. Both are fixed before the next push. |
| 2026-11-14 09:39 | The ruleset, classic protection, the deploy key and the `release` environment are configured on the repository; `GET /branches/main/protection` changes from 404 to 200. |
| 2026-11-14 09:52 | The Rust and C# golden fixture tests, which ran on Windows for the first time, fail on `windows-2025` with a hash mismatch: Windows checkouts had CRLF line endings (root cause 8). |
| 2026-11-14 09:43 | PRs #36-#41, #49 and #57 are closed with a comment; their branches and issues stay. |
| 2026-11-14 16:13 | The owner's review: reopen every closed pull request, remove the stale requirement (no stale pull request policy may exist), and add a full JavaScript server/embedded implementation next to the client. |
| 2026-11-14 16:16 | PRs #36-#41, #49 and #57 are reopened with a comment each; `stale.yml` is removed and a test forbids any stale policy. |
| 2026-11-14 16:31 | `js/server`, the package `@link-foundation/link-cli-server`, starts (1ce61b6a): a port of `rust/src/**` to plain JavaScript with no WebAssembly. Its own workflow, `js-server.yml`, runs behind the JavaScript stage (611bc991). |
| 2026-11-14 17:16-18:36 | The port grows feature by feature: the query language and command line, undo, permissions and triggers, triplets, the doublets store with names and types, the LiNo parser, transactions and version control, GraphQL, `.lenv`/`.env`, the protocol server, client and IPC transports, and the `<db>.lock` file lock. JavaScript becomes the third suite that must load every golden fixture (4d350095). |
| 2026-11-14 18:26 | The first CI run of `js-server.yml` on Linux, macOS and Windows with Node.js 20, 22 and 26 (1d825202) fails on Windows and on Node.js 20 and 22 (root causes 9-12). CodeQL flags two regular expressions as polynomial ReDoS (fixed by ac4526fe). |
| 2026-11-14 18:51-18:58 | Windows and old Node.js fixes (d0c65de9, 4fc2204a, e1315be9), complete coverage of every source file, `LockFileEx` locking on Windows through `node:ffi` and `bun:ffi` (88f4279a), and the package README. |
| 2026-11-14 19:14 | CI for 205c3c62 is green on Linux and macOS with Node.js 22 and 26, on Bun and Deno, and against the Rust `clink`. It fails on Node.js 20 and on Windows (root causes 13-14). |
| 2026-11-14 20:10 | `js-server.yml` for 24656c61 is green on every operating system and Node.js version. `DoubletsStorage`, a port of `rust/src/storage/doublets_storage.rs` and of the file-mapped doublets unit store, joins the package; `TransactionsDecorator` writes through it directly, and the JavaScript embedded-store example writes the same `db.links` as the Rust one. |
| 2026-11-14 20:15 | C# CI for 24656c61 crashes its test host in `IpcTransportTests` (root cause 15). Rust, C# and the JS server now end a stream when the peer removes the link before acknowledging the end (5a7064a5). |
| 2026-11-14 21:00 | The Deno unit tests of `js-server.yml` for 5a7064a5 fail: a flush did not publish a new revision (root cause 16). |
| 2026-11-14 21:31 | All 14 workflows pass for 9285f447. GitHub marks both CodeQL alerts fixed. |
| 2026-11-20       | #182 merges NuGet package-boundary checks, release recovery and newer dependencies. Both NuGet packages reach 5.1.0. Rust's automatic release on `main` still uses a token, and GitHub rejects its version commit under the required checks ruleset. |
| 2026-11-21       | PR #180 merges `main` to resolve conflicts in the C#, Security and WebAssembly workflows. The merge keeps NuGet recovery, allows `publish-current` from `main`, and gates the release on successful JavaScript checks. New dependency releases require another lockfile and pin refresh. |

## Requirements and how each is met

| #  | Requirement                                                                                  | Solution in PR #180                                                                                                                                                                                                                                                                                                                                                       | Verification                                                                                                                         |
| -- | -------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| 1  | Rust and C# jobs run only after the JavaScript stage succeeds for the same commit.            | The reusable `javascript-stage.yml` runs lint, `test:js` and parity. `javascript.yml` is its entry workflow. Every workflow that runs Rust or .NET calls it as job `javascript`, and each Rust or .NET job lists it in `needs:`. `needs:` only starts a job after its dependencies succeeded, in the same run and for the same commit. Jobs with `!cancelled()` also check `needs.javascript.result == 'success'`, so a cancelled or skipped stage cannot start them. | `workflow-policy.test.mjs` (gate rule). The deliberate failing-test run (commit 2c57efc5) skipped every Rust, .NET and WASM job; see [`data/ci-evidence.md`](data/ci-evidence.md#gate-demonstration-requirement-1-commit-2c57efc5). |
| 2  | Move `npm run test:js` out of `wasm.yml` into a stage that needs no Rust.                     | `test:js` now runs only in `javascript-stage.yml`, which installs only Node.js. `wasm.yml` keeps `test:wasm` and the WASM build behind the gate.                                                                                                                                                                                                                           | The policy test checks that the stage installs no Rust or .NET tooling and that `wasm.yml` does not run `test:js`.                    |
| 3  | A parity manifest and a checker that fails on a one-sided feature change, run everywhere with no path filter. | `parity/features.json` covers 30 features in 3 languages. `parity/check-parity.mjs` checks schema, coverage, fingerprints, one-sided changes and golden fixtures; see [PARITY.md](../../PARITY.md). It runs in the JavaScript stage that every stack workflow calls. `rust.yml` and `csharp.yml` lost their `paths:` filters; `stack-paths.mjs` skips unaffected build jobs inside the run instead. | `parity/check-parity.test.mjs`, and `ci-governance.test.mjs` ("required stage checks report on every pull request").                |
| 4  | Shared golden fixtures that both suites must load, and a CI job that fails otherwise.         | `parity/golden-fixtures.json` lists 41 `docs/protocol` fixtures with SHA-256. `rust/tests/golden_fixtures_tests.rs`, `GoldenFixturesTests.cs` and `js/test/golden-fixtures.test.mjs` load and hash all of them and write a receipt. `--receipt` in `rust.yml`, `csharp.yml` and `javascript-stage.yml` fails if a receipt is missing or incomplete. Every fixture lists C#, Rust and JavaScript consumer tests.                                                                                     | The receipt steps in CI. `ci-governance.test.mjs` checks that all three workflows keep them.                                              |
| 5  | Document what the `js/` workbench implements and what it delegates.                           | [JS-WORKBENCH.md](../../JS-WORKBENCH.md). The per-feature truth is the `javascript` cell in `parity/features.json`.                                                                                                                                                                                                                                                      | The parity checker keeps that cell honest. Review of the page.                                                                       |
| 6  | Branch protection or a ruleset on `main` that requires the three stage checks; the protection API returns 200. | Both, because neither alone works. The ruleset "main requires the JavaScript, Rust and C# stages" requires `Pipeline Status (javascript)`, `Pipeline Status (rust)` and `Pipeline Status (csharp)` from GitHub Actions, up to date with `main`, and forbids force pushes and deletion. Classic protection forbids force pushes and deletion too, which makes the protection API return 200. The release jobs push their version commits to `main`, and those commits carry no checks, so the ruleset lets only deploy keys bypass it. The jobs push with the deploy key in the `RELEASE_DEPLOY_KEY` secret of the `release` environment, which only `main` can use (root cause 7). | `gh api repos/link-foundation/link-cli/branches/main/protection` returns 200; `gh api repos/link-foundation/link-cli/rules/branches/main` lists the three checks. Both are saved in [`data/branch-protection-after.json`](data/branch-protection-after.json). `ci-governance.test.mjs` checks that every job that pushes a version commit uses the deploy key. |
| 7  | ~~Triage PRs #36-#41, #49 and #57, and flag or close PRs idle for more than 60 days.~~ **Reverted by the owner's review.** | The eight PRs were closed on 2026-11-14 and reopened the same day after the owner's review (see "Review feedback"). link-foundation repositories have no stale pull request policy: `stale.yml` was removed, and no workflow may close, label or rebase a pull request because of age. | `ci-governance.test.mjs` fails if any workflow uses `actions/stale`, a stale setting or closes pull requests. The reopening is recorded in [`data/ci-evidence.md`](data/ci-evidence.md). |
| 8  | Fix or close PR #109, and make a red scheduled `Dependencies` on `main` open or update a tracking issue. | #109 was already closed on 2026-10-12. The new `tracking-issue` job in `dependencies.yml` runs on `main` after `latest`. It opens "Dependencies check is failing on main", comments on it while the check stays red, and closes it after the next green run. The held-back `cargo-audit` pin became due during this work and was updated.                                       | `dependency-tracking-issue.test.mjs`, and a dry run against the live repository (see [troubleshooting](../../ci-cd/troubleshooting.md#dependencies)). |
| 9  | `workflows.yml`, `.github/zizmor.yml`, the CodeQL config, and ESLint/Prettier for `js/`, with their findings fixed. | All of these already existed on `main` (see the timeline). This PR fixed zizmor's new `self-repository` finding (see root cause 5), and documented every linter and its local command in [troubleshooting](../../ci-cd/troubleshooting.md#workflow-lint-actionlint-and-zizmor). | `Workflows` and the JavaScript stage are green. Locally, actionlint 1.7.12 and zizmor 1.32.0 report nothing.                          |
| 10 | `CONTRIBUTING.md`, `AGENTS.md`/`CLAUDE.md` and `.github/CODEOWNERS` with the single-pusher rule. | [CONTRIBUTING.md](../../../CONTRIBUTING.md) and [AGENTS.md](../../../AGENTS.md) state the rule and give the command that confirms no run is in progress. `CLAUDE.md` imports `AGENTS.md`. `.github/CODEOWNERS` requests the maintainer's review.                                                                                                                         | Review. CODEOWNERS is valid when GitHub shows no error banner on the file.                                                            |
| 11 | A CI check that fails if a workflow can start Rust or C# without the JavaScript gate.         | `gateViolations()` in `workflow-policy.test.mjs` finds every job that installs or runs `cargo`, `rustup`, `wasm-pack`, `dotnet` or `setup-dotnet`. That includes jobs reached through nested reusable workflows. It fails unless the job transitively `needs` a job that calls `javascript-stage.yml`, and unless every `always()`/`!cancelled()` condition also requires that job's success. | Synthetic positive and negative cases, plus a check of every real workflow, in `Workflow Policy Tests`.                              |
| 12 | A full JavaScript server/embedded implementation next to the JavaScript client (the owner's review). | `js/server` is `@link-foundation/link-cli-server`: the Rust `clink` in plain JavaScript for Node.js 20+, Bun and Deno. It has the same command line, output, errors and exit codes, writes the same store files, and has the same embedded API (`NamedTypesDecorator`, `QueryProcessor`, the transactions, version control, trigger and permission decorators, triplets), the file-backed `DoubletsStorage` over a port of the doublets unit store (byte for byte the Rust database file, changed in place, under the same `TransactionsDecorator`), the protocol server and client over every IPC kind the runtimes support, `--serve --graphql`, and the `<db>.lock` lock. See [its README](../../../js/server/README.md) and [ARCHITECTURE.md](../../ARCHITECTURE.md#javascript-server-and-embedded-store). Each feature's `javascript` cell in `parity/features.json` points at it. | `js-server.yml`: unit tests with 100 % line, branch and function coverage on Node.js, Bun and Deno; the unit tests on Linux, macOS and Windows with Node.js 20, 22 and 26; differential tests that run each invocation through the Rust `clink` and `bin/clink.js` and compare output, status and files; and the client's conformance scenarios against `clink --serve` from JavaScript (`CLINK_JS`). `DoubletsStorage` is checked against golden database files written by Rust, against the Rust `DoubletsStorage` through `rust/examples/unit_store_trace.rs` on fixed and random scenarios (`CLINK_RUST_UNIT_STORE`), and by `examples/embedded-store/embedded-store.mjs` on Node.js, Bun and Deno, whose `db.links` is identical to the one `rust/examples/embedded_store.rs` writes. |

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
   rulesets or CODEOWNERS, and a failing scheduled run produced only an email
   to whoever had notifications on. (Old pull requests staying open is not a
   defect: the owner's review states that link-foundation repositories have no
   stale pull request policy.)
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
7. **The release jobs push to `main` themselves.** `rust/scripts/version-and-commit.rs`
   and `csharp/scripts/version-and-commit.mjs` commit the version bump and
   push it to `main` with `GITHUB_TOKEN`. That new commit has no checks, so
   any rule requiring checks rejects it. The usual fix, making GitHub Actions
   a bypass actor, is not available for this repository: GitHub answers 422
   "Actor GitHub Actions integration must be part of the ruleset source or
   owner organization" ([`data/actions-bypass-422.json`](data/actions-bypass-422.json)).
   Classic protection cannot exempt anyone but administrators from required
   checks.
8. **Windows checkouts change the golden fixtures.** The fixtures are hashed
   byte for byte, but Windows runners default to `core.autocrlf=true`, so
   git wrote CRLF into 39 of the 41 fixtures and both suites' hash checks
   failed on `windows-2025` only (runs 43581837745 and 43581837858). The fixtures were
   already read on Windows before, but by parsers that accept either line
   ending, so nothing had noticed. `.gitattributes` now marks
   `docs/protocol/**` as `-text`, and the parity checker fails if a fixture
   loses that attribute, so the JavaScript stage catches it on Linux.

9. **Windows has no directory `fsync`.** The JavaScript store synced the
   parent directory after it wrote a file, as on POSIX. Windows cannot open
   a directory that way, and the Rust store syncs the parent directory only
   under `cfg(unix)`. `syncParentDirectory` now skips it on Windows too, and
   paths are joined with a backslash there, as Rust's `Path::join` does.
   The test fixtures also needed LF line endings in `.gitattributes`.
10. **A Windows named pipe cannot half-close.** `pipe:` is a named pipe on
    Windows and a FIFO elsewhere. The IPC tests that ended a request by
    closing their write side now frame it with a length prefix on Windows,
    and the rendezvous test uses `fifo:`.
11. **Node.js 20 does not expand test globs.** `node --test 'test/**/*.test.mjs'`
    needs Node.js 21 or later, and Windows shells expand no globs either.
    `scripts/run-tests.mjs` lists the test files itself.
12. **`socket.resetAndDestroy()` before a write sends FIN on Node.js 22 and
    earlier.** A GraphQL test expected a reset and got an orderly close.
    The test now writes before it resets, which behaves the same everywhere
    ([`experiments/issue-111/reset-and-destroy-events.mjs`](../../../experiments/issue-111/reset-and-destroy-events.mjs)).
13. **libuv opens a directory for writing on Windows.** It always passes
    `FILE_FLAG_BACKUP_SEMANTICS`, so `openSync(dir, 'r+')`, `'a+'` and
    `O_RDWR | O_CREAT` succeed there. A directory in place of the
    transitions log or the lock file was opened silently, where `open(2)`
    and Rust's `File::open` fail. `openFileSync` (`src/open-file.js`) now
    checks what it opened and fails with `EISDIR` on every platform.
14. **`LockFileEx` is mandatory, `flock(2)` is advisory.** The Windows
    lock, like Rust's `File::lock`, covers the whole file, so another handle
    can open a locked sidecar but cannot read it (`EBUSY`). The test now
    expects this behaviour. The same CI run (205c3c62) also showed that Node.js 20's
    test runner writes its serialized reports to the standard output the
    `bin/clink.js` test captured, that `EWOULDBLOCK` is 140 on Windows
    rather than an alias of `EAGAIN`, and that Node.js has no `AF_UNIX`
    sockets on Windows. The tests now handle each of these.
15. **A System V peer that hangs up also removes its ACK.** C# run
    [43635026592](https://github.com/link-foundation/link-cli/actions/runs/43635026592)
    crashed the test host with `IpcException: the peer hung up`, thrown by
    `CloseWrite`. The peer read EOF and closed, and closing removes both
    queues, so the retransmitted EOF failed with `EIDRM` before the ACK or
    ABORT could be read. Rust, C# and the JS server now treat that failure
    while sending the end as a hang-up (5a7064a5). In each language a
    regression test uses a link that vanishes on EOF.
16. **Deno stores a modification time one millisecond early.**
    `DoubletsStorage.flush` publishes a revision by setting the file's
    modification time to now, as Rust does with `SystemTime::now()`. The
    JavaScript clock has only milliseconds, so a flush in the millisecond of
    the last change kept the old revision. Deno 2.9 also truncates the time it
    is given to whole milliseconds, and `x.370` as a binary fraction is
    `x.36999…`, so about half the times land one millisecond early. The Deno
    unit tests for 5a7064a5 failed on this. The flush now asks for the middle
    of a millisecond later than the current modification time
    ([`experiments/issue-111/futimes-millisecond-truncation.mjs`](../../../experiments/issue-111/futimes-millisecond-truncation.mjs)).
    The bounded experiment reproduced the issue on Deno 2.9.7 and 2.9.8 on
    Linux. Each version stored 496 of 1,000 Date arguments one millisecond
    early and stored all 1,000 midpoint arguments in the requested
    millisecond. The upstream report includes a fixed-timestamp example and
    a proposed fix. [denoland/deno#37212](https://github.com/denoland/deno/issues/37212)

## Solutions considered

| Problem                    | Options                                                                                                                                                       | Chosen, and why                                                                                                                                                                                                                                                                                                                                      |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Stage gate                 | (a) `workflow_run` chained on the JavaScript workflow; (b) one reusable JavaScript workflow called from each stack workflow as a `needs:` dependency.          | **(b).** `workflow_run` runs on the default branch's copy of the workflow, cannot report its checks on the pull request's head commit without extra API calls, and needs `head_sha` matching. With `needs:`, the gate is in the same run and on the same commit, the job is shown as skipped when the stage fails, and the policy test can check it statically. The cost is that each workflow runs the stage again, which takes about a minute of Node.js time. |
| Path filters               | Keep `paths:` and also run parity elsewhere, or remove `paths:` and decide inside the run.                                                                    | **Remove.** A required check from a workflow that a path filter skipped stays "Expected" forever and blocks the merge (GitHub docs: "Handling skipped but required checks"). `stack-paths.mjs` gives the same savings inside the run.                                                                                                                       |
| Parity manifest            | Hand-written checklists; a generated feature list; per-feature fingerprints with a one-sided-change rule.                                                     | **Fingerprints.** A checklist does not notice drift. A fingerprint of each language's sources makes any change visible, and the base-commit comparison tells a one-sided change from a parallel one. `--acknowledge` records deliberate exceptions, each with a reason and a link.                                                                         |
| Fixture enforcement        | Rely on the tests existing; parse test output; receipts.                                                                                                      | **Receipts.** A test that is filtered out, skipped or never built writes nothing, so a missing or short receipt proves the suite did not load the fixtures. Parsing logs would be fragile across `cargo test` and `dotnet test`.                                                                                                                          |
| Branch protection          | (a) Classic protection with required checks; (b) a ruleset with GitHub Actions as bypass actor; (c) a ruleset with a deploy key as bypass actor; (d) release through a pull request. | **(c), plus classic protection without required checks.** (a) rejects every release push, because `GITHUB_TOKEN` is not an administrator. (b) is rejected by GitHub with a 422 (root cause 7). (d) would rewrite both release pipelines. A deploy key is the bypass actor GitHub accepts. Its private key is a secret of the `release` environment, which only `main` can deploy to, so a workflow on another branch cannot read it. Classic protection without required checks keeps the protection endpoint at 200 and does not block the deploy key. |
| Stale pull requests        | actions/stale; a custom script; manual triage; no policy.                                                                                                      | **No policy** (the owner's decision in review). An old pull request is work that can still continue, so none is ever closed, labelled or rebased for age. The first version of this PR used actions/stale v11 and closed eight PRs; both were reverted. |
| Red `Dependencies` on main | GitHub's email notifications; an issue per failure; one tracking issue.                                                                                       | **One tracking issue**, opened, updated and closed by the workflow, found by a hidden marker. It does not depend on anyone's notification settings and never creates duplicates.                                                                                                                                                                         |
| zizmor versus actionlint   | Use `$/` and disable actionlint; ignore each finding inline; disable the audit with a reason and a replacement test.                                          | **Disable with a reason, plus a test.** Reusable workflows called through `./` are resolved from the same commit, so the risk the audit targets, unpinned local *actions* after a checkout, is covered by a policy test that forbids step-level `uses: ./`. Re-enable the audit when rhysd/actionlint#711 is fixed.                                               |

## Existing components used

- [actionlint](https://github.com/rhysd/actionlint) 1.7.12 with shellcheck
  through its Docker image, and [zizmor](https://docs.zizmor.sh/) 1.32.0.
  Both already ran in `workflows.yml`.
- GitHub reusable workflows (`workflow_call`) and `jobs.<id>.needs` for the
  gate. See the GitHub docs, "Reusing workflows".
- GitHub repository rulesets (REST: `POST /repos/{owner}/{repo}/rulesets`)
  with a `DeployKey` bypass actor, and classic branch protection (REST:
  `PUT /repos/{owner}/{repo}/branches/{branch}/protection`).
- A deploy key with write access, used through the `ssh-key` input of
  `actions/checkout`, and a GitHub deployment environment that holds it.
- `taiki-e/install-action` for verified prebuilt Rust tools.

## Upstream reports

- actionlint rejects the `$/` syntax recommended by zizmor. The workaround
  is recorded in `.github/zizmor.yml`. [rhysd/actionlint#711](https://github.com/rhysd/actionlint/issues/711)
- Deno can write a file timestamp one millisecond earlier than requested.
  The report includes a standalone reproduction, the midpoint workaround and
  a suggestion to use integer millisecond arithmetic for `Date` arguments.
  [denoland/deno#37212](https://github.com/denoland/deno/issues/37212)

## Debugging aids added

Every new script has an opt-in verbose mode. CI turns each one on when a job
is re-run with debug logging. The switches are listed in
[troubleshooting](../../ci-cd/troubleshooting.md#debug-switches):

- `PARITY_DEBUG`
- `STACK_PATHS_DEBUG`
- `TRACKING_ISSUE_DEBUG`
- `TRACKING_ISSUE_DRY_RUN`

The JavaScript server adds three more:

- `clink --trace` prints each step of the store, the transactions layer and
  every transport event of `--serve`, `--connect` and `--forward`.
- `CLINK_IPC_TRACE=1` turns the transport trace on without `--trace`.
- `CLINK_DIFFERENTIAL_DEBUG=1` makes the differential tests print each
  invocation and both implementations' outputs.

The experiments behind the fixes above are kept in `experiments/issue-111/`.
