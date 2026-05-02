# WebAssembly Implementation

Issue #12 asks for a browser-executable `link-cli` experience based on the Rust
implementation of Doublets. The current implementation uses the Rust `link-cli`
core from `rust/`, compiles a small wrapper crate with `wasm-pack`, and renders a
React single-page app for GitHub Pages.

## Architecture

```text
rust/                  Native Rust link-cli library and clink binary
src/lib.rs             wasm-bindgen wrapper around the Rust query processor
web/src/               React workbench
web/pkg/               Generated Rust WASM package, ignored by git
dist/                  Generated GitHub Pages artifact, ignored by git
```

The browser app initializes two WebAssembly-backed runtimes:

- `clink-wasm`: exposes `Clink#execute`, `Clink#snapshot`, and `Clink#reset`.
  It uses an in-memory implementation of the `NamedTypeLinks` trait, so the same
  Rust `QueryProcessor` used by the native CLI can run in the browser without
  filesystem access.
- `doublets-web@0.1.2`: the latest npm release of the WebAssembly bindings for
  `doublets-rs`. The React app mirrors the current `Clink` snapshot into a
  `UnitedLinks` instance after each query.

## Why the Old Proof of Concept Changed

The previous branch had a root-level Rust parser and storage implementation
that duplicated only a small subset of CLI behavior. After merging current
`main`, the repository has a fuller Rust port under `rust/`, so the WASM wrapper
now delegates query semantics to that shared Rust core.

## CI and Pages

`.github/workflows/wasm.yml` now:

1. Installs stable Rust with the `wasm32-unknown-unknown` target.
2. Installs npm dependencies with `npm ci`.
3. Runs the Rust CLI core tests.
4. Runs `wasm-pack test --node` for the wrapper.
5. Builds the React app.
6. Deploys `dist/` to GitHub Pages on pushes to `main`.

The workflow uses current Pages and artifact actions:

- `actions/upload-artifact@v4`
- `actions/configure-pages@v5`
- `actions/upload-pages-artifact@v3`
- `actions/deploy-pages@v4`

## Local Commands

```bash
cargo test --manifest-path rust/Cargo.toml --all-features
cargo test --lib
npm run test:wasm
npm run build
npm run dev
```

## Browser Data Model

The page session is intentionally in-memory. Query results include a structured
`links` array:

```json
[
  { "id": 1, "source": 1, "target": 1, "name": "father" },
  { "id": 2, "source": 2, "target": 2, "name": "mother" },
  { "id": 3, "source": 1, "target": 2, "name": "child" }
]
```

That array drives both the rendered graph and the `doublets-web` `UnitedLinks`
mirror.

## Follow-Up Scope

The current implementation proves the browser runtime and static deployment.
Durable browser storage can be added later with IndexedDB without changing the
Rust query processor API.
