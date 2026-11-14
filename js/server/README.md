# @link-foundation/link-cli-server

The Rust `clink` in pure JavaScript: the same command line, the same output,
errors and exit codes, and the same files on disk, as an executable, as an
embedded store and as a server. It has no WebAssembly and no native code, and
it runs on Node.js 20 and later, Bun and Deno, on Linux, macOS and Windows.

The client library, [`@link-foundation/link-cli-client`](../client/README.md),
talks to a `clink` server. This package **is** a `clink`: it opens a store in
your process, or serves one to clients over TCP, IPC or GraphQL.

| Use | Entry point |
| --- | --- |
| Command line | `clink` (`bin/clink.js`), or `main(args, io)` in-process |
| Embedded store | `NamedTypesDecorator.open(path)` and `QueryProcessor` |
| Transactions, version control, triggers, permissions | the decorators of `src/index.js` |
| Protocol server and client | `LinksServer`, `LinksClient`, `Endpoint` |
| GraphQL endpoint for Deep | `GraphqlServer`, or `clink --serve ADDR --graphql` |

## Install

```bash
npm install @link-foundation/link-cli-server
npx clink --help
```

From a checkout of this repository: `cd js/server && npm ci`, then
`node bin/clink.js --help`.

## Command line

Every option of the Rust `clink` (`clink --help` prints the same text):

```bash
clink --db family.links '() ((child: father mother))' --auto-create-missing-references --changes --after
clink --db family.links --structure 3
clink --db family.links --transactions '((child: father mother)) ()' --changes
clink --db family.links --vc --tag v1 --list-tags
clink --db family.links --serve 127.0.0.1:7878            # the clink protocol over TCP
clink --serve unix:/tmp/links.sock --forward 127.0.0.1:7878 # a relay
clink --connect 127.0.0.1:7878 '() ((1 1))'                 # a protocol client
clink --db family.links --serve 127.0.0.1:8080 --graphql    # GraphQL for Deep
```

Like the Rust `clink`, it reads `.lenv` and then `.env` from the current
directory before it parses the command line, and never overwrites a variable
that is already set (`src/env-files.js`).

## Embedded store

A store is two files, `family.links` and its names `family.names.links`,
in the format of the Rust `doublets` unit store, so the Rust `clink` and this
package can open each other's stores:

```js
import { NamedTypesDecorator, QueryProcessor } from '@link-foundation/link-cli-server';

const store = NamedTypesDecorator.open('family.links');
const processor = new QueryProcessor().withAutoCreateMissingReferences(true);
const changes = processor.processQuery(store, '() ((child: father mother))');
for (const [before, after] of changes) store.printChangeLino(before, after, process.stdout);
store.printAllLino(process.stdout); // (father: father father) (mother: ...) (child: father mother)
console.log(store.getByName('child'), store.getName(3)); // 3 child
store.save();
```

`LinkStorage.openExclusive`, `openShared` and `tryOpenExclusive`, and
`FileTransitionLog.openExclusive`, hold the advisory `<db>.lock` sidecar lock
of the Rust `clink` (`FileLock`), so a JavaScript and a Rust process that
share a store exclude each other.

### Transactions

`TransactionsDecorator` logs every write to `<db>.transitions.links` and runs
crash recovery when it opens: a transaction that never committed is rolled
back.

```js
import { NamedTypesDecorator, TransactionsDecorator } from '@link-foundation/link-cli-server';

const open = () =>
  new TransactionsDecorator(
    NamedTypesDecorator.open('db.links'),
    NamedTypesDecorator.open(TransactionsDecorator.makeTransitionsDatabaseFilename('db.links')),
  );
const store = open();
store.beginTransaction();
store.create(1, 1);
store.commit();
store.beginTransaction();
store.create(2, 2);
store.rollback();
store.save();
console.log(open().all().map((link) => [link.index, link.source, link.target])); // [[1, 1, 1]]
```

`VersionControlDecorator` adds branches, tags and checkout on top of it,
`PersistentTransformationDecorator` the `--always`/`--once` triggers,
`PermissionsDecorator` the permission policies and tokens, and
`TripletsOverDoublets` triplets.

## Serving

```js
import {
  LinksClient,
  LinksServer,
  NamedTypesDecorator,
  TextLinoProtocol,
} from '@link-foundation/link-cli-server';

const store = NamedTypesDecorator.open('db.links');
const server = await LinksServer.listen('tcp:127.0.0.1:0');
const stop = server.shutdownHandle();
const serving = server.serve(store);

const client = await LinksClient.connect(server.endpoint(), new TextLinoProtocol());
console.log(await client.queryText('() ((1 1))')); // () ((1: 1 1))
client.stream().shutdown();

stop.shutdown();
await serving;
store.save();
```

The endpoints are those of the Rust `clink --serve` (see
[the client's IPC table](../client/README.md#ipc-endpoints)):

| Kind | Listen | Connect |
| --- | --- | --- |
| `tcp`, `udp`, `pipe`, `mmap`, `file` | everywhere | everywhere |
| `unix`, `fifo`, `signal` | Linux, macOS | Linux, macOS |
| `shm` | Linux | Linux |
| `stdio` | everywhere | no |
| `exec` | no | everywhere |
| `mq`, `sysvmq`, `mailslot`, `copydata`, `alpc` | no | no |

`register(kind, connect, listen)` of `src/ipc.js` adds a kind of your own.
`pipe:` is a Windows named pipe on Windows and a FIFO elsewhere, as in Rust.

## Differences from the Rust `clink`

- `clink --version` prints the version of this package.
- No POSIX message queue (`mq`) or System V queue (`sysvmq`) endpoints, and
  none of the Windows-only kinds: Node.js, Bun and Deno have no API for
  them.
- `FileLock` needs a runtime with a foreign function interface: Deno, Bun,
  or Node.js 26 and later (`node:ffi`). On an older Node.js, taking a lock
  fails with a `LockError` that says so, and never pretends to hold it.
  The command line itself does not lock, as in Rust.
- I/O error texts use the POSIX wording of the Rust standard library on
  every platform.

## Debugging

- `clink --trace` prints every step of the store, the transactions layer
  and each transport event of `--serve`, `--connect` and `--forward`.
- `CLINK_IPC_TRACE=1` turns the transport trace on without `--trace`.
- `CLINK_DIFFERENTIAL_DEBUG=1` makes the differential tests print each
  invocation and both outputs.

## Development

```bash
npm ci
npm test                     # unit tests (Node.js)
npm run test:coverage        # with 100 % line, branch and function coverage
bun test --timeout 60000 test/
deno test --allow-all --no-check test/
CLINK_RUST=../../rust/target/release/clink npm run test:differential
```

The differential tests run each invocation through the Rust `clink` and
through `bin/clink.js`, each in its own sandbox, and compare standard output,
standard error, the exit status and every file written.
`node test/conformance/run.mjs` in `js/client`, with
`CLINK_JS=js/server/bin/clink.js`, runs the client's conformance scenarios
against this package's `clink --serve`. CI runs all of them in
[`js-server.yml`](../../.github/workflows/js-server.yml), and the unit tests
also on Linux, macOS and Windows with Node.js 20, 22 and 26.

Every feature's `javascript` cell in
[`parity/features.json`](../../parity/features.json) points at the tests
here; see [PARITY.md](../../docs/PARITY.md) and
[JS-WORKBENCH.md](../../docs/JS-WORKBENCH.md).
