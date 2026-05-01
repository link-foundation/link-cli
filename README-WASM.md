# link-cli WebAssembly Workbench

The browser workbench combines three runtimes:

- Rust `link-cli` core compiled to WebAssembly through the root `clink-wasm`
  crate.
- React and Vite for the single-page browser interface in `web/`.
- `doublets-web@0.1.2` for a live WebAssembly `UnitedLinks` mirror built from
  the current query result.

## Local Development

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --version 0.14.0 --locked
npm install
npm run dev
```

The dev script builds the Rust WebAssembly wrapper into `web/pkg/` and starts a
Vite server.

## Production Build

```bash
npm run build
```

This creates:

- `web/pkg/`: generated `wasm-pack --target web` package for the Rust wrapper.
- `dist/`: static React app ready for GitHub Pages.

For the same base path used by GitHub Pages:

```bash
npm run build:pages
```

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

## Verification

```bash
cargo test --manifest-path rust/Cargo.toml --all-features
cargo test --lib
npm run test:wasm
npm run build
```

The WebAssembly CI workflow runs these checks and deploys `dist/` to GitHub
Pages from `main`.
