# @link-foundation/link-cli-client

A small JavaScript/TypeScript client for link-cli links stores. It offers one
contract over several backends:

| Backend | Entry point | Runtimes | Reaches |
| --- | --- | --- | --- |
| TCP | `@link-foundation/link-cli-client/tcp` | Node.js, Bun, Deno | a running `clink --serve` (Rust or C#) |
| CLI process | `@link-foundation/link-cli-client/process` | Node.js, Bun, Deno | a `clink --serve` the client starts and stops |
| WebAssembly | `@link-foundation/link-cli-client` | browsers, Node.js, Bun, Deno | an in-process `Clink` store from `rust/wasm` |

It is the JS counterpart of `RemoteLinks` in the other ports (C#
`INamedTypesLinks<uint>`, Rust `NamedTypeLinks`). Code written against
`LinksClient` works the same whether the store is local (WebAssembly) or
remote (TCP), and `LinksDecorator` wraps any of them, so you can swap one for
the other.

The package has no runtime dependencies. The main entry point imports nothing
from `node:`, and nothing here needs React or a browser.

The separately imported `./engine-causal-version` component supplies bounded
causal row encoding, retained-sibling reconciliation and checked counter
construction. See [configuration, costs and limits](../../docs/ENGINE-CAUSAL-VERSIONS.md).
The separately selected `./engine-causal-replica` composes a coherent local
atomic provider with causal mutations and durable retry receipts. See
[ownership, finite replay costs and physical examples](../../docs/ENGINE-CAUSAL-REPLICAS.md).
The independent `./engine-causal-merkle` export adds bounded immutable content
snapshots, canonical proof frames and resumable differing-range pulls. See
[configuration, budgets and physical exchange](../../docs/ENGINE-CAUSAL-MERKLE.md).
The selected `./engine-causal-batch` export stages up to 64 ordered row edits
and publishes the owner and batch receipts with one local two-key CAS. See
[request identity, complete-journal costs and limits](../../docs/ENGINE-CAUSAL-BATCHES.md).
The separate `./engine-causal-projection` export supplies owned rows and names
under explicit conflict rejection or maximum-dot selection while retaining every
causal sibling. See [visibility, bounds and costs](../../docs/ENGINE-CAUSAL-PROJECTION.md).
The separate `./engine-causal-session` export stages ordinary links/names in
the actual Rust WASM workspace and publishes one explicit-token local batch.
Its `tx.client()` exposes the standard BigInt links/query API, preserving
original query reference kinds until explicit preparation transfers ownership.
See [reservations, lifecycle, physical examples and costs](../../docs/ENGINE-CAUSAL-SESSIONS.md).
Rust and C# expose matching selected query adapters; safe causal history
reclamation remains in progress.
The separately selected `./engine-address-allocator` reserves permanent fresh
blocks through one coherent atomic coordinator key. Exact author/token/count
retries retain their original range after lost replies and later grants. See
[canonical journals, session selection, budgets and costs](../../docs/protocol/address-reservations-v1.md).

## Use

```js
import { connectTcp } from '@link-foundation/link-cli-client/tcp';

const links = await connectTcp('127.0.0.1:7878', { timeout: 5000 });
try {
  const link = await links.create(1n, 1n);       // { index, source, target } as BigInt
  await links.setName(link.index, 'Hà Nội');
  console.log(await links.getByName('Hà Nội'));  // link.index
  console.log(await links.count(['*', link.index])); // links whose target is link.index
} finally {
  await links.close();                           // closes only this connection
}
```

Start and own a server process (the executable and its arguments go to argv
as separate entries, with no shell, so paths with spaces need no quoting):

```js
import { connectProcess } from '@link-foundation/link-cli-client/process';

await using links = await connectProcess({
  command: '/opt/link cli/clink',     // or command: 'dotnet', args: ['clink.dll']
  db: '/data/my links/db.links',
  capabilities: { duplicateLinks: true }, // what you know about this server
});
// leaving the scope closes the connection, then stops this server process
```

Use the in-process WebAssembly store (browser or server). Build `rust/wasm`
(`clink-wasm` 2.4.0 or later, which has `Clink.request`) with
`wasm-pack build --target web`:

```js
import init, { Clink } from './pkg/clink_wasm.js';
import { createWasmClient } from '@link-foundation/link-cli-client';

await init();
const links = createWasmClient(new Clink(), { owned: true }); // close() frees it
```

## Contract

Every method resolves to plain data and takes `{ signal, deadline, timeout }`
as its last argument:

| Method | Resolves to |
| --- | --- |
| `count(restriction?)` | `bigint` |
| `each(restriction?)` | `{ index, source, target }[]` in address order |
| `create(source, target)` | the created link, or `null` when the backend merged it into an equal link |
| `update(index, source, target)` / `delete(index)` | the net change of every link touched, `{ before, after }[]` |
| `getName(index)` / `getByName(name)` | `string \| null` / `bigint \| null` |
| `setName(index, name)` / `removeName(index)` | the address the backend answers / nothing |
| `query(text)` | changes of a LiNo substitution query, references as the backend wrote them |
| `exportLinks()` | every link, with names in place of named references |
| `batch(operations, { window })` | per operation `ok`, `failed`, `unknown` or `not-sent`, and `completed` |
| `requestText(text)` / `request(document)` | raw protocol access |
| `require(...guarantees)` | throws `UnsupportedError` unless the backend offers them all |
| `close()` / `[Symbol.asyncDispose]()` | releases what this client owns |

A restriction is `[]` (every link), `[index]`, `[index, source, target]` or
`[source, target]`; `'*'` (`ANY`) matches anything.

**Addresses** are `bigint` going out and coming back. Safe-integer numbers are
accepted as input. Anything a `number` cannot hold exactly, or the backend
cannot store, is refused before sending with `AddressRangeError`. The servers
and the WebAssembly store use `u32` addresses (`capabilities.maxAddress` is
2^32 - 1). The client itself is not limited to 32 bits: the unit tests drive a
64-bit backend with addresses above 2^53.

**Errors** are `LinksError` subclasses. Each carries `outcome`, which tells
you what you can know about the request:

- `not-sent`: the request never left the client (abort or deadline before
  sending, a limit, an address out of range, a closed connection, a missing
  capability).
- `failed`: the backend answered `(error: ...)` (`RemoteError`). Nothing
  changed.
- `unknown`: the request was sent but its answer is lost (abort or deadline
  while in flight, a lost connection, an oversized reply). Read the store
  back before retrying.

Other subclasses: `ProtocolMismatchError` (the peer is not a links server, or
it refuses the text protocol), `MalformedMessageError`, `LimitExceededError`,
`ConnectionClosedError`, `AbortError` and `DeadlineExceededError`.

**Cancellation.** `signal` is an `AbortSignal`. `deadline` is absolute (a
`Date` or epoch milliseconds), and `timeout` becomes `Date.now() + timeout`;
when both are given, the earlier wins. A queued request that is aborted is
never sent. The protocol has no request ids or cancel message, so aborting a
request that is already in flight closes the TCP connection: that request ends
`unknown`, and the ones queued behind it end `not-sent`. The WebAssembly store
runs each request synchronously, so it checks the scope before a request
starts.

**Limits and backpressure.** `maxRequestBytes` and `maxReplyBytes` (default
64 MiB) bound every message. A reply that grows past the limit closes the
connection instead of buffering without end. `maxInFlight` (default 64) caps
pipelined requests on a connection, the rest wait in a queue, and writes wait
for the socket to drain.

**Batches** pipeline up to `window` requests and stop sending after the first
failure. A batch is **not a transaction**: what ran stays applied. A batch is
verified when `completed` is `true`, or when a fresh client reads the result
back. The conformance tests and the synthetic consumer do exactly that.

**Ownership.** `close()` ends only what the client created: its own TCP
connection, and the server process it started (`connectProcess`). A borrowed
socket (`new TcpTransport(socket, { owned: false })`), a server you started
yourself (`startServer`), and a `Clink` passed without `{ owned: true }` all
outlive the client.

## Capabilities

`client.capabilities` states what a backend guarantees. Nothing below is
emulated: `require('atomicTransactions')` throws on every backend today,
rather than letting a batch look atomic.

| Capability | Rust `clink --serve` | C# `clink --serve` | WebAssembly `Clink` |
| --- | --- | --- | --- |
| `transport` / `rawTcp` | `tcp` / yes | `tcp` / yes | `in-process` / **no**: browsers have no raw TCP, and the adapter never pretends to |
| `protocol` | `links-text/1` | `links-text/1` | `links-text/1` |
| `addressBits` / `maxAddress` | 32 / 2^32 - 1 | 32 / 2^32 - 1 | 32 / 2^32 - 1 |
| `atomicTransactions`, `rollback` | no: one write per request | no | no |
| `persistent` | yes: the whole file is replaced after every write | yes: memory-mapped file | no: memory only |
| `crashConsistency` | no: a stopped server keeps its last complete save (written aside, then renamed), but the links and names files are saved one after the other, without `fsync` | no: no flush ordering | no |
| `durableFlush` | no `fsync` | no flush | no |
| `pagination` | no: `each`/`exportLinks` return every match in one reply | no | no |
| `bulk` | `pipelined-non-atomic` | `pipelined-non-atomic` | `sequential-non-atomic` |
| `sharding` | no: one store per server | no | no |
| `duplicateLinks` | `true`: equal links stay apart | `false`: equal links merge, so `create` resolves to `null` | `true` |
| set-name answers | the names-store link (`(link: 102)`) | the names-store link | the named link |

TCP clients cannot tell from the protocol whether a Rust or a C# server
answers. That is why `duplicateLinks` is `null` (unknown) over TCP unless you
state it: `connectTcp(address, { capabilities: { duplicateLinks: true } })`.

Both servers reuse freed addresses (the lowest first), delete links that use a
deleted link, answer `Link not found: N` when you update or delete a missing
link, and keep each name on exactly one link. The conformance runner checks
all of this on every backend.

### Known limits of the stores behind the servers

Plan for these explicitly, and do not route around sharding to avoid them:

- The Rust server rewrites its whole database file after every write. Writes
  get slower as the store grows (about 210-270 writes/s at 4k links in the
  synthetic consumer). Benchmarks belong in link-cli #31 and #35.
- [linksplatform/doublets-rs#68](https://github.com/linksplatform/doublets-rs/issues/68):
  unit stores panic after 1,040,383 links.
- [linksplatform/mem-rs#36](https://github.com/linksplatform/mem-rs/issues/36):
  memory-mapped file growth can overwrite existing data.
- [linksplatform/data-rs#18](https://github.com/linksplatform/data-rs/issues/18):
  the external address range overlaps a service constant.

## Development

```bash
npm install
npm test                  # unit tests (node:test, fake servers and stores)
npm run typecheck         # the .d.ts files against test/types.ts, strict
CLINK_RUST=../../rust/target/release/clink \
CLINK_CSHARP=../../csharp/Foundation.Data.Doublets.Cli/bin/Release/net10.0/clink \
CLINK_WASM=../pkg \
  npm run test:conformance   # also: bun test/conformance/run.mjs, deno run -A test/conformance/run.mjs
npm run benchmark -- --backend rust --records 300
```

The conformance runner replays `docs/protocol/links-operations.txt`, the
golden conversation that the Rust and C# test suites also replay. It covers
u32 addresses, duplicates, holes and reuse, append and restart through a fresh
server process, aborted batches read back by a fresh client, a server stopped
in the middle of a write, Unicode and LiNo quoting of names, size limits, a
server that refuses the text protocol, and process ownership. The benchmark (`examples/synthetic-consumer.mjs`) writes
multilingual nested records with trace references and duplicate offers in
bulk batches. It then reads them back with a fresh client and reports latency,
requests, memory and disk use.

Versioning follows semver. The wire protocol is versioned separately as
`capabilities.protocol` (`links-text/1`). See
[docs/case-studies/issue-108](../../docs/case-studies/issue-108/README.md) for
the design and the measurements.

### Optional isolated engine

`WasmEngine` wraps the compiled Rust staged engine with the same asynchronous
links methods on each session. Create a session with `engine.begin()`, issue
several requests, then `await session.commit()` or `await session.rollback()`.
Closing an uncommitted session rolls it back. Commit shares the request queue,
so it cannot overtake an earlier write. Defaults are optimistic admission,
serializable revision validation, no historical retention and a volatile memory log.
Use `{ serializableValidation: 'read-set' }` for serializable independent point
edits; scans still validate the entire revision. It composes with log/history
choices. See [validation configuration and costs](../../docs/ENGINE-VALIDATION.md).

Use `{ concurrency: 'pessimistic' }` to reserve written addresses until the
session ends. Independent writes can proceed; a competing write returns busy
immediately. See [ownership, retries and costs](../../docs/ENGINE-PESSIMISTIC.md).

`await engine.commitGroup([first, second])` drains and consumes independent
sessions, preserving each accepted write's revision while sharing one adapter
flush. Results report `committed`, `aborted` or `indeterminate` per member.
An uncertain member requires reopening to discover the persisted prefix; the
group is not one atomic transaction. See [group commit, ownership and costs](../../docs/ENGINE-GROUP-COMMIT.md).

Use `{ revisionEncoding: 'changes' }` to serialize changed rows independently of
`stateStructure`, history and physical log choice. Default `snapshot` encoding
remains available; both reopen mixed journals. See [change records and costs](../../docs/ENGINE-CHANGE-RECORDS.md).

See [engine configurations, guarantees, costs and runnable examples](../../docs/ENGINE-STRATEGIES.md).
A custom synchronous commit log selects storage; `MemoryCommitLog` and
`ReplicatedCommitLog` compose portable adapters. Optional `DeltaCommitLog` wraps
either a physical log or each replica to compress localized edits between full
checkpoints. Reopen the stream through the same decorator; see the
[delta format, costs and recipes](../../docs/ENGINE-DELTA.md).
For a filesystem journal use
the separate `./engine-file-log` entry point. It repairs incomplete final lines
on open and before reuse after an I/O error, retaining complete records for
outcome discovery. An engine with an uncertain commit still requires reopening.
For SQL-free immutable doublets pages use `FilePagedCommitLog` from the separate
`./engine-file-paged-log` entry. It publishes groups through alternating durable
roots, rejects committed page corruption and fences uncertain writers. The
parent directory must exist; crashed ownership requires explicit external
fencing. See [native page costs, assumptions and example](../../docs/ENGINE-NATIVE-PAGES.md).
For complete ordered-journal replacement, `CheckpointPagedCommitLog` from the
separate `./engine-checkpoint-pages` entry accepts a root device and two
page-bank devices. Keep all devices under one exclusive owner. It synchronizes
the inactive bank before publishing its epoch-bound root; uncertain operations
require reopening. See [protocol, ownership and costs](../../docs/protocol/checkpoint-pages.md).
Select root redundancy independently with `MirroredCheckpointRoot` from
`./engine-checkpoint-root`. The native owning `FileCheckpointPagedCommitLog`
from `./engine-file-checkpoint-pages` accepts `{ mirroredRoot, mirroredBanks }`,
both false by default. It acquires every selected raw owner before repair and
closes them root last. Keep both persisted modes when reopening; conversion
requires a separate destination. See [native selection and costs](../../docs/ENGINE-NATIVE-CHECKPOINTS.md).

With a replacement-capable log, call `engine.compactJournal()` to publish retained
committed snapshots while holding the engine commit lock. It returns false for
an identical journal and preserves pending work and pinned history. An uncertain
publication fences the engine until reopening. `DeltaCommitLog` forwards this
optional capability and prepares its new physical stream before inner writes.
The [actual-WASM example](../../examples/journal-checkpoints/compact.mjs) covers
both bank modes and all state, encoding and retention choices.
For a physical page WAL use the separate `./engine-sqlite-log` entry with a
dedicated synchronous Node/Bun/Deno SQLite connection. It imports no provider
into the universal entry. The adapter requires SQLite >= 3.51.3, enforces WAL/FULL,
groups pending appends into one SQL COMMIT and exposes checkpoint counters.
See [ownership, maintenance and interoperability examples](../../docs/ENGINE-SQLITE.md).
Durability is an explicit backend capability; replication acknowledgements do
not provide consensus or failover.

`PartitionedJournalStore` accepts `{ cacheBytes: 65536 }` as its fourth argument
to cache verified immutable data objects within a UTF-8 payload budget. Default
zero retains no objects. Catalog reads and publication checks always stay fresh;
every hit returns an independent array. See [cache costs and ownership](../../docs/ENGINE-OBJECT-CACHE.md)
and [coordinated network engines](../../docs/ENGINE-CLUSTERS.md).

`await ReclaimingPartitionedJournalStore.open(catalog, partitions, route, options)`
selects explicit epoch-fenced reclamation on dedicated maintenance-capable
namespaces. Call `collectGarbage()` to retire old unreferenced objects; the default
partitioned store retains its objects. See [protocol, bounded scans, owned reads
and native/WASM handoff examples](../../docs/ENGINE-RECLAMATION.md).

`const branch = engine.fork(options, journal)` seeds an independent journal.
Edit it through `branch.engine.begin()`, then call `branch.merge()` to atomically
merge its committed rows into the parent. Save `branch.mergeBaseRecord()` after
each successful merge; `engine.resumeBranch(journal, savedRecord, options)`
reopens it after a restart. Divergent edits to the same row conflict, including
independent allocations of the same address with different values. A failed
flush reports `LinksError.outcome === 'unknown'` and requires reopening the
parent for outcome discovery. Close the branch and every session before closing
its caller-owned journal adapter.

The optional `engine-mirrored-pages` and `engine-file-mirrored-pages` entries add
two-copy native page repair without importing filesystem or SQL dependencies
into the universal entry. See [native page recipes](../../docs/ENGINE-NATIVE-PAGES.md)
for ownership, synchronization, immutable mode and corruption limits.


The separate `@link-foundation/link-cli-client/engine-file-checkpoint-pages`
entry exports `FileCheckpointPagedCommitLog` for retained-history compaction in
Node, Bun and Deno. It owns a fixed root and two page banks, optionally mirrors
each bank with `{ mirroredBanks: true }`, and implements `replace(records)` for
`WasmEngine.compactJournal()`. This selection adds no SQLite requirement. Use a
new dedicated path and close the log after its engine and sessions. See the
[native configuration, file ownership and interoperability recipe](../../docs/ENGINE-NATIVE-CHECKPOINTS.md).

`SortedJournalTable` from `./engine-sorted-table` creates or opens a sealed
immutable physical table on a caller-owned `PageDevice`, including the native
`FilePageDevice`. It retains sorted keys, physical addresses and page hashes
after complete bounded validation, then reads selected physical pages for a
point lookup. Tombstones preserve exact decimal u64 generations; an empty array
is a live empty journal. Close the selected device yourself. See the
[format and costs](../../docs/protocol/native-sorted-table.md) and
[25 native writer/reader combinations](../../examples/sorted-tables/README.md).
The separate physical LSM component composes WAL/manifest publication and
compaction over these tables.

The separate `./engine-lsm-wal` entry exports `LsmWal.recover(records, options)`
and `LsmWal.encode(state, options)` for bounded semantic replay and revalidated
checkpoint encoding. Counters, IDs and digests retain all u64 bits as decimal
strings. Replay owns returned payload arrays and preserves tombstones and live
empty journals. See the [wire protocol, admission bounds and physical validation
requirements](../../docs/protocol/native-lsm-wal.md). This pure codec performs
no storage I/O.

The separate `./engine-lsm-store` entry exports `PhysicalLsmJournalStore`.
`PhysicalLsmJournalStore.open(replaceableWal, exclusiveTableNamespace, options)`
selects synchronous group CAS, bounded buffering, immutable table publication,
foreground compaction and generation-preserving maintenance deletion. Its
methods can also be awaited by the existing cluster, partition and reclamation
consumers. Close the store before releasing the caller-owned WAL and namespace.
See the [provider contract, capacities, recovery ordering and costs](../../docs/protocol/native-lsm-store.md).
The explicit `./engine-file-lsm-store` entry exports `FileLsmJournalStore` for
an existing dedicated native directory, with independent root, bank and table
mirrors. It validates existing selected state read-only before recovery effects
and holds an exclusive outer owner until all child handles close. Its finite
physical bounds, stale-fence recovery and platform durability limits are in the
[native directory contract](../../docs/protocol/native-lsm-store.md#javascript-native-directory-selection).
The [native facade examples](../../examples/native-lsm-js/README.md) exercise
Rust/C# and Node/Bun/Deno whole-store physical interchange in CI.

Optional `./engine-author-fence` guards an author lineage with explicit takeover
and permanent owner-token receipts. Pass its bound provider to existing causal
batches, sessions or address reservations. Every data CAS compares the current
fence atomically; takeover is application-controlled, without TTL or
leader election. All writers must use the same ownership key and uniquely owned
token. See [contract and O(retained claims) costs](../../docs/protocol/author-fence-v1.md).
