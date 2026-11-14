# What `js/` implements and what it delegates to Rust WebAssembly

`js/` holds two separate JavaScript products. The authoritative per-feature
record is the `javascript` cell of each feature in
[`parity/features.json`](../parity/features.json). The parity checker keeps
that record current (see [PARITY.md](PARITY.md)), and this page summarises it.

| Product              | Path          | Runs where           | Rust needed?                                                        |
| -------------------- | ------------- | -------------------- | ------------------------------------------------------------------- |
| Browser workbench    | `js/src/`     | Browser (React/Vite) | Yes. `wasm-pack` builds `rust/wasm` into `js/pkg/`.                 |
| Client library       | `js/client/`  | Node.js and browsers | No. It is pure JavaScript; WebAssembly is an optional binding.      |

## The browser workbench (`js/src/`) delegates the core to Rust

The workbench is a user interface. It has no query engine or store of its
own.

| Workbench part                                      | Implemented in                     | Notes                                                                                                                                                   |
| --------------------------------------------------- | ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Query editor, samples, options, result panes        | JavaScript (`js/src/App.jsx`)      | React state only.                                                                                                                                       |
| Graph layout of the result                          | JavaScript (`js/src/linkGraph.js`) | Lays out the result on a circle. Its unit tests are `js/test/linkGraph.test.mjs`.                                                                       |
| LiNo query parsing and execution                    | **Rust**, via `Clink#execute`      | `rust/wasm/src/lib.rs` wraps the same `QueryProcessor` the native `clink` uses.                                                                          |
| Change reports, names, types, Unicode strings, LiNo I/O | **Rust**, via `Clink`          | Delegated cells in `parity/features.json`.                                                                                                              |
| Store                                               | **Rust**, in memory                | `Clink` uses an in-memory `NamedTypeLinks`. Files, transactions and version control are not exposed.                                                    |
| `UnitedLinks` mirror of the result                  | `doublets-web` (WebAssembly)       | A third-party package built from the doublets Rust crate.                                                                                               |
| Snapshot, reset, version                            | **Rust**, via `Clink`              | `snapshot()`, `reset()`, `version()`, `rustCoreVersion()`.                                                                                              |

So features marked `delegated` with `"to": "rust"` change only in Rust and
need no JavaScript code. The workbench shows them through the WASM binding:

- query language
- change reports
- names
- pinned types
- types
- Unicode strings
- LiNo input and output
- doublets storage
- the CLI surface

## The client library (`js/client/`) implements its own features

`@link-foundation/link-cli-client` talks to a C# or Rust `clink` server, or
runs its own engine. It implements these features in JavaScript, with tests
under `js/client/test/`:

- the protocol client, framing and LiNo codec
- TCP, process and IPC transports
- union, remap and cache links
- the embedded engine, including:
  - delta and timed logs
  - causal sessions
  - native pages and checkpoints
  - LSM sorted runs
  - clusters and Raft
  - reclamation
  - the SQLite log
  - the author fence
  - the address allocator
- the WebAssembly bindings (`js/client/src/wasm.js`)

Some features are server-side store policy that the WASM build does not
expose: persistent transformations, transactions, version control, triplets
and permissions. They are `not-applicable` for JavaScript, because a
JavaScript program reaches them as a protocol client of a C# or Rust server.
The GraphQL endpoint is served only by the C# and Rust CLIs.

## Why the distinction matters for CI

Since issue #111, the JavaScript stage (`javascript-stage.yml`) runs
`npm run lint`, `npm run test:js` and the parity checker without Rust or
`wasm-pack`. `test:js` covers the workbench's own JavaScript, the CI and
release scripts, and the parity checker. The client library's unit tests run
in `js-client.yml`. Its conformance job against the C# and Rust servers waits
for the JavaScript stage.

The delegated parts need the WebAssembly build. That build stays in
`wasm.yml` (`npm run test:wasm` and `npm run build`), and like every other
Rust job, it starts only after the JavaScript stage succeeds.
