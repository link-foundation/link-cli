# Issue 82 Case Study: C# and Rust Releases Did Not Happen

Issue: https://github.com/link-foundation/link-cli/issues/82

Prepared PR: https://github.com/link-foundation/link-cli/pull/83

## Requirements

- Investigate why commit `d47e551d2d66c7bfe93f9a869ce9fab36347e317` did not trigger Rust CI/CD.
- Investigate why the C# workflow skipped the version bump and release.
- Download issue details, comments, run metadata, and logs into `docs/case-studies/issue-82`.
- Compare this repository against the JavaScript, Rust, and C# pipeline templates.
- Use relevant template best practices in the fix.
- Search for online facts that explain the observed behavior.
- Report issues to related template repositories when the same defect exists there.
- Add regression coverage and implement the fix in one pull request.

## Timeline

- 2026-05-12T16:57:31Z: merge commit `d47e551d2d66c7bfe93f9a869ce9fab36347e317` landed on `main`.
- 2026-05-12T16:57:36Z: GitHub Actions created two runs for that SHA: `C# CI/CD Pipeline` run `25749404027` and `WebAssembly CI` run `25749403949`. `runs-for-d47e551.json` contains no Rust workflow run for this SHA.
- 2026-05-12T17:00:52Z: the C# release job found 12 changesets, merged them to one minor changeset, then ran `version-and-commit.mjs`.
- 2026-05-12T17:00:52Z: the C# version script printed `Tag csharp-v2.4.0 already exists`, set `already_released=true`, and exited without a bump, commit, tag, package publish, or GitHub release.
- 2026-05-12T17:01:42Z: WebAssembly Pages deployment succeeded at `https://link-foundation.github.io/link-cli/`.

## Evidence

- Issue and PR data: `issue-82.json`, `issue-82-comments.json`, `pr-83*.json`.
- CI run lists: `recent-runs.json`, `runs-for-d47e551.json`.
- Downloaded logs: `run-25749404027-csharp.log`, `run-25749403949-wasm.log`.
- Commit diff evidence: `d47e551-changed-files.txt`, `d47e551-diff-stat.txt`.
- Release state: `recent-releases.txt`.
- Template metadata: `template-{js,rust,csharp}-file-tree.json` and `template-{js,rust,csharp}-workflows.json`.
- Regression and validation logs: `test-logs/regression-before-fix.log`, `test-logs/regression-after-action-updates.log`, `test-logs/npm-test-js.log`, `test-logs/dotnet-test.log`, `test-logs/cargo-test.log`, `test-logs/cargo-clippy.log`, `test-logs/npm-build-after-ci.log`.
- Verified action tags: `verified-action-tags.txt`.

Important log anchors:

- `run-25749404027-csharp.log:5763` shows 12 C# changesets.
- `run-25749404027-csharp.log:5793` to `5798` shows the merge selected a minor bump.
- `run-25749404027-csharp.log:5855` to `5860` shows the false `already_released=true` result.
- `run-25749403949-wasm.log:1984` to `1988` shows Pages deployment succeeded.

## Online Facts

GitHub's workflow trigger documentation states that when `branches` and `paths` filters are both configured, both filters must match for a workflow to run. It also states that path filters are evaluated against changed files. Source: https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow

The downloaded GitHub Actions logs also reported Node 20 action deprecation warnings and pointed to GitHub's September 2025 changelog. The local fix updates official action major versions that were already used by the templates or verified by tag lookup.

## Template Comparison

The JavaScript, Rust, and C# release templates do not use `push.paths` filters for the main release workflow. They schedule release workflows on every push to `main`, then use internal change detection and release-needed scripts to decide what work to perform. The template workflow metadata in this case study links to the public upstream files instead of copying template code into this repository.

The local Rust and C# workflows did use `push.paths`. That is the direct reason Rust did not run for `d47e551`: the merge commit changed C# workflow/script files, WebAssembly workflow files, JavaScript tests, and documentation, but no `rust/**` file. The Rust workflow never reached its own release-needed logic.

The C# template has the same silent-command defect in `scripts/version-and-commit.mjs`: its `exec(command, true)` wrapper swallows failures, while `checkTagExists()` expects a thrown error for a missing tag. This was reported upstream as https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/issues/9.

The C# template also verifies NuGet package availability through the NuGet flat-container API after publish. The local C# workflow now does the same before creating the GitHub release.

The JavaScript and Rust templates use newer official action majors for Node 24 compatibility. Local workflows were updated from the deprecated action majors where verified tags exist.

## Root Causes

1. Rust release workflow was blocked by a main-push path filter.

   The workflow was configured with `push.branches: main` and `push.paths` limited to Rust files. Because `d47e551` did not change a Rust path, GitHub did not schedule Rust CI/CD at all.

2. C# tag detection always treated missing tags as existing.

   `version-and-commit.mjs` used a silent `exec()` helper that returned an empty string on command failure. `checkTagExists()` wrapped `git rev-parse csharp-v2.4.0` in `try/catch`, but no exception was thrown when the tag was missing. The script exited with `already_released=true`.

3. C# release verification was too weak.

   The workflow published to NuGet and then immediately created the GitHub release without checking that the package had become available. Template comparison showed an existing NuGet flat-container verification pattern.

4. CI workflow actions were approaching a platform break.

   Downloaded logs warned that several `@v4` official JavaScript actions used Node 20, which GitHub will force off the runner path in 2026. The templates already had newer action majors for the JavaScript and Rust workflows.

## Implemented Solution

- Removed `push.paths` filters from `.github/workflows/csharp.yml` and `.github/workflows/rust.yml` so release workflows are scheduled on every push to `main`.
- Kept pull request path filters for the monorepo workflows to avoid unrelated PR noise; release safety is now handled by the main-push schedule plus internal checks.
- Fixed `csharp/scripts/version-and-commit.mjs` so silent commands still throw on failure.
- Changed C# tag detection to verify the exact tag ref with `git rev-parse --verify --quiet refs/tags/csharp-vX.Y.Z`.
- Added a C# regression test that creates a temporary git repository, runs `version-and-commit.mjs`, and asserts the version commit and `csharp-v2.4.0` tag are created when the tag is missing.
- Added repository-layout regression coverage that ensures C# and Rust release workflows are not path-filtered on pushes to `main`.
- Added NuGet package availability verification to C# automatic and manual release jobs.
- Updated local workflow action majors to avoid the logged Node 20 deprecation path.
- Removed the root `.gitkeep` artifact because the existing repository-layout test explicitly requires generated evidence and package artifacts to stay out of the repository root.

## Validation

- Focused before-fix regression: `test-logs/regression-before-fix.log` captured the C# false already-released bug and the C# push path-filter regression.
- Focused after-fix regression: `test-logs/regression-after-action-updates.log` passes all focused tests.
- `npm --prefix js run test:js` passed 12 Node tests.
- `dotnet test csharp/Foundation.Data.Doublets.Cli.sln --configuration Release` passed 187 .NET tests.
- `cargo test --manifest-path rust/Cargo.toml --all-features` passed the Rust test suite and doc tests.
- `cargo fmt --manifest-path rust/Cargo.toml --all -- --check` passed.
- `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features` passed.
- `git diff --check` passed.
- `npm --prefix js ci` installed local JavaScript dependencies with 0 vulnerabilities.
- `npm --prefix js run build` passed after `npm ci`, covering the WebAssembly package build and Vite production build.

## Remaining Watch Items

- After PR 83 merges, the next push to `main` should schedule Rust and C# CI/CD even if the merge contains only documentation or workflow changes.
- The C# release should bump from `2.3.0` to `2.4.0`, create tag `csharp-v2.4.0`, publish package `clink` to NuGet when `NUGET_API_KEY` is available, verify NuGet availability, and create the GitHub release.
- The Rust workflow should reach its existing `check-release-needed.rs` logic and decide whether to release based on the pending Rust changelog fragments and external release state.
