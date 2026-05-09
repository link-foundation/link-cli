# Issue 73 Case Study: Remove `wee_alloc` Dependabot Alert

Issue: https://github.com/link-foundation/link-cli/issues/73
Pull request: https://github.com/link-foundation/link-cli/pull/74
Dependabot alert: https://github.com/link-foundation/link-cli/security/dependabot/1

## Evidence Collected

- `evidence/issue-73.json`: original issue details.
- `evidence/issue-73-comments.json`: issue comments. The issue had no
  comments when this case study was prepared.
- `evidence/pr-74.json`, `evidence/pr-74-conversation-comments.json`,
  `evidence/pr-74-review-comments.json`, and `evidence/pr-74-reviews.json`:
  PR state and review surfaces before implementation.
- `evidence/recent-merged-prs.json`: recent merged PRs used as PR description
  style reference.
- `evidence/dependabot-alert-1.json` and
  `evidence/dependabot-alerts.json`: repository Dependabot alert data.
- `evidence/github-advisory-ghsa-rc23-xxgq-x27g.json`: GitHub Advisory
  Database data for `GHSA-rc23-xxgq-x27g`.
- `evidence/rustsec-2022-0054.md`: RustSec advisory source for
  `RUSTSEC-2022-0054`.
- `evidence/rustwasm-wee-alloc-issue-107.json` and
  `evidence/rustwasm-wee-alloc-issue-107-comments.json`: upstream maintenance
  discussion referenced by the advisory.
- `evidence/code-search-wee-alloc.json`: organization code search results for
  `wee_alloc`.
- `evidence/local-wee-alloc-references-before.txt` and
  `evidence/local-wee-alloc-references-after.txt`: local production references
  before and after the fix.
- `evidence/cargo-tree-wee-alloc-before.txt`: all-features dependency path from
  `clink-wasm` to `wee_alloc` before the fix.
- `evidence/cargo-tree-all-features-after.txt` and
  `evidence/cargo-metadata-all-features-after.json`: resolved dependency graph
  after the fix.
- `evidence/dependabot-regression-test-before.log` and
  `evidence/dependabot-regression-test-after.log`: failing and passing
  regression test runs.
- `evidence/cargo-fmt-root.log`, `evidence/cargo-clippy-root.log`,
  `evidence/cargo-test-root-all-features.log`, `evidence/cargo-test-lib-after.log`,
  `evidence/cargo-test-rust-core.log`, and
  `evidence/check-file-size-rust.log`: Rust verification logs.
- `evidence/cargo-install-wasm-pack.log`, `evidence/npm-ci.log`,
  `evidence/npm-run-test-wasm.log`, `evidence/npm-run-build.log`, and
  `evidence/npm-test.log`: WebAssembly and web build verification logs.
- `evidence/npm-audit-before.json` and `evidence/npm-audit-after.json`: npm
  audit context confirming the alert is not from the Node dependency graph.

## Timeline

- 2022-05-11: Upstream issue rustwasm/wee_alloc#107 asked whether the crate was
  still maintained and pointed to unresolved memory leak concerns.
- 2022-09-08: RustSec issued `RUSTSEC-2022-0054` for unmaintained `wee_alloc`.
- 2022-09-16: GitHub published `GHSA-rc23-xxgq-x27g` for `wee_alloc`.
- 2025-08-25: The `rustwasm/wee_alloc` repository was archived and became
  read-only.
- 2026-05-02: Dependabot opened repository alert 1 for `wee_alloc` in
  `Cargo.lock`.
- 2026-05-09: Issue 73 requested a full case study and a single PR solution.

## Requirements

- Download issue, PR, Dependabot, advisory, upstream, and local dependency data
  into `docs/case-studies/issue-73`.
- Search for additional online facts and data.
- Reconstruct the timeline and list all requirements.
- Identify the root cause of each problem.
- Propose possible solutions and a solution plan.
- Add debug output or verbose mode if the root cause cannot be found.
- If another repository needs an issue, report it with a reproduction,
  workaround, and code-level suggestion.
- Fix the bug with a reproducing automated test.
- Keep the work in PR 74 on branch `issue-73-d71d2656d381`.

## Root Cause

Dependabot alert 1 was caused by the root WebAssembly crate declaring:

```toml
wee_alloc = { version = "0.4.5", optional = true }
```

The wrapper then installed it as a global allocator when the implicit
`wee_alloc` feature was enabled:

```rust
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;
```

That dependency was resolved in `Cargo.lock`, so Dependabot reported
`GHSA-rc23-xxgq-x27g` against `Cargo.lock`. The alert has no patched version:
the affected range is `>= 0`.

## External Facts

- RustSec states that `wee_alloc` is unmaintained, has open memory-leak issues,
  and recommends switching to Rust's standard default allocator for wasm32
  targets.
- GitHub Advisory Database marks all `wee_alloc` versions as affected and lists
  no patched version.
- The upstream `rustwasm/wee_alloc` repository was archived on 2025-08-25, so a
  new upstream issue is not actionable. The relevant upstream maintenance issue
  already exists as rustwasm/wee_alloc#107.
- The former project-level benefit of `wee_alloc` was reduced wasm binary size.
  That tradeoff does not outweigh an unpatched critical Dependabot alert for
  this repository.

## Possible Solutions

1. Dismiss the alert.
   This would leave an unmaintained crate with no patched version in the lockfile
   and does not satisfy the issue.
2. Replace `wee_alloc` with another custom wasm allocator.
   This would avoid this specific advisory but adds allocator-specific risk and
   is unnecessary because this project does not require a custom allocator for
   correctness.
3. Remove `wee_alloc` and use Rust's default allocator.
   This follows the advisory guidance, removes the vulnerable crate from
   `Cargo.lock`, and keeps the WebAssembly wrapper behavior simple.

## Solution Applied

- Added `tests/dependabot_alert_tests.rs` to reproduce alert 1 by asserting
  that production WebAssembly dependency surfaces do not reference `wee_alloc`.
- Removed the optional `wee_alloc` dependency from the root `Cargo.toml`.
- Removed the conditional `#[global_allocator]` block from `src/lib.rs`.
- Regenerated `Cargo.lock` so `wee_alloc`, `memory_units`, and its private
  `winapi` transitive dependencies are no longer present.
- Kept `console_error_panic_hook` unchanged because it is independent of the
  allocator and still useful for wasm panic diagnostics.

## Verification

- Before the fix, `cargo test --test dependabot_alert_tests` failed because
  `Cargo.toml` referenced `wee_alloc`.
- After the fix, `cargo test --test dependabot_alert_tests` passed.
- `cargo metadata --locked --all-features --format-version 1` succeeds after
  the lockfile update.
- `rg` found no production references to `wee_alloc` or `global_allocator` in
  `Cargo.toml`, `Cargo.lock`, `src`, and WebAssembly docs after the fix.
- `npm audit --json` reported zero Node vulnerabilities before and after the
  fix, confirming the alert was limited to the Rust lockfile.
- `cargo fmt --all -- --check` passed.
- `cargo clippy --all-targets --all-features` passed.
- `cargo test --all-features` passed for the root WebAssembly crate.
- `cargo test --manifest-path rust/Cargo.toml --all-features` passed for the
  Rust CLI core.
- `node scripts/check-file-size.mjs --lang rust` passed.
- `npm ci` installed the lockfile dependencies needed by the clean workspace.
- `npm run test:wasm`, `npm run build`, and `npm test` passed after installing
  the workflow-pinned `wasm-pack 0.14.0`.
