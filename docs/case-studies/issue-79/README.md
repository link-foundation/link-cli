# Issue 79 Case Study

Issue: <https://github.com/link-foundation/link-cli/issues/79>

PR: <https://github.com/link-foundation/link-cli/pull/80>

## Problem

The repository root still contained JavaScript package files, WebAssembly
documentation, a root `ci-logs/` folder, and `.gitkeep`. That conflicted with
the current multi-language layout:

- C# implementation and release helpers live under `csharp/`.
- Rust implementation, release helpers, and the WebAssembly wrapper crate live
  under `rust/`.
- The browser app should own its JavaScript package files under `js/`.
- Investigation logs and screenshots should live under issue-specific case
  studies, not root folders.

## Requirements

- Move root `package.json` and `package-lock.json` into the browser package.
- Move `web/` to `js/`.
- Merge root `README-WASM.md` and `WEBASSEMBLY_IMPLEMENTATION.md` into the
  JavaScript package README.
- Move root `ci-logs/` into `docs/case-studies/issue-79/evidence/`.
- Move `docs/screenshots/issue-12` into the issue 12 case study.
- Remove `.gitkeep`.
- Verify GitHub Pages and NuGet release coverage.
- Compare CI/CD layout with the referenced JS, Rust, and C# templates.
- Preserve issue, PR, CI, template, and local verification evidence.

## Evidence

Captured evidence is stored in `evidence/`.

- `issue-79.json`, `issue-79-comments.json`, and PR 80 exports preserve the
  GitHub discussion state.
- `link-cli-file-tree-before.txt` and `link-cli-file-tree-after.txt` record the
  layout before and after the cleanup.
- `template-*-file-tree.json`, `template-*-workflows.json`, and
  `template-*-release.yml` preserve the referenced template comparison.
- `recent-runs-issue-branch-before.json` showed no issue branch runs before this
  implementation.
- `recent-runs-main-before.json` showed latest `main` C#, Rust, and WebAssembly
  runs from 2026-05-12 were successful.
- `run-25594941803.json` and `csharp-25594941803-log.txt` preserve the stale
  2026-05-09 C# failure visible in recent history. The preserved log shows
  Windows temp-file locking failures around lines 2695-5218 and an older release
  script path failure around lines 5726-5740.
- `npm-doublets-web-version.json` showed npm reported `doublets-web@0.1.3` on
  2026-05-12; the committed lockfile remains pinned by `js/package-lock.json`.
- `npm-link-cli-web-version.json` showed `link-cli-web` is not published as an
  npm package, matching the package's `private: true` setting.
- `repository-layout-test-before.txt` captures the failing regression test before
  the move.
- `repository-layout-test-after.txt`, `npm-ci-js.txt`, `npm-test-js.txt`,
  `npm-test-wasm-js.txt`, `npm-build-js.txt`, `cargo-test-rust-core.txt`, and
  `dotnet-test-csharp.txt` capture local verification after the fix.

## Template Comparison

The JS template is a JavaScript-only repository, so root `package.json`,
root `package-lock.json`, and root `scripts/` are expected there. Its
`example-app.yml` still provided a useful pattern for subdirectory JavaScript
apps: `setup-node` uses `cache-dependency-path` for a nested lockfile and npm
commands target the package directory.

The Rust and C# templates are single-language templates, so root `Cargo.toml` or
root release scripts are expected there. In this repository, the same helper
families already live under `rust/scripts/` and `csharp/scripts/` because the
repository is multi-language.

No upstream template issue was filed. The root files found in the templates are
appropriate for their single-language or JS-only template scopes, while the
local problem was specific to this repository's mixed C#, Rust, WebAssembly, and
React layout.

## External CI Facts Checked

- `actions/setup-node` documents that dependency caching looks for lockfiles in
  the repository root by default and uses `cache-dependency-path` for lockfiles
  in subdirectories: <https://github.com/actions/setup-node>
- GitHub Pages custom workflow docs require the deploy job to have `pages: write`
  and `id-token: write`, an environment, and a dependency on the build job:
  <https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages>

## Root Causes

The browser package had outgrown the root folder. Keeping `package.json` and the
lockfile at root made the repository look like a JavaScript-only project and
made CI cache behavior rely on root lockfile defaults.

The WebAssembly docs were split into two root files, so browser implementation
details were disconnected from the package that owns the browser app.

Root `ci-logs/` and `docs/screenshots/` duplicated the repository's newer case
study evidence convention. Those files belonged with the issue-specific
investigation records.

The perceived missing GitHub Pages and NuGet deploys came from stale context.
The current workflows already include manual GitHub Pages deployment in
`.github/workflows/wasm.yml` and NuGet publishing in
`.github/workflows/csharp.yml`. The latest `main` runs from 2026-05-12 were
successful before this cleanup.

## Fix

- Moved `web/` to `js/`.
- Moved `package.json` and `package-lock.json` to `js/`.
- Updated npm scripts to run from `js/`, output generated Rust WASM to
  `js/pkg/`, and keep shared C# script tests referenced through `../csharp/`.
- Updated `.github/workflows/wasm.yml` path filters, npm working directories,
  and `setup-node` cache dependency path for `js/package-lock.json`.
- Merged root WebAssembly docs into `js/README.md` and removed the old root doc
  files.
- Moved root `ci-logs/rust-lint-75003226910.log` into issue 79 evidence.
- Moved issue 12 screenshots into `docs/case-studies/issue-12/screenshots/`.
- Removed `.gitkeep`.
- Updated README, architecture, requirements, case-study, and ignore-file paths.
- Extended `js/test/repositoryLayout.test.mjs` to guard against root package
  files, root WASM docs, root `ci-logs/`, root `web/`, and stale WASM workflow
  path filters.

## Verification

The new regression test failed before the move because `.gitkeep`, root package
files, missing `js/package.json`, and stale `web/**` workflow paths were still
present. After the cleanup, the same checks passed.

Local checks after the fix:

- `node --test js/test/repositoryLayout.test.mjs`
- `npm ci --prefix js`
- `npm --prefix js run test:js`
- `npm --prefix js run test:wasm`
- `npm --prefix js run build`
- `cargo test --manifest-path rust/Cargo.toml --all-features`
- `dotnet test csharp/Foundation.Data.Doublets.Cli.sln`

## Follow-up (PR #81)

After PR 80 merged, @konard reported in
<https://github.com/link-foundation/link-cli/issues/79#issuecomment-4432584607>
that two requirements were still not met:

1. The post-merge `WebAssembly CI` run
   <https://github.com/link-foundation/link-cli/actions/runs/25747032579>
   had its `Deploy GitHub Pages` job skipped, so the live site was not
   refreshed.
2. `csharp.yml` and `rust.yml` were not triggered at all by the merge.

### Root causes found in this round

- **Pages deploy was opt-in.** `wasm.yml`'s `deploy` job had
  `if: github.event_name == 'workflow_dispatch' && ... && inputs.deploy_pages`,
  so a normal push to `main` could never deploy. The `js` template's
  `example-app.yml` already shows the right pattern: build the Pages artifact in
  the build job (with `actions/configure-pages` + `upload-pages-artifact`) and
  hand off to a dedicated `deploy-pages` job that only contains
  `actions/deploy-pages@v4`. Evidence: `template-js-example-app.yml`.
- **`csharp.yml` / `rust.yml` did not match the changed paths.** The PR 80
  merge commit (sha `9c93a27e`) only touched `.github/workflows/wasm.yml`,
  `.gitignore`, root README files, `docs/**`, and the moved `js/**` tree —
  nothing under `csharp/**` or `rust/**`. Both pipelines correctly skipped
  themselves. Evidence: `gh api repos/link-foundation/link-cli/commits/9c93a27e`
  shows zero matches for `^csharp/` or `^rust/` filenames.
  This is by design and does not need a code change in the workflow triggers,
  but the case study now records the analysis so the requirement is closed
  with evidence rather than a guess.
- **GitHub Releases never carried the NuGet artifact.**
  `csharp/scripts/create-github-release.mjs` only posted the release notes; the
  `.nupkg` produced by `dotnet pack` was uploaded to nuget.org but never
  attached to the GitHub Release. The same shortfall exists in the upstream
  `csharp-ai-driven-development-pipeline-template`.

### Fixes applied in PR 81

- Split `wasm.yml` into `test`, `build-pages`, and `deploy-pages` jobs.
  `build-pages` runs on every push to `main` (and on `workflow_dispatch`),
  configures Pages, and uploads the artifact. `deploy-pages` only runs
  `actions/deploy-pages@v4` with the required `pages: write` and
  `id-token: write` permissions and the `github-pages` environment.
  Removed the obsolete `deploy_pages` boolean input.
- Added a `--assets-glob` flag to `csharp/scripts/create-github-release.mjs`
  that resolves a `dir/*.ext` pattern and calls
  `gh release upload <tag> <files...> --clobber` after the release exists.
  Wired `--assets-glob "csharp/artifacts/*.nupkg"` into both the auto release
  and instant release jobs in `.github/workflows/csharp.yml`.
- Extended `js/test/repositoryLayout.test.mjs` with two regression tests:
  one asserting `wasm.yml` deploys Pages on push to `main`, and one asserting
  `csharp.yml` carries the asset glob in both release jobs.
- Added a unit test for the new asset glob in
  `csharp/scripts/release-scripts.test.mjs` that uses `--dry-run` so it does
  not contact GitHub.
- Removed the regenerated `.gitkeep` and the throwaway `ci-logs/` folder
  used to download the wasm CI log; the log is preserved as
  `evidence/wasm-25747032579.log`.

### Upstream report

Filed
<https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/issues/7>
covering the missing `.nupkg` upload in the C# template. The issue includes
the reproduction, the suggested fix, and a link back to PR 81 with the
working patch. Evidence:
`evidence/upstream-csharp-template-issue-7.json`.

### New evidence files

- `wasm-25747032579.log`, `run-25747032579.json`,
  `run-25747032579-jobs.json` — the post-merge WebAssembly CI run that
  triggered the followup.
- `issue-79-comments-followup.json` — captures the user's comment that
  reopened the work.
- `upstream-csharp-template-issue-7.json` — confirmation that the upstream
  issue was filed.

### Verification (PR 81)

- `node --test js/test/repositoryLayout.test.mjs`
- `node --test csharp/scripts/release-scripts.test.mjs`
