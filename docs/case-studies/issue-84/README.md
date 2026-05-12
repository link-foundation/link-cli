# Issue 84 Case Study: C# CI/CD Failed to Deliver NuGet and GitHub Releases

Issue: https://github.com/link-foundation/link-cli/issues/84

Prepared PR: https://github.com/link-foundation/link-cli/pull/85

Failed run: https://github.com/link-foundation/link-cli/actions/runs/25757419575

## Requirements

Restated from issue #84:

1. Investigate why run `25757419575` failed to deliver the C# NuGet package `clink@2.4.0` and the matching `csharp-v2.4.0` GitHub Release.
2. Download issue details, run metadata, and CI logs into `docs/case-studies/issue-84`.
3. Compare every CI/CD file against the upstream JavaScript, Rust, and C# pipeline templates and reuse best practices.
4. Find root causes and propose solutions for each defect.
5. Search online for additional facts and known prior art (existing libraries/components/patterns that solve the same problem).
6. If the same defect exists in any template repository, report it upstream with a reproducer, workaround, and suggested fix.
7. Add regression coverage and implement the fix in a single pull request.

## Timeline

- `2026-05-12T16:57:31Z`: PR 83 (issue 82 fix) merged to `main` as `d39285a`. CI for that commit completed but the C# release was blocked by the original issue 82 bug.
- `2026-05-12T19:30:58Z`: Push of `d39285a` finally scheduled run `25757419575` (the issue 82 fix had cleared the path-filter block, so the next push to `main` ran C# CI/CD).
- `2026-05-12T19:34:20Z` (attempt 1, Release job): `version-and-commit.mjs --mode changeset` merged 12 changesets, bumped `csharp/Foundation.Data.Doublets.Cli/Foundation.Data.Doublets.Cli.csproj` from `2.3.0` to `2.4.0`, created `csharp/CHANGELOG.md`, committed `b52c8f1`, created tag `csharp-v2.4.0`, pushed both commit and tag, set `version_committed=true`. Evidence: `evidence/ci-logs/attempt1/Release/7_Version and commit.txt`.
- `2026-05-12T19:34:36Z` (attempt 1, Release job): `dotnet nuget push` returned HTTP 403 `The specified API key is invalid, has expired, or does not have permission to access the specified package.` Evidence: `evidence/ci-logs/attempt1/Release/10_Publish to NuGet.txt:19-20`.
- `2026-05-12T19:34:37Z` (attempt 1): Release job failed with exit code 1. `Verify package on NuGet`, `Create GitHub Release` did not run.
- `2026-05-12T19:43:19Z` (attempt 2, re-run): `version-and-commit.mjs` detected tag `csharp-v2.4.0`, exited with `already_released=true` and **without** `version_committed=true`. Build/Resolve/Publish/Verify/Create release were all skipped (gated on `version_committed == 'true'`). Job reported success.
- `2026-05-12T19:43:54Z`: Run `25757419575` overall conclusion was `success` (GitHub reports the latest attempt's status). No NuGet package and no GitHub release exist.

Verified post-state:
- `https://api.nuget.org/v3-flatcontainer/clink/index.json` returns versions up to `2.2.2`; no `2.3.0` or `2.4.0`.
- `gh api repos/link-foundation/link-cli/releases` returns no `csharp-v*` entries.
- `git rev-parse refs/tags/csharp-v2.4.0` resolves to `b52c8f1`.

## Evidence

- Issue: `evidence/issue-84.json`, `evidence/issue-84-comments.json`.
- PR: `evidence/pr-85.json`, `evidence/pr-85-comments.json`, `evidence/pr-85-reviews.json`.
- Run metadata: `evidence/runs-d39285a.json`, `evidence/run-25757419575.json`, `evidence/run-25757419575-attempts.json`, `evidence/run-25757419575-jobs.json`.
- Per-attempt logs: `evidence/ci-logs/attempt1/Release/10_Publish to NuGet.txt` (the HTTP 403), `evidence/ci-logs/attempt1/Release/7_Version and commit.txt` (the bump and tag push), `evidence/ci-logs/run-25757419575-full.log` (attempt 2).
- NuGet state: `evidence/nuget-clink-index.json`, `evidence/nuget-clink-2.4.0.headers.txt`.
- GitHub Release state: `evidence/csharp-releases.json`.
- Tag state: `evidence/git-tag-csharp-v2.4.0.txt`.
- Templates fetched for comparison: `evidence/templates/csharp-release.yml`, `evidence/templates/csharp-version-and-commit.mjs`, `evidence/templates/csharp-create-github-release.mjs`, `evidence/templates/rust-release.yml`, `evidence/templates/js-release.yml`, `evidence/templates/js-check-release-needed.mjs`, `evidence/templates/js-publish-to-npm.mjs`.

Important log anchors:

- `evidence/ci-logs/attempt1/Release/7_Version and commit.txt:31-39` shows `b52c8f1` committed, `csharp-v2.4.0` tag created and pushed, `version_committed=true`.
- `evidence/ci-logs/attempt1/Release/10_Publish to NuGet.txt:17-21` shows `Pushing clink.2.4.0.nupkg ... Forbidden ... 403 (The specified API key is invalid, has expired, or does not have permission to access the specified package.) ... Process completed with exit code 1`.
- `evidence/ci-logs/run-25757419575-full.log` (attempt 2) shows `Tag csharp-v2.4.0 already exists` and `already_released=true`; all downstream Build/Resolve/Publish/Verify/Release steps absent.

## Online Facts

- GitHub Actions reports an overall run `conclusion` based on the **latest attempt**: a re-run that succeeds (or no-ops) hides the original failure unless attempts are inspected individually. Source: https://docs.github.com/en/actions/managing-workflow-runs-and-deployments/managing-workflow-runs/re-running-workflows-and-jobs
- NuGet returns `HTTP 403 The specified API key is invalid, has expired, or does not have permission to access the specified package.` when (a) the API key is expired, (b) the key has been revoked, (c) the key's `Glob Pattern` does not include the package id, or (d) the package id is reserved/owned by a different account and the key cannot push it. Sources: https://learn.microsoft.com/en-us/nuget/nuget-org/scoped-api-keys, https://learn.microsoft.com/en-us/nuget/nuget-org/publish-a-package#publish-with-dotnet-cli
- NuGet flat-container endpoint `https://api.nuget.org/v3-flatcontainer/{id-lowercase}/{version}/{id-lowercase}.nuspec` returns `200` only after the package is registered globally on the read CDN; immediately after `dotnet nuget push`, it may be `404` for several seconds. Source: https://learn.microsoft.com/en-us/nuget/api/package-base-address-resource
- `dotnet nuget push --skip-duplicate` only suppresses 409 conflicts; it does not retry transient HTTP failures and exits non-zero on 403. Source: https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-nuget-push
- GitHub Actions does not re-trigger `push` workflows for commits authored by `GITHUB_TOKEN` (default behavior), which is why the bot-pushed `b52c8f1` produced no new CI run. Source: https://docs.github.com/en/actions/security-for-github-actions/security-guides/automatic-token-authentication#using-the-github_token-in-a-workflow
- The JavaScript template (`js-ai-driven-development-pipeline-template`) documents in `check-release-needed.mjs` that "Git tags can exist without the package being published … only npm publication means users can actually install the package" and provides a self-healing path that re-publishes when no package is found in the registry. Source: `evidence/templates/js-check-release-needed.mjs`.
- The Rust template uses `crate_published` from `check-release-needed.rs` and gates `cargo publish` on `crate_published != 'true'`, then unconditionally waits for crate availability afterwards. Source: `evidence/templates/rust-release.yml:347-358`.

## Template Comparison

We fetched the C# template, the JS template, and the Rust template release workflows and the C# template's release scripts.

### Same defect (C# template still has it)

- `csharp-version-and-commit.mjs` (template) defines `exec(command, silent)` that swallows command failures and returns `''` (lines 54-61). `checkTagExists(version)` (lines 136-143) calls `exec('git rev-parse v${version}', true)` and treats any thrown error as "tag missing". Because the silent exec swallows the exit-128 from `git rev-parse`, a missing tag is reported as existing (the issue 82 bug). The local repository already fixed this (`csharp/scripts/version-and-commit.mjs:54-56`, `:131-138`), but the upstream template has not. Upstream issue: https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/issues/9 (still OPEN as of this writing).

  We verified this by replaying both wrappers in `/tmp/test-tag-bug` with a fresh repo and a missing tag — the template wrapper returns `true`, the local wrapper returns `false`.

- The C# template has the **same idempotency gap** that caused issue 84: build/publish/verify/release are all gated on `steps.version.outputs.version_committed == 'true'`. If `version-and-commit.mjs` exits with `already_released=true` (e.g., after a prior publish failure on a re-run), every downstream step is skipped. This is silently fatal whenever publish or verification fails after the commit and tag are pushed.

### Best practices present in JS template (not in C#)

- **Self-healing release detection** (`js-release.yml:439-445`): publish runs when `version_committed == 'true'`, OR `already_released == 'true'`, OR (`should_release == 'true'` AND `skip_bump == 'true'`). The last branch fires when there are no changesets but the current package version is not yet on npm — the re-run resumes a stuck release. The script is `scripts/check-release-needed.mjs`.
- **Wait-for-registry with retry** (`js-release.yml:447`, `scripts/publish-to-npm.mjs`): the publish script handles its own existence check, `--should-pull`, retry-on-transient-error, and explicit "did we actually publish?" output.

### Best practices present in Rust template (not in C#)

- **Source-of-truth registry probe before publish** (`rust-release.yml:329-330, 348-349`): `check-release-needed.rs` sets `crate_published`; `cargo publish` only runs when `crate_published != 'true'`. Rust never re-publishes an existing crate version even on re-runs, and skipping publish is not a silent error.
- **Unconditional `wait-for-crate`** (`rust-release.yml:356-358`): once `should_release == 'true'`, `wait-for-crate.rs` waits regardless of whether this attempt published, so the rest of the pipeline (GitHub Release, Docker) only proceeds when the version is globally visible on `crates.io`.

### Best practices present in C# template (kept and reused)

- NuGet flat-container verification loop with growing sleeps (template lines 296-314, mirrored locally at `.github/workflows/csharp.yml:307-325`).
- Changeset merging + `version-and-commit.mjs` separation.
- `dotnet nuget push --skip-duplicate` (idempotent on 409 conflicts).

## Root Causes

1. **Invalid or insufficiently-scoped `NUGET_API_KEY` (immediate cause)**

   `dotnet nuget push clink.2.4.0.nupkg` returned `HTTP 403 (The specified API key is invalid, has expired, or does not have permission to access the specified package.)` (`evidence/ci-logs/attempt1/Release/10_Publish to NuGet.txt:17-21`). This was the actual delivery failure for issue 84. The clink package's API key is owner-side state we cannot self-heal in code, but the workflow can validate and surface it.

2. **No release idempotency: tag/commit are pushed before publish succeeds**

   `version-and-commit.mjs` commits + tags + pushes both **before** `dotnet nuget push` runs. When publish fails (root cause 1), the tag is already public on `main`. The next attempt sees the tag and short-circuits with `already_released=true`, but `version_committed` is empty, so every gated downstream step (build/resolve/publish/verify/release) is skipped. The job goes green and no recovery is possible without manual intervention or new code changes.

   This is a workflow design defect, not an upstream secret problem: even with a valid key, any transient publish failure would put the pipeline into the same dead state. The JS template proves this is solvable with `should_release && skip_bump` self-healing; the C# pipeline lacks the equivalent.

3. **No upfront credential validation**

   The workflow uses the secret only at publish time. A missing or invalid `NUGET_API_KEY` is detected only after the version commit, tag, and push are made, maximizing the cost of failure.

4. **GitHub overall run status hides the original failure**

   When the re-run no-ops successfully, GitHub reports `conclusion=success` for the run as a whole. The original failure is visible only under `attempts`. This made the issue easy to miss until the user noticed NuGet had no new version. Mitigation: surface the no-op via a hard step that ensures the publish/release happened when the version is expected to be new.

## Implemented Solution

All changes are scoped to `.github/workflows/csharp.yml`, `csharp/scripts/`, and a regression test, and use patterns already shipping in the JS and Rust templates.

### 1. Self-healing release detection (port of JS `check-release-needed.mjs`)

Added `csharp/scripts/check-release-needed.mjs`. Inputs: `HAS_CHANGESETS`. Behavior:

- If changesets exist → `should_release=true`, `skip_bump=false`.
- Else, query `https://api.nuget.org/v3-flatcontainer/{id-lower}/index.json`:
  - If the csproj `<Version>` is **in** the published list → `should_release=false`.
  - If the csproj `<Version>` is **not** in the list → `should_release=true`, `skip_bump=true` (self-healing).

Also probes `https://api.github.com/repos/{owner}/{repo}/releases/tags/csharp-v{version}` to determine whether the matching GitHub Release exists.

### 2. Release workflow becomes idempotent

`.github/workflows/csharp.yml` `release` and `instant-release` jobs:

- Added `Validate NuGet API key` step that runs immediately after checkout. If `NUGET_API_KEY` is missing or `dotnet nuget push --dry-run` returns 401/403 against a synthetic non-existent version, the job fails fast **before** any version commit. (Fast feedback; preserves the existing `skip if secret unset` semantics by treating absent-secret as "warn and skip" only in the `instant-release` path where the operator opted out.)
- Added `Check release needed` step (`check-release-needed.mjs`) right after `Check for changesets`.
- The `Version and commit` step now runs when `has_changesets == 'true' && skip_bump != 'true'` (the unchanged default path) and is skipped on self-heal runs.
- The build/resolve/publish/verify/release steps are now gated on `(version_committed == 'true' || already_released == 'true' || (should_release == 'true' && skip_bump == 'true'))`. The condition is identical in structure to JS template `release.yml:442-445`.
- The publish step verifies upfront that the version under release is missing from NuGet (via the flat-container index) and skips publish only if it is already there; the existing `--skip-duplicate` handles the same case at the registry.

### 3. Atomic version commit and tag push

`csharp/scripts/version-and-commit.mjs` is split into two phases:

- Phase A (default, runs in `Version and commit`): bump csproj, update changelog, remove changesets, **commit**, **push commit**, but **do not create or push the tag**. Output `version_committed=true`, `new_version=X.Y.Z`.
- Phase B (new `--mode finalize-tag`): after `Verify package on NuGet` succeeds, create the annotated tag `csharp-vX.Y.Z` for the already-pushed release commit and push only the tag. If the tag already exists locally or remotely, no-op safely.

The new workflow step ordering is: version-and-commit (commit+push) → build → resolve id → publish → verify on NuGet → finalize tag → create GitHub release. The tag is the public marker of "this version is on NuGet and has a release", so it is created last.

### 4. Verbose tracing and post-release verification

- `check-release-needed.mjs` logs the package id, the csproj version, the NuGet flat-container query URL, and the published-versions list. The output is added to the CI job summary via `GITHUB_STEP_SUMMARY`.
- A new `Assert release published` step at the end of the `release` and `instant-release` jobs checks that, when a release was expected (`should_release == 'true'`), the NuGet package and the GitHub release both exist. If either is missing, the step fails the job, so a no-op cannot silently report success on a re-run.

### 5. One-shot recovery for the current stuck state (`csharp-v2.4.0`)

The fix above makes future releases self-healing, but the current `csharp-v2.4.0` tag is already on `main` with no package and no release. To recover, the same self-healing path now applies: on the next push to `main`, `check-release-needed.mjs` will see csproj `<Version>2.4.0</Version>`, find no `2.4.0` on NuGet, set `should_release=true, skip_bump=true`, and the publish + verify + create-release steps will run on the existing `b52c8f1` commit. The `finalize-tag` step will see the tag exists and skip.

The recovery is therefore the natural consequence of merging this PR, provided the operator has rotated/repaired `NUGET_API_KEY` first.

### 6. Regression coverage

`csharp/scripts/check-release-needed.test.mjs`:

- Test 1: changesets present → `should_release=true, skip_bump=false`.
- Test 2: no changesets, csproj version found on NuGet (mock) → `should_release=false`.
- Test 3: no changesets, csproj version missing from NuGet (mock) → `should_release=true, skip_bump=true`.
- Test 4: NuGet flat-container returns 404 for `{id}/index.json` (package never registered) → treated as "version missing", `should_release=true, skip_bump=true`.

`csharp/scripts/version-and-commit.test.mjs` (existing tests retained) plus a new test that asserts:

- After Phase A in a temp git repo, the commit exists on `HEAD` but the tag `csharp-vX.Y.Z` does **not** exist.
- After Phase B, the tag exists, is annotated, and points at the release commit.
- Calling Phase B twice is a no-op the second time (idempotent).

`.github/workflows/csharp.yml` repository-layout test: the existing test from issue 82 is extended to assert that the release job has `skip_bump` and `already_released` branches in its publish gate.

### 7. Updated templates upstream

We do not modify the template repository here. We file the upstream issue (root cause 2 also affects the template) so future template users do not hit the same dead state.

## Validation

- `node --test csharp/scripts/check-release-needed.test.mjs` (new): all four cases pass with a mocked `fetch` for the NuGet flat-container endpoint.
- `node --test csharp/scripts/*.test.mjs`: existing 12 tests + new 5 tests pass.
- `dotnet test csharp/Foundation.Data.Doublets.Cli.sln --configuration Release`: passes (no behavior change in C# code).
- `git diff --check`: clean.
- A focused local replay of the issue scenario:
  1. Initialize a tmp repo with csproj `<Version>2.4.0</Version>` and tag `csharp-v2.4.0`.
  2. Run `node csharp/scripts/check-release-needed.mjs` with mocked NuGet response that excludes `2.4.0`.
  3. Assert outputs: `should_release=true`, `skip_bump=true`.
- Run `version-and-commit.mjs --mode finalize-tag` twice in a row → second call is a no-op (idempotent), tag still points at the original commit.

Validation logs: `evidence/test-logs/`.

## Upstream Reports

- [link-foundation/csharp-ai-driven-development-pipeline-template#9](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/issues/9) — already filed by issue 82 (silent `exec` + missing-tag-reported-as-existing). Still OPEN. We added a comment summarizing issue 84 as a second symptom of the same wrapper bug.
- A new upstream issue is filed against the C# template for the **idempotency gap** (no `skip_bump` / no self-healing) — same root cause 2 as this case study.

## Remaining Watch Items

- `NUGET_API_KEY` must be rotated/repaired by the repository owner before the next push to `main` can publish `clink@2.4.0`. The workflow now validates the key upfront and will fail loudly until the key is fixed.
- If recovery for `2.4.0` is not desired (e.g., the team wants to ship `2.4.1` instead), bump csproj to `2.4.1` and add a patch changeset; the new self-healing path will produce a normal release.
- After this PR merges, the next push to `main` is expected to: validate NuGet key → detect csproj `2.4.0` missing from NuGet → publish `clink@2.4.0` → wait for flat-container availability → create GitHub release `csharp-v2.4.0` → skip tag creation (tag already exists). Assert-release-published guards against any of these steps being silently skipped again.
