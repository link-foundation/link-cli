---
bump: patch
---

Update rusqlite to 0.42.0, ureq to 3.5.2, errno to 0.3.15 and rustls-webpki to 0.103.17 so the Dependencies check passes; cargo-audit stays held back at 0.22.2 until taiki-e/install-action ships a verified 0.23.0 manifest (issue #176).

Remove `package.homepage`, which only repeated `package.repository` and now trips Cargo's `redundant_homepage` lint under `-Dwarnings`.

In the WebAssembly wrapper, make `console_error_panic_hook` a wasm32-only dependency (it is used only there) and drop the unused `web-sys`, which Rust 1.100.0's `cargo::unused_dependencies` lint rejects.
