# Issue 75 CI/CD Case Study

## Scope

Issue: https://github.com/link-foundation/link-cli/issues/75

PR: https://github.com/link-foundation/link-cli/pull/76

The issue asked for a deep CI/CD investigation after the C# release job failed and the Rust release job skipped, leaving the C# and Rust components without component releases.

## Timeline

- 2026-05-09T07:06:13Z: PR 74, "Remove unmaintained wee_alloc dependency", was merged.
- 2026-05-09T07:08:11Z: C# CI/CD run 25594941803 started on SHA `70a959516ae64dd5878b72f5cb6af961a765aedc`.
- 2026-05-09T07:08:11Z: Rust CI/CD run 25594941825 started on the same SHA.
- 2026-05-09T07:09:58Z: Rust `Build Package` completed successfully and `Auto Release` was marked skipped.
- 2026-05-09T07:11:16Z: C# `Release` failed while merging changesets.

## Evidence

Downloaded evidence is stored in `docs/case-studies/issue-75/evidence/`.

- `run-25594941803.json` and `run-25594941803.log`: C# CI/CD run metadata and full log.
- `run-25594941825.json` and `run-25594941825.log`: Rust CI/CD run metadata and full log.
- `release-script-tests-before.log`: local failing reproduction of the release script defects.
- `template-csharp-release.yml` and `template-rust-release.yml`: upstream workflow templates used for comparison.
- `recent-runs-main.json`, `recent-runs-issue-branch.json`, `recent-merged-prs.json`, `recent-releases.txt`, and `releases.json`: repository state around the incident.

Key log lines:

- C# run: `run-25594941803.log:5712` ran `node scripts/merge-changesets.mjs --dir csharp/.changeset`.
- C# run: `run-25594941803.log:5726` failed with `ENOENT: no such file or directory, scandir '.changeset'`.
- C# run metadata: all lint/test/build jobs passed; only `Release` failed.
- Rust run metadata: lint, test, and build passed; `Changelog Fragment Check` and `Auto Release` were skipped.

## Root Causes

1. The C# merge script accepted `--dir csharp/.changeset` in the workflow, but the script ignored it and always scanned root `.changeset`.
2. The same script still used the template package placeholder `MyPackage`, so it could not correctly parse the real package `Foundation.Data.Doublets.Cli`.
3. The Rust bump script accepted `--dir rust/changelog.d` in the workflow, but it ignored that option and always scanned root `changelog.d`.
4. The GitHub release helper accepted `--tag-prefix`, but it ignored that option and always generated `v<version>`. It also read root `CHANGELOG.md` instead of the component changelog.
5. The Rust `Auto Release` job depended on jobs that themselves handled skipped prerequisites. GitHub Actions skips downstream jobs when needed jobs are skipped unless the downstream job uses a continuing condition such as `always()`.
6. The C# release flow built and uploaded a NuGet artifact before versioning. The release job then versioned the project and reused the old package, which could publish a stale package if it reached that step.
7. `.github/workflows/ci.yml` duplicated C# CI coverage already owned by `.github/workflows/csharp.yml`, increasing drift without adding release coverage.

## External References

- GitHub Actions workflow syntax documents that skipped or failed `needs` jobs skip downstream jobs unless the downstream job uses a condition that continues, and it specifically points to `always()` for this pattern: https://docs.github.com/en/actions/writing-workflows/workflow-syntax-for-github-actions#jobsjob_idneeds
- GitHub's release API requires a `tag_name` and uses that value as the release tag. Component releases therefore need component-specific tag names such as `csharp-v2.4.0` and `rust-v2.4.0`: https://docs.github.com/en/rest/releases/releases#create-a-release

No upstream product bug was found. The failures came from local template adaptation drift, so no external issue was filed.

## Template Comparison

The Rust template already used `always() && !cancelled()` around downstream jobs and release jobs. The local Rust workflow had lost that release-job guard, which explains why `Auto Release` skipped after successful lint/test/build.

The C# template built the release package after the versioning step. The local C# workflow uploaded a package from the pre-version build and downloaded that artifact in the release job. The corrected workflow rebuilds and packs after the release commit so the NuGet package version matches the tag and changelog.

The template scripts expected component-local paths; the local JavaScript helpers had not been adapted from template defaults. The corrected scripts now accept explicit component paths and package metadata.

## Fix Summary

- `scripts/get-bump-type.mjs` now honors `--dir` and avoids network-loaded argument parsing.
- `scripts/merge-changesets.mjs` now honors `--dir` and `--package-name`.
- `scripts/create-github-release.mjs` now honors `--tag-prefix`, `--changelog-path`, `--language`, and `--package-id`, and has a dry-run mode for tests.
- `scripts/collect-changelog.mjs` and `scripts/validate-changeset.mjs` now support component paths instead of root-only defaults.
- `.github/workflows/csharp.yml` now rebuilds the NuGet package after versioning and creates a `csharp-v<version>` release using `csharp/CHANGELOG.md`.
- `.github/workflows/rust.yml` now applies explicit `always() && !cancelled()` release gating and creates `rust-v<version>` releases using `rust/CHANGELOG.md`.
- `.github/workflows/ci.yml` was removed because C# CI/CD is consolidated in `csharp.yml`.
- `.github/workflows/wasm.yml` remains separate because it owns WebAssembly and GitHub Pages deployment, not C# or Rust package release.

## Regression Coverage

`scripts/release-scripts.test.mjs` reproduces and verifies the failed paths:

- Rust bump detection reads fragments from the requested component directory.
- C# changeset merging reads the requested component directory and real package name.
- GitHub release payload generation uses component tag prefixes and component changelogs.
- Rust changelog collection works from repository-root component paths.

Run:

```bash
node --test scripts/*.test.mjs
```
