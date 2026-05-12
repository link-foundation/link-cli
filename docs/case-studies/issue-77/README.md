# Issue 77 Case Study

Issue: <https://github.com/link-foundation/link-cli/issues/77>

PR: <https://github.com/link-foundation/link-cli/pull/78>

## Problem

The repository still had shared root-level release scripts and a root-level
WebAssembly Rust crate. That made package ownership ambiguous:

- C# and Rust release helpers were mixed under `scripts/`.
- The WebAssembly wrapper crate lived at `Cargo.toml`, `src/`, and `tests/` in
  the repository root instead of under a language package tree.
- Package READMEs and registry/release badges were not split by language.
- Older CI history included a GitHub Pages deploy failure and a C# release
  failure, so the current workflow state needed to be verified before changing
  the layout.

## Evidence

Captured evidence is stored in `evidence/`.

- `issue-77.json`, `issue-77-comments.json`, and PR 78 comment/review exports
  preserve the GitHub discussion state.
- `template-*-file-tree.txt` and `template-*-release.yml` preserve the C#,
  Rust, and JS template comparisons used for the split.
- `recent-runs-*.json` and `run-*.json` preserve CI state. The issue branch had
  no runs at the start of the investigation. The latest main C#, Rust, and
  WebAssembly runs from 2026-05-12 were successful.
- `webassembly-25245349330-log.txt` preserves the stale Pages deploy failure. It
  failed in `actions/configure-pages@v5` because Pages was not configured for
  GitHub Actions deployment at that time.
- `csharp-25594941803-log.txt` preserves the stale C# failure. It included Windows
  test failures and a missing `.changeset` release error.
- `link-cli-file-tree-before.txt` and `link-cli-file-tree-after.txt` show the
  layout before and after the fix.

## Fix

- Moved the WebAssembly wrapper crate to `rust/wasm/`, including its
  `Cargo.toml`, `Cargo.lock`, `src/`, and `tests/`.
- Moved C# release helpers to `csharp/scripts/`.
- Replaced Rust release helpers with Rust `rust-script` scripts under
  `rust/scripts/`, including crates.io release checks and publication helpers.
- Updated C#, Rust, and WebAssembly workflows to use the new paths.
- Added `csharp/README.md` and `rust/README.md` with package badges and install
  instructions.
- Updated the root README and architecture docs with workflow, NuGet, crates.io,
  and GitHub release badges.
- Kept WebAssembly Pages deployment manual-only on `main` with the existing
  `workflow_dispatch` and `deploy_pages` gate, so normal branch CI no longer
  attempts Pages deployment.

## Regression Test

`web/test/repositoryLayout.test.mjs` reproduces the original layout problem by
asserting that root `Cargo.toml`, `Cargo.lock`, `scripts/`, `src/`, and `tests/`
do not exist, while `csharp/scripts/`, `rust/scripts/`, and `rust/wasm/` do.

The test failed before the layout move because root `Cargo.toml` and the old
WASM package scripts still existed.
