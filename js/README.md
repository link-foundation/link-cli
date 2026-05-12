# link-cli JavaScript Workbench

This package contains the React and WebAssembly browser workbench for
`link-cli`. It lives under `js/` so the repository root stays language-neutral
and the JavaScript lockfile is scoped to the app that uses it.

The browser workbench combines three runtimes:

- Rust `link-cli` core compiled to WebAssembly through the `rust/wasm`
  `clink-wasm` crate.
- React and Vite for the single-page browser interface in `js/`.
- `doublets-web` for a live WebAssembly `UnitedLinks` mirror built from the
  current query result. The committed lockfile currently pins `0.1.2`.

## Architecture

```text
rust/                  Native Rust link-cli library and clink binary
rust/wasm/             wasm-bindgen wrapper around the Rust query processor
js/src/                React workbench
js/pkg/                Generated Rust WASM package, ignored by git
dist/                  Generated GitHub Pages artifact, ignored by git
```

The browser app initializes two WebAssembly-backed runtimes:

- `clink-wasm`: exposes `Clink#execute`, `Clink#snapshot`, and `Clink#reset`.
  It uses an in-memory implementation of the `NamedTypeLinks` trait, so the same
  Rust `QueryProcessor` used by the native CLI can run in the browser without
  filesystem access.
- `doublets-web`: the WebAssembly bindings for `doublets-rs`. The React app
  mirrors the current `Clink` snapshot into a `UnitedLinks` instance after each
  query.

The page session is intentionally in-memory. Durable browser storage can be
added later with IndexedDB without changing the Rust query processor API.

## Local Development

Run these commands from `js/`:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --version 0.14.0 --locked
npm install
npm run dev
```

The dev script builds the Rust WebAssembly wrapper into `js/pkg/` and starts a
Vite server.

## Production Build

Run this from `js/`:

```bash
npm run build
```

This creates:

- `js/pkg/`: generated `wasm-pack --target web` package for the Rust wrapper.
- `dist/`: static React app ready for GitHub Pages.

For the same base path used by GitHub Pages:

```bash
npm run build:pages
```

## CI and Pages

`.github/workflows/wasm.yml`:

1. Installs stable Rust with the `wasm32-unknown-unknown` target.
2. Installs npm dependencies from `js/package-lock.json`.
3. Runs the Rust CLI core tests.
4. Runs `wasm-pack test --node ../rust/wasm` from `js/`.
5. Builds the React app into root `dist/`.
6. Deploys `dist/` to GitHub Pages only for a manual `workflow_dispatch` run on
   `main` when `deploy_pages` is true.

## API

```js
import init, { Clink } from './pkg/clink_wasm.js';

await init();

const clink = new Clink();
const result = JSON.parse(
  clink.execute(
    '() ((child: father mother))',
    JSON.stringify({
      before: false,
      changes: true,
      after: true,
      autoCreateMissingReferences: true,
    }),
  ),
);

console.log(result.output);
console.log(result.links);
```

`Clink#execute(query, optionsJson)` returns:

```json
{
  "success": true,
  "output": "() ((child: father mother))",
  "error": null,
  "links": [
    { "id": 1, "source": 1, "target": 1, "name": "father" }
  ]
}
```

Supported options are `before`, `changes`, `after`, `trace`,
`autoCreateMissingReferences`, and `structure`.

## Browser Data Model

Query results include a structured `links` array:

```json
[
  { "id": 1, "source": 1, "target": 1, "name": "father" },
  { "id": 2, "source": 2, "target": 2, "name": "mother" },
  { "id": 3, "source": 1, "target": 2, "name": "child" }
]
```

That array drives both the rendered graph and the `doublets-web` `UnitedLinks`
mirror.

## Verification

From the repository root:

```bash
cargo test --manifest-path rust/Cargo.toml --all-features
cargo test --manifest-path rust/wasm/Cargo.toml --lib
npm --prefix js run test:wasm
npm --prefix js run build
```
