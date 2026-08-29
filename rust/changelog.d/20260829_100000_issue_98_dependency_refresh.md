---
bump: patch
---

Refreshed every Rust dependency to its latest release (issue #98):
`doublets` 0.3.0 -> 0.4.0, `links-notation` 0.13.0 -> 0.16.1,
`thiserror` 1.0 -> 2.0.20 and `anyhow` -> 1.0.104 in the CLI workspace,
plus `wasm-bindgen` 0.2.127, `serde` 1.0.229, `serde_json` 1.0.151,
`web-sys` 0.3.104 and `wasm-bindgen-test` 0.3.77 in the WASM workspace.
The `doublets` bump matters beyond hygiene: an external crate that
already depends on `doublets` 0.4 can now link against `link-cli`
without pulling two semver-incompatible copies of the same store into
one binary.
