---
bump: patch
---

Hardened the Rust pipeline (issue #96). Every job now declares a
timeout and a least-privilege token, release jobs run in a
non-cancellable writer concurrency group, the advertised
`changelog-pr` release mode is implemented, and the lint job fails on
`Cargo.lock` drift. `anyhow` and `memmap2` were refreshed in both
lockfiles to clear the outstanding RUSTSEC advisories.
