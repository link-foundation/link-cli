# Issue 86 Case Study: NuGet Indexing Delay Broke C# Release Verification

Issue: https://github.com/link-foundation/link-cli/issues/86

Prepared PR: https://github.com/link-foundation/link-cli/pull/87

Failed run: https://github.com/link-foundation/link-cli/actions/runs/25760911270

Upstream template report: https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/issues/13

## Requirements

Restated from issue #86:

1. Preserve the issue, PR, run metadata, and CI logs under `docs/case-studies/issue-86`.
2. Reconstruct the failed release timeline and identify the root cause.
3. Search official/public sources for NuGet indexing behavior.
4. Compare local CI/CD files with the JavaScript, Rust, Python, and C# templates.
5. Reuse template best practices where they address the same class of failure.
6. Report the same issue upstream if it exists in a template repository.
7. Add regression coverage and fix the C# workflow so package availability is checked every 2 minutes across the normal NuGet indexing window.

## Timeline

- `2026-05-12T20:40:11Z`: C# CI/CD run `25760911270` started on `main` at `0c8092c`.
- `2026-05-12T21:20:40Z`: The Release job entered `Publish to NuGet`.
- `2026-05-12T21:20:41Z`: `dotnet nuget push` completed and the workflow moved to `Verify package on NuGet`.
- `2026-05-12T21:20:41Z` through `2026-05-12T21:22:47Z`: The verifier checked `clink@2.4.0` at delay schedule `0, 5, 10, 20, 30, 60` seconds. Every flat-container nuspec check returned HTTP 404. Evidence: `logs/csharp-run-25760911270-verify-nuget-excerpt.log`.
- `2026-05-12T21:22:47Z`: The Release job failed with `clink@2.4.0 was not available from NuGet after publish`.
- `2026-05-12T21:24:36Z`: NuGet flat-container later showed `clink@2.4.0` as available by the blob `last-modified` header. Evidence: `github-data/nuget-clink-2.4.0.headers.txt`.
- `2026-05-12T21:29:51Z`: The investigation confirmed `https://api.nuget.org/v3-flatcontainer/clink/2.4.0/clink.nuspec` returned HTTP 200 and the package index included `2.4.0`. Evidence: `github-data/nuget-clink-index.json` and `github-data/wait-for-nuget-clink-2.4.0.log`.
- At investigation time, the GitHub release `csharp-v2.4.0` was still missing because `Create GitHub Release` was skipped after the NuGet verification failure. Evidence: `github-data/csharp-v2.4.0-release.json`.

## Evidence

- Issue and PR data: `github-data/issue-86.json`, `github-data/issue-86-comments.json`, `github-data/pr-87.json`, `github-data/pr-87-comments.json`, `github-data/pr-87-review-comments.json`, `github-data/pr-87-reviews.json`.
- Run data: `github-data/csharp-run-25760911270.json`, `github-data/csharp-run-25760911270-jobs.json`, `github-data/csharp-run-25760911270-attempt-1.json`.
- Logs: `logs/csharp-run-25760911270.log`, with focused excerpts in `logs/csharp-run-25760911270-publish-excerpt.log` and `logs/csharp-run-25760911270-verify-nuget-excerpt.log`.
- NuGet state: `github-data/nuget-clink-index.json`, `github-data/nuget-clink-index.headers.txt`, `github-data/nuget-clink-2.4.0.nuspec`, `github-data/nuget-clink-2.4.0.headers.txt`.
- Template data: `templates/*/tree.json`, `templates/*/file-tree.txt`, and fetched release workflow/scripts for JS, Rust, Python, and C# templates.
- Validation logs: `github-data/node-release-scripts-test.log`, `github-data/wait-for-nuget-clink-2.4.0.log`.

## Online Facts

- Microsoft documents that NuGet packages pushed to nuget.org go through validation and indexing, and that validation/indexing usually take less than 15 minutes. Source: https://learn.microsoft.com/en-us/nuget/nuget-org/publish-a-package#package-validation-and-indexing
- Microsoft documents the NuGet flat-container `PackageBaseAddress` resource. The versions index lists versions available for package-content API calls, and a package nuspec endpoint returns 200 only when the package exists on the source, otherwise 404. Source: https://learn.microsoft.com/en-us/nuget/api/package-base-address-resource
- `dotnet nuget push --skip-duplicate` only treats HTTP 409 conflicts as warnings; it does not wait for package indexing. Source: https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-nuget-push

## Root Cause

The release workflow treated NuGet publish success as if the package would be immediately installable. In this run, NuGet accepted the push, but flat-container availability lagged beyond the workflow's hard-coded retry window.

The old loop checked at 0, 5, 15, 35, 65, and 125 seconds after publish. NuGet made `clink@2.4.0` available around `2026-05-12T21:24:36Z`, about 3 minutes and 55 seconds after the publish step and about 1 minute and 49 seconds after the workflow had already failed.

This is not a package-build failure and not a NuGet API key failure. It is a release-orchestration bug: the pipeline did not wait long enough for the registry state that the next step depends on.

## Template Comparison

- Local C# workflow and the C# template both used the same short inline NuGet verification loop. The same issue was reported upstream in `link-foundation/csharp-ai-driven-development-pipeline-template#13`.
- The JS template has a dedicated `scripts/wait-for-npm.mjs` script and invokes it before Docker publishing. It defaults to 30 attempts with 10-second intervals and has testable logic.
- The Rust template has a dedicated `scripts/wait-for-crate.rs` script and invokes it once `should_release == 'true'`, regardless of whether the current attempt performed the publish. It also centralizes registry availability logic outside YAML.
- The Python template does not contain NuGet behavior and has no directly matching defect. It publishes to PyPI through `pypa/gh-action-pypi-publish`, so it was recorded for comparison but not reported as this NuGet-specific issue.

The reusable-script pattern from the JS and Rust templates is the best fit here: YAML should orchestrate, while package availability polling belongs in a tested script.

## Implemented Solution

Added `csharp/scripts/wait-for-nuget.mjs`.

Behavior:

- Checks `https://api.nuget.org/v3-flatcontainer/{lower-id}/{lower-version}/{lower-id}.nuspec`.
- Uses `HEAD` to avoid downloading package metadata when only availability is needed.
- Defaults to 8 attempts with 120 seconds between attempts. This checks immediately, then at roughly 2, 4, 6, 8, 10, 12, and 14 minutes after the first check.
- Writes `nuget_available=true|false` to `GITHUB_OUTPUT`.
- Supports override arguments and environment variables for tests and future tuning.

Updated `.github/workflows/csharp.yml`:

- Automatic release `Verify package on NuGet` now calls `node scripts/wait-for-nuget.mjs`.
- Manual instant release `Verify package on NuGet` now calls the same script.

Added regression coverage in `csharp/scripts/release-scripts.test.mjs`:

- Defaults are 8 attempts and 120-second sleeps.
- Flat-container nuspec URLs are normalized to lowercase package IDs.
- A package that becomes available only on the 8th attempt succeeds, covering the issue scenario where the old 125-second loop would fail.
- Exhausting all attempts returns failure.

## Upstream Report

The C# pipeline template contains the same short NuGet verification loop, so it was reported upstream:

- https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/issues/13

The report includes the failing link-cli run, the old delay schedule, the observed later NuGet availability, a manual workaround, and the suggested tested-script fix.

## Validation

- `node --test csharp/scripts/*.test.mjs`: 16 tests pass. Log: `github-data/node-release-scripts-test.log`.
- `node --check csharp/scripts/wait-for-nuget.mjs`: passes. Log: `github-data/node-check-wait-for-nuget.log`.
- `dotnet test csharp/Foundation.Data.Doublets.Cli.sln --configuration Release`: 187 tests pass. Log: `github-data/dotnet-test-release.log`.
- `git diff --check`: passes. Log: `github-data/git-diff-check.log`.
- `node csharp/scripts/wait-for-nuget.mjs --package-id clink --release-version 2.4.0 --max-attempts 1`: confirms the package is now available from NuGet. Log: `github-data/wait-for-nuget-clink-2.4.0.log`.
