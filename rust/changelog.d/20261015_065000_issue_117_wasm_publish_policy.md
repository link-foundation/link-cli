---
bump: patch
---

Mark the internal `clink-wasm` browser wrapper as `publish = false` so registry scans no longer treat it as an unpublished crates.io package. Its wasm-pack app build and GitHub Pages deployment remain available.
