---
bump: patch
---

Hardened the Rust pipeline (issue #96). Every job now declares a
timeout and a least-privilege token, release jobs run in a
non-cancellable writer concurrency group, the advertised
`changelog-pr` release mode is implemented, and the lint job fails on
`Cargo.lock` drift. `anyhow` and `memmap2` were refreshed in both
lockfiles to clear the outstanding RUSTSEC advisories.
`cargo clippy` now runs with `-D warnings` and also covers the `rust/wasm`
workspace, which had never been formatted or linted, and pull requests
re-run the fast checks on a simulated merge with the tip of `main` so a
semantic merge conflict fails the pull request instead of `main`. The
pattern-matching helpers moved from `query_processor.rs` into
`query_processor/matching.rs` to clear the file-size warning; no public API
changed.
