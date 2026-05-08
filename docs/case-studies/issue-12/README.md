# Issue 12 Case Study: WebAssembly link-cli

Issue: https://github.com/link-foundation/link-cli/issues/12
Pull request: https://github.com/link-foundation/link-cli/pull/52

## Evidence Collected

- `evidence/issue-12.json`: original issue details.
- `evidence/pr-52-conversation-comments.json`: PR comments, including the
  2026-04-30 request to merge current `main`, use GitHub Pages, support
  `doublets-web`, and build a React browser demo.
- `evidence/ci-runs.json`: recent branch workflow runs.
- `evidence/npm-doublets-web.json`: npm metadata showing `doublets-web@0.1.2`
  as the latest release.
- `evidence/doublets-web-issue-5.json`: dependency issue details from
  https://github.com/linksplatform/doublets-web/issues/5.

GitHub Actions log downloads for runs `17617650587` and `17617649689` returned
HTTP 410 on 2026-05-01, so the exact old log bodies were no longer available.
The run metadata still showed both failures were on branch SHA
`efe5e5984191ef2b4e1deb54ca083b363ef766ad` on 2025-09-10.

## Timeline

- 2025-09-10: PR branch CI failed in the old WebAssembly workflow on SHA
  `efe5e5984191ef2b4e1deb54ca083b363ef766ad`.
- 2026-04-30: PR feedback requested fresh `main`, a GitHub Pages browser demo,
  React, Rust WebAssembly, and latest `doublets-web` support.
- 2026-05-01: `doublets-web` issue 5 was closed; npm metadata reported latest
  `doublets-web` as `0.1.2`.
- 2026-05-01: This PR branch was merged with current `main` and conflict
  resolution kept the branch version bump while adopting the new `csharp/`
  layout.

## Requirements

- Run `link-cli` directly in a browser.
- Use the Rust implementation rather than a separate partial parser.
- Use `doublets-web` as the browser-facing WebAssembly Doublets package.
- Provide a React single-page demo.
- Publish the demo with GitHub Pages.
- Keep CI checks current and reproducible.
- Collect issue and dependency context in `docs/case-studies/issue-12`.

## Root Causes

- The previous branch duplicated a small subset of query behavior in root-level
  Rust files. After `main` added the full Rust CLI implementation under `rust/`,
  that duplication became stale.
- The old WebAssembly workflow used outdated artifact/deployment actions and
  published from the repository root rather than a static app artifact.
- Generated Rust build output was tracked in the PR, so local builds could dirty
  the working tree.
- Documentation still referenced the old `/demo/www/` page and an npm package
  shape that no longer matched the React/GitHub Pages requirement.

## Solution Plan Applied

- Merge current `main` into the PR branch.
- Replace the partial WASM query engine with a `wasm-bindgen` wrapper around the
  Rust `link-cli` `QueryProcessor`.
- Add an in-memory browser storage implementation of `NamedTypeLinks`.
- Build a React/Vite workbench that imports the Rust wrapper and
  `doublets-web@0.1.2`.
- Mirror each Rust query snapshot into a `doublets-web` `UnitedLinks` instance.
- Replace the old CI workflow with stable Rust, npm, `wasm-pack`, Vite build,
  artifact upload v4, and GitHub Pages deployment.
- Ignore generated `target/`, `web/pkg/`, `dist/`, and local investigation
  artifacts.

## Residual Risks

- Browser storage is session-local. IndexedDB persistence remains a follow-up.
- The app mirrors the Rust query result into `doublets-web`; the Rust query
  processor does not yet use a JS-hosted `UnitedLinks` object as its direct
  storage backend.
