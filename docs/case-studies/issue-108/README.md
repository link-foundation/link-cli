# Case study: a minimal universal JS/TS client (issue #108)

Issue: [#108 "Publish a minimal universal JS/TS adapter for existing RemoteLinks TCP and WASM backends"](https://github.com/link-foundation/link-cli/issues/108)
Pull request: [#123](https://github.com/link-foundation/link-cli/pull/123)

This folder contains:

- `README.md`: this analysis. It covers the requirements, the design, what
  conformance found in the backends, the measurements and the remaining limits.
- [`synthetic-consumer.txt`](synthetic-consumer.txt): the full output of the
  acceptance benchmark on every runtime and backend.
- `evidence/`: the conformance runs on Node.js, Bun and Deno, and the run
  against the Rust server from before the save fix (§5.3).
- `github-data/`: the issue and its comments as JSON, captured on 2026-10-15.

## 1. The problem

A consumer (`konard/vietnam-accomodation-search`) uses the Rust `clink` as a
verified binary ledger. It carries its own projection, subprocess, readback
and rollback adapters. A supported JS/TS client could make them much smaller.

The servers already exist: `clink --serve` speaks the LiNo text protocol of
#105 in both the Rust and the C# port, and both ports have a `RemoteLinks`
client. What was missing on the JS side:

- The only JS package (`js/`) is a private React and WebAssembly workbench,
  not a client a server program can depend on.
- Nothing states which guarantees each backend gives (transactions, flush,
  pagination), so consumers had to find out by reading Rust and C# code.

The issue explicitly does not ask for a new TCP server (#105), a new
transactional library (#94, #98) or a new benchmark issue (#31, #35).

## 2. Requirements

| # | Requirement (from the issue and its comments) | Where it is addressed |
|---|---|---|
| R1 | A small, versioned, published JS/TS adapter with a stable operations/batch contract | `js/client`: `@link-foundation/link-cli-client` 0.1.0, no runtime dependencies, released to npm by `js-client.yml` (§4.1, §4.7) |
| R2 | Explicit backend capabilities | `client.capabilities` and `require(...)` (§4.3) |
| R3 | A Node/Bun/Deno transport for the RemoteLinks protocol | `connectTcp` / `TcpTransport` (`./tcp`), on `node:net`, which all three runtimes provide |
| R4 | An optional CLI adapter where subprocesses are needed | `connectProcess` / `startServer` (`./process`) start `clink --serve` and own it |
| R5 | A browser WASM adapter with the same operations, which does not pretend to support raw TCP | `createWasmClient` over `Clink.request` (clink-wasm 2.4.0). `rawTcp: false`, and `require('rawTcp')` throws. The main entry point imports nothing from `node:`, and a test checks that |
| R6 | Lossless full-width addresses, including fixtures above 2^53 | `bigint` everywhere. A 64-bit fake backend round-trips 2^53 + 1, and out-of-range addresses are refused before sending (§4.2) |
| R7 | Bounded requests and replies, backpressure | `maxRequestBytes`, `maxReplyBytes`, `maxInFlight`, and writes that wait for `drain` (§4.4) |
| R8 | AbortSignal and absolute deadlines | `{ signal, deadline, timeout }` on every call, with `not-sent` / `unknown` outcomes (§4.4) |
| R9 | Clean up only owned processes and connections | `owned` flags. `close()` and `await using` end only what the client created (§4.5) |
| R10 | Import/export or batches with verified completion and fresh-client readback | `batch()` reports each operation and `completed`, `exportLinks()`, and the conformance runner and benchmark read back with a fresh client (§4.6) |
| R11 | Document transactions/rollback, crash consistency, durable flush, pagination, bulk; no silent emulation | Capability table in `js/client/README.md` and §5. `require('atomicTransactions')` throws on every backend |
| R12 | No shell: executable and argv separate, paths with spaces | `spawn(command, args)` without a shell. The conformance runner keeps every database in a directory whose name has spaces |
| R13 | No React workbench or browser runtime needed in server consumers | A separate package. The TCP and process entry points use only Node.js built-ins |
| R14 | Conformance across runtimes and backend implementations: duplicates, holes, append/restart, aborted batches, Unicode/LiNo fidelity, incompatible protocol versions | `test/conformance/run.mjs` on Node.js, Bun and Deno against Rust, C# and WebAssembly: 32 passed, 4 skipped as not applicable on each runtime (§5) |
| R15 | A synthetic consumer benchmark: multilingual nested records, trace references, duplicate offers, bulk batches, fresh-client readback, measuring latency, RPCs, memory and disk | `examples/synthetic-consumer.mjs` (§6) |
| R16 | Do not bypass sharding or assume unbounded safe storage; keep limits explicit (doublets-rs#68, mem-rs#36, data-rs#18) | Limits in the README and §7. The client never shards or spreads data on its own |
| R17 | Comment: the decorator pattern must work as a remote or a local client everywhere, so that one can be swapped for the other, like `ILinks` in C#, in all supported languages | `LinksClient` is the same contract over local (WebAssembly) and remote (TCP, process) stores, and `LinksDecorator` wraps any of them. Rust `RemoteLinks` implements `Links<u32>`, `Doublets<u32>` and `NamedTypeLinks`, and C# `RemoteLinks` implements `INamedTypesLinks<uint>`, the interfaces their local stores implement (§4.8) |

### Reading of ambiguous points

- **"published".** The package is set up to be published: an npm release
  job runs on `main` and publishes each version that npm does not have yet.
  It publishes through npm trusted publishing (OIDC) once the package trusts
  the workflow, and otherwise with an `NPM_TOKEN` repository secret, the same
  order main uses for crates.io. When neither works the job fails on `main`
  with a message that says what to add, instead of passing silently. Pull
  requests run `npm publish --dry-run`.
- **"full-width addresses".** Both servers and the WebAssembly store use
  `u32` addresses. The client is not limited to 32 bits: addresses are
  `bigint`, and the range comes from `capabilities.maxAddress`. The > 2^53
  fixtures run against a 64-bit fake backend, because no real backend has
  64-bit addresses today.
- **"incompatible protocol versions".** The text protocol has no version
  handshake, so "incompatible" means a server that refuses the text
  protocol: `clink --serve --protocol binary`. The client reports this as a
  `ProtocolMismatchError` before it sends anything, and `capabilities.protocol`
  (`links-text/1`) names the protocol version the client speaks.
- **"rollback verification".** No backend has transactions over the
  protocol. The client says so (`atomicTransactions: false`,
  `rollback: false`) and does not emulate them. What it gives instead is a
  per-operation outcome, so the consumer knows which writes ran and can read
  the store back to verify.

## 3. Prior art

| Source | What it has | What we took |
|---|---|---|
| `docs/protocol/links-operations.txt` (#105) | The golden conversation that the Rust and C# server tests replay | The conformance runner replays it byte for byte on every backend |
| Rust `RemoteLinks` (`rust/src/protocol/remote_links.rs`), C# `RemoteLinks` | The operations as LiNo documents: `(create: (s t))`, `(each: (...))`, `(get-name: i)` ... | The same request shapes, so all three clients send identical text |
| `links-notation` 0.26 `binary::{format_document, parse_document}` | Canonical LiNo with `~1{hex}` literals and quote runs | `src/lino.js` ports exactly the subset the protocol exchanges, checked against the server texts |
| Redis RESP pipelining, PostgreSQL extended query | FIFO replies on one connection; cancel through a separate channel | Pipelining up to `maxInFlight`. Since this protocol has no cancel message, aborting a request in flight closes the connection and marks it `unknown` |
| `AbortSignal`, `AbortSignal.timeout`, `Symbol.asyncDispose` | Standard cancellation and scoped cleanup across runtimes | Used directly, so the same code runs on Node.js, Bun and Deno |

## 4. Design

### 4.1 Package layout

```
js/client/
  src/index.js        universal entry: LinksClient, LinksDecorator, errors, capabilities, createWasmClient
  src/tcp.js          connectTcp, TcpTransport (node:net)
  src/process.js      startServer, connectProcess (node:child_process)
  src/lino.js         LiNo formatting and parsing
  src/framing.js      message framing of the text protocol (dot line, dot stuffing)
  src/operations.js   requests and reply shapes of every operation
  types/*.d.ts        TypeScript declarations, checked with tsc --strict
  test/*.test.mjs     unit tests (node:test, fake servers and stores)
  test/conformance/   the conformance runner (real backends, any runtime)
  examples/           the synthetic consumer benchmark
```

A transport is `{ capabilities, send(text, scope), close() }`. `LinksClient`
turns operations into protocol text and parses the replies, so every backend
gets the same parsing and validation.

### 4.2 Addresses

Addresses are `bigint` both ways. Numbers are accepted when they are safe
integers. Anything above `capabilities.maxAddress` is refused with
`AddressRangeError` before a byte is sent, so a too-large address can never
reach a `u32` server and wrap around.

### 4.3 Capabilities

`capabilities` is a frozen object per backend (see the table in
`js/client/README.md`). `require(...guarantees)` throws `UnsupportedError`
naming every missing guarantee, so code that needs transactions fails before
it writes. Over TCP the client cannot tell a Rust server from a C# one, so
`duplicateLinks` is `null` (unknown) unless the caller states it.

### 4.4 Cancellation, limits and backpressure

Every request has a scope: `signal`, an absolute `deadline` (`Date` or epoch
milliseconds), or a relative `timeout`; the earliest wins. The outcome of a
failed request says what is known about it:

- `not-sent`: it never left the client. Retrying is safe.
- `failed`: the backend answered with an error. Nothing changed.
- `unknown`: it was sent, and its answer is lost. Read the store back first.

Replies are matched to requests in FIFO order, because the protocol has no
request ids. That is why an abort in flight has to close the connection: the
reply would otherwise be matched to the next request.

Message size is bounded in both directions. A reply that grows past
`maxReplyBytes` closes the connection as soon as the limit is crossed,
instead of buffering it. At most `maxInFlight` requests are pipelined, the
rest wait, and writes wait for the socket to drain.

### 4.5 Ownership

A client closes only what it created. `connectTcp` owns its socket;
`new TcpTransport(socket, { owned: false })` borrows one. `connectProcess`
owns the server it starts and stops it (SIGINT, then SIGKILL after a grace
period) after closing the connection. `startServer` returns a process the
caller owns. A `Clink` passed to `createWasmClient` is freed only with
`{ owned: true }`. Clients implement `Symbol.asyncDispose`, so `await using`
works where the runtime supports it.

### 4.6 Batches

`batch(operations, { window })` validates every operation first (a malformed
one sends nothing), pipelines up to `window` requests, and stops sending after
the first failure. It reports each operation as `ok`, `failed`, `unknown` or
`not-sent`, with counts and `completed`. A batch is not a transaction. It is
verified when `completed` is `true`, or when a fresh client reads the result
back. The conformance runner and the benchmark both do that.

### 4.7 Release and CI

`.github/workflows/js-client.yml` follows the repository's workflow policy
(`.github/scripts/workflow-policy.test.mjs`):

- unit tests and the strict type check of the declarations;
- builds the Rust server, the C# server and the WebAssembly package, then runs
  the conformance runner on Node.js, Bun and Deno, and a small run of the
  synthetic consumer on each backend;
- the release job runs `npm publish --dry-run` on pull requests. On `main` it
  publishes with provenance when the version is new (trusted publishing
  first, then the `NPM_TOKEN` secret), and fails with an explicit message
  when neither credential can publish.

### 4.8 Local and remote behind one contract (issue comment)

The comment asks that the decorator pattern work the same for local and
remote stores in every language, so that one can be swapped for the other.

| Language | Contract | Local | Remote | Decorators |
|---|---|---|---|---|
| C# | `ILinks<uint>` / `INamedTypesLinks<uint>` | the doublets stores | `RemoteLinks` (#105) | `NamedTypesDecorator`, `PinnedTypesDecorator`, ... |
| Rust | `Links<u32>`, `Doublets<u32>`, `NamedTypeLinks` | `LinkStorage`, the doublets stores | `RemoteLinks` (#105) | the decorators in `rust/src` |
| JS/TS | `Links` (`types/index.d.ts`) | `createWasmClient(clink)` | `connectTcp`, `connectProcess` | `LinksDecorator` |

In JS the swap is a single line. The synthetic consumer is one program that
runs unchanged against the WebAssembly store and both servers; only
`--backend` changes. The unit test "a decorator is a drop-in client over any
backend" wraps a client in a `LinksDecorator` subclass and checks that every
call reaches the backend.

## 5. What conformance found

The runner (`js/client/test/conformance/run.mjs`) runs every scenario against
each backend that is configured (`CLINK_RUST`, `CLINK_CSHARP`, `CLINK_WASM`).
Each process backend gets its own database in a directory whose name contains
spaces:

| Scenario | Rust | C# | WASM |
|---|---|---|---|
| golden conversation, byte for byte | ✓ | ✓ | ✓ (with its own `set-name` answer) |
| u32 addresses, out-of-range refused before sending | ✓ | ✓ | ✓ |
| duplicates (as stated by `duplicateLinks`) | ✓ kept apart | ✓ merged, `create` → `null` | ✓ kept apart |
| holes: cascade delete, lowest-first reuse, `Link not found` | ✓ | ✓ | ✓ |
| append and restart through a fresh server process | ✓ | ✓ | n/a (memory only) |
| aborted batch: every `ok` is stored, no `not-sent` is | ✓ | ✓ | ✓ |
| a server stopped mid-write keeps every acknowledged write | ✓ (after §5.3) | ✓ | n/a |
| Unicode and LiNo names (quotes, parens, colon, `~1{..}`, newlines, 2000 chars) | ✓ | ✓ | ✓ |
| empty names, moving a name between links | ✓ | ✓ | ✓ (after §5.2) |
| request size limit | ✓ | ✓ | ✓ |
| a server that refuses the text protocol | ✓ | ✓ | n/a |
| process ownership | ✓ | ✓ | n/a |

The result is 32 passed, 0 failed and 4 skipped (not applicable) on Node.js
26.11, Bun 1.4 and Deno 2.9: [`evidence/conformance-node.txt`](evidence/conformance-node.txt),
[`-bun`](evidence/conformance-bun.txt), [`-deno`](evidence/conformance-deno.txt).

### 5.1 Duplicate links differ between the servers

Creating a link equal to an existing one stores a second link on the Rust
server and the WebAssembly store. The C# server keeps doublets unique, so it
merges them and answers with no change. Rather than hide the difference, the
client makes it explicit: `create` resolves to `null` when the backend merged
the link, and the `duplicateLinks` capability states the behaviour.

### 5.2 Moving a name left it on the old link (fixed)

`set-name` on a second link with a name already held by a first one moved the
name in the C# port. In the Rust `LinkStorage`, which the WebAssembly store
uses, both links reported the name, and removing the stale one unnamed the
new holder. `LinkStorage::set_name` now takes the name away from its previous
link (`test_storage_moving_a_name_unnames_its_previous_link`).

### 5.3 Stopping the Rust server mid-write lost the whole database (fixed)

The aborted-batch scenario failed intermittently against the Rust server:

```
not ok - rust: aborted batch: every write reported ok is there, nothing reported not-sent is
  # AssertionError [ERR_ASSERTION]: 0 links after 32 ok, 8 unknown
```

Zero links: not only the unknown writes, but the 32 acknowledged ones were
gone too. Root cause: `LinkStorage::save` opened the database with
`truncate(true)` and wrote it again. The server has no signal handler, so the
SIGINT that `connectProcess` sends when it closes ended the process at once.
If that happened between the truncation and the end of the write, the file
stayed empty or partial.

The fix writes `db.links.saving` next to the database and renames it over the
old file, so a stopped process leaves the last complete save. Evidence:

- `test_storage_save_replaces_the_file_instead_of_truncating_it`
  (`rust/tests/link_storage_tests.rs`) is deterministic. A handle opened on
  the database before a save must still read the previous content. It failed
  before the fix (`left: "(1 1 1)\n(2 2 2)\n(3 3 3)\n"`, `right: "(1 1 1)\n(2 2 2)\n"`).
- The new conformance scenario stops a server in the middle of a pipelined
  batch four times, reopening the database each time. Against the pre-fix
  binary it failed in every one of three runs
  ([`evidence/conformance-node-rust-before-save-fix.txt`](evidence/conformance-node-rust-before-save-fix.txt)).
  With the fix it passes on every runtime, for both servers.
- A standalone stress loop of 20 rounds of "stop during writes, restart,
  count" lost every acknowledged write in 6 of 20 rounds before the fix, and
  in 0 of 20 after.

This is not full crash consistency. Nothing is `fsync`ed, and the links and
the names are two files saved one after the other, so `crashConsistency` and
`durableFlush` stay `false`.

## 6. Synthetic consumer benchmark

`js/client/examples/synthetic-consumer.mjs` generates deterministic,
self-authored data (seed 108): 300 records with multilingual names and places
(Vietnamese, Russian, Japanese, Arabic, emoji ...), nested addresses, offers
with trace references (`trace:crawl-N/page-M`), and a duplicate offer every
fourth record that the consumer drops. It writes them in phased bulk batches
(window 64), then reads everything back with a fresh client and compares the
normalized records, traces and offers. The run fails unless the readback is
identical.

Results from a 6-vCPU AMD EPYC VM, 300 records = 4128 links, 5137 write
requests, 1010 read requests. Full output with per-phase timings and
latency percentiles: [`synthetic-consumer.txt`](synthetic-consumer.txt).

| Runtime | Backend | Write | Writes/s | Read back | Server peak RSS | Disk | Readback |
|---|---|---|---|---|---|---|---|
| Node.js | WASM | 0.50 s | 10 352 | 0.09 s | in process | memory | identical |
| Node.js | C# | 5.60 s | 917 | 0.94 s | 62 MiB | 345 KiB | identical |
| Node.js | Rust | 22.4 s | 229 | 2.71 s | 3.8 MiB | 183 KiB | identical |
| Bun | WASM | 0.33 s | 15 638 | 0.06 s | in process | memory | identical |
| Bun | C# | 1.81 s | 2 835 | 0.53 s | 62 MiB | 345 KiB | identical |
| Bun | Rust | 18.3 s | 280 | 3.86 s | 3.9 MiB | 183 KiB | identical |
| Deno | WASM | 0.39 s | 13 332 | 0.13 s | in process | memory | identical |
| Deno | C# | 1.75 s | 2 937 | 0.66 s | 62 MiB | 345 KiB | identical |
| Deno | Rust | 18.5 s | 277 | 2.40 s | 3.8 MiB | 183 KiB | identical |

Observations:

- The readback was identical on all nine combinations.
- The Rust server writes its whole database file on every write, so each
  write costs time in proportion to the size of the store. It handles about
  230 to 280 writes/s at 4k links, and its `get-name` takes about 2.5 ms.
  This server design limits throughput, not the client.
- The C# server is about 10x faster on writes, but it uses about 16x the
  memory. Its memory-mapped files are larger on disk.
- The WebAssembly store has no I/O and shows the overhead of the client
  itself: about 0.1 ms per request.
- The Node.js C# figure varied between runs (2.7 s and 5.6 s on the same
  machine), so read these as orders of magnitude.

Benchmarks across releases belong in #31 and #35. This run is acceptance
evidence for #108, and CI only runs a small smoke version of it.

## 7. Known limits

- No backend offers atomic transactions, rollback, durable flush, crash
  consistency, pagination or sharding over the protocol. The capabilities say
  so, and the client does not emulate any of them.
- `each` and `exportLinks` return every match in one reply. `maxReplyBytes`
  bounds the reply, and there is no pagination to fall back on.
- Over TCP, the client cannot tell a Rust server from a C# one. Callers who
  depend on duplicate handling should state `duplicateLinks`.
- Upstream storage defects block unsharded growth:
  [doublets-rs#68](https://github.com/linksplatform/doublets-rs/issues/68)
  (panic after 1,040,383 links),
  [mem-rs#36](https://github.com/linksplatform/mem-rs/issues/36) (growth of
  a mapped file can overwrite data) and
  [data-rs#18](https://github.com/linksplatform/data-rs/issues/18) (the
  external address range overlaps a service constant). The client keeps each
  connection on one store and never shards on its own.
- The first npm release needs the `NPM_TOKEN` secret: npm can only trust a
  workflow as publisher of a package that already exists (§4.7).

## 8. Reproduce

```bash
cargo build --release --manifest-path rust/Cargo.toml --bin clink
dotnet build csharp/Foundation.Data.Doublets.Cli --configuration Release
(cd js && npm run build:wasm)
cd js/client && npm install && npm test && npm run typecheck
export CLINK_RUST=$PWD/../../rust/target/release/clink
export CLINK_CSHARP=$PWD/../../csharp/Foundation.Data.Doublets.Cli/bin/Release/net10.0/clink
export CLINK_WASM=$PWD/../pkg
node test/conformance/run.mjs; bun test/conformance/run.mjs; deno run -A test/conformance/run.mjs
node examples/synthetic-consumer.mjs --backend rust --records 300
```
