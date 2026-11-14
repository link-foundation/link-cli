# What `js/` implements and what it delegates to Rust WebAssembly

`js/` holds three separate JavaScript products. The authoritative per-feature
record is the `javascript` cell of each feature in
[`parity/features.json`](../parity/features.json). The parity checker keeps
that record current (see [PARITY.md](PARITY.md)), and this page summarises it.

| Product              | Path          | Runs where           | Rust needed?                                                        |
| -------------------- | ------------- | -------------------- | ------------------------------------------------------------------- |
| Browser workbench    | `js/src/`     | Browser (React/Vite) | Yes. `wasm-pack` builds `rust/wasm` into `js/pkg/`.                 |
| Client library       | `js/client/`  | Node.js and browsers | No. It is pure JavaScript; WebAssembly is an optional binding.      |
| Server and embedded store | `js/server/` | Node.js, Bun and Deno | No. It is pure JavaScript, a port of the Rust `clink`.        |

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

The workbench shows these features through the WASM binding and needs no
JavaScript code of its own for them:

- query language
- change reports
- names
- pinned types
- types
- Unicode strings
- LiNo input and output
- doublets storage
- the CLI surface

Each of them also has a JavaScript implementation in `js/server/`, so the
`javascript` cells of `parity/features.json` are `implemented` and point at
its tests. A change to one of these features in Rust is a change to
`js/server/` too.

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

## The server and embedded store (`js/server/`) ports the Rust `clink`

`@link-foundation/link-cli-server` is the Rust `clink` in JavaScript, with no
WebAssembly and no native code. Its `clink` executable takes the same command
line and prints the same output, errors and exit codes; its files are the
Rust files, byte for byte, so either program can open a store the other
wrote. Its [README](../js/server/README.md) shows both uses: the command line
and the embedded API (`NamedTypesDecorator.open`, `QueryProcessor`, the
decorators and the servers, from `src/index.js`).

It implements, with tests under `js/server/test/`:

- the doublets store, its file format and the names sidecar
- names, pinned types, types and Unicode strings
- the query language, change reports and the reference validator
- LiNo input and output, with a port of the Rust links-notation parser
  (`src/protocol-lino-parser.js`), and store archives
- persistent transformations (triggers)
- transactions, their log, commit modes and retention
- version control: branches, tags and checkout
- triplets over doublets
- permissions and tokens
- the protocol server (`--serve`) over TCP and the IPC transports, the relay
  (`--forward`) and the protocol client (`--connect`)
- the GraphQL endpoint for Deep (`--serve ADDR --graphql`)

So the store policies that the WASM build does not expose, and the GraphQL
endpoint, have a JavaScript implementation as well as the C# and Rust ones.
The differential tests in `js/server/test/differential/` run each
invocation through the Rust `clink` and through this one and compare
standard output, standard error, the exit status and every file written.

## Why the distinction matters for CI

Since issue #111, the JavaScript stage (`javascript-stage.yml`) runs
`npm run lint`, `npm run test:js` and the parity checker without Rust or
`wasm-pack`. `test:js` covers the workbench's own JavaScript, the CI and
release scripts, and the parity checker. The client library's unit tests run
in `js-client.yml`. Its conformance job against the C# and Rust servers waits
for the JavaScript stage. The server's unit tests run in `js-server.yml` on
Node.js, Bun and Deno with complete coverage, and so do its differential
tests against the Rust `clink` and the client conformance scenarios against
its own `clink --serve`; the jobs that build Rust wait for the JavaScript
stage.

The delegated parts need the WebAssembly build. That build stays in
`wasm.yml` (`npm run test:wasm` and `npm run build`), and like every other
Rust job, it starts only after the JavaScript stage succeeds.
