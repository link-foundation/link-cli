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
| `subscribe(restriction?)` / `unsubscribe(n)` | the subscription number (from 1 on each connection) / nothing |
| `receiveNotification()` | the next `{ subscription, changes }` pushed, or `null` when the deadline passes first |
| `onNotification(listener)` | the function that removes the listener |
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
`ConnectionClosedError`, `AbortError`, `DeadlineExceededError`, and
`NotFoundError` (a composing layer was asked to write a link it has not got,
`not-sent`).

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

**Subscriptions** (`capabilities.subscriptions`, servers only).
`subscribe(restriction)` asks the server to push the changes that *other*
connections make to the links matching the restriction (the same patterns as
`each`). The server pushes one `(notify: N)` document per subscription per
write, followed by its `(before) (after)` changes. A side that does not match
the restriction is `null`, so a link that moves into the pattern arrives as a
creation and one that leaves it as a deletion. The transport tells pushes from
replies by their first link, `(notify: N)`. Pipelined requests still match
their replies. Pushes wait in a queue for `receiveNotification()`, or go to the
`onNotification` listeners while any are registered. The server writes every
push before any later reply on the connection, so once a reply arrives, every
push from earlier writes is already received. A connection with more than
`maxPendingPushes` (default 1024) unread pushes is closed, just as the server
disconnects a subscriber that falls too far behind. Closing the connection ends
its subscriptions.

```js
const watcher = await connectTcp('127.0.0.1:8080');
const subscription = await watcher.subscribe([ANY, ANY, ANY]);
// ... another connection runs create(0, 0) ...
const { changes } = await watcher.receiveNotification({ timeout: 5000 });
// changes: [{ before: null, after: { index: 1n, source: 0n, target: 0n } }]
await watcher.unsubscribe(subscription);
```

**Batches** pipeline up to `window` requests and stop sending after the first
failure. A batch is **not a transaction**: what ran stays applied. A batch is
verified when `completed` is `true`, or when a fresh client reads the result
back. The conformance tests and the synthetic consumer do exactly that.

**Ownership.** `close()` ends only what the client created: its own TCP
connection, and the server process it started (`connectProcess`). A borrowed
socket (`new TcpTransport(socket, { owned: false })`), a server you started
yourself (`startServer`), and a `Clink` passed without `{ owned: true }` all
outlive the client.

## Permissions

A server started with `--permissions` (and `--tokens` for authentication)
serves every connection as a subject: the default `--subject`, or the one a
token authenticates as. Send the token first, as raw protocol text:

```js
const links = await connectTcp('127.0.0.1:7878');
console.log(await links.requestText('(auth: alice-secret)')); // (subject: 11)
```

From then on, reads skip what the subject may not read, and writes it may not
make reject with `RemoteError` (`Forbidden: update 14`). `getByName` of a link
the subject cannot read resolves to `null`. The package has no permission
layer of its own: the rules are the server's. See
[examples/permissions](../../examples/permissions).

## Composing networks

These layers are links clients over links clients, so they wrap any backend
or each other. The C# and Rust ports have the same layers;
[docs/COMPOSITION.md](../../docs/COMPOSITION.md) describes how they compose.

| Export | What it does |
| --- | --- |
| `UnionLinks([a, b, ...])` | Many networks seen as one. The first member that has an address shadows the rest, a new link goes to the first member, and a write goes to the member that has the link (`ownerOf(index)`). Names are found in member order. Raw text, queries and subscriptions go to a member: `union.members[i]`. |
| `RemapLinks(links, map)` | A network under other addresses, both ways. A read of a foreign address finds nothing; a write naming one rejects before sending. `toOuter(inner)` and `toInner(outer)` translate by hand. |
| `OffsetAddressMap(offset)` | Moves a network by `offset`, so a union of remapped networks shadows nothing. |
| `AddressTable({ first, newId })` | Gives addresses (from `first`) or ids (from `newId()`) in the order they are seen. It lives in memory. |
| `LinksExternalIds(scheme)` | The ids of another system bound to links, stored as links: `find`, `idOf`, `bind`, `ensure`, `unbind`, each taking the store of the table. |
| `ExternalIdMap(table, scheme, { newId })` | A `RemapLinks` map that reads a network under those ids (random UUIDs by default) and keeps them in `table`. |

```js
import { ExternalIdMap, OffsetAddressMap, RemapLinks, UnionLinks } from '@link-foundation/link-cli-client';

// The client's own links first, the server's moved out of their way.
const links = new UnionLinks([local, new RemapLinks(server, new OffsetAddressMap(1n << 24n))]);
const draft = (await links.create(0n, 0n)).index;     // in local
await links.update(draft, draft, await links.getByName('news')); // refers to the server

// The same network under UUIDs, kept in a store of their own.
const byUuid = new RemapLinks(links, new ExternalIdMap(ids, 'uuid'));
console.log(await byUuid.each([]));                   // indexes are UUID strings
```

[examples/composition](../../examples/composition) runs this against a server
with permissions, with both CLIs.

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
| `subscriptions` | yes: `(notify: N)` pushes | yes | no: an in-process store has no other connection |
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

The conformance runner replays `docs/protocol/links-operations.txt` and, over
two connections, `docs/protocol/subscriptions.txt`, the golden conversations
that the Rust and C# test suites also replay. It covers
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
