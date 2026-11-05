# Composing links networks (issue #149)

A client's own links joined with a server's, the server's permission rules
applied to what the client reads and writes there, and the whole union read
under UUIDs. [docs/COMPOSITION.md](../../docs/COMPOSITION.md) explains the
layers, their order, and the same composition in C# and Rust.

`client-and-server.mjs` starts three `clink` processes through
`@link-foundation/link-cli-client`:

| Store | Role |
|-------|------|
| `server.links` | The world of [examples/permissions](../permissions), served with `--permissions self --tokens tokens.links`. The connection sends `(auth: alice-secret)` and is `alice` from then on. |
| `local.links` | The client's own links. |
| `ids.links` | The UUIDs given to the links of both, a table of `LinksExternalIds` apart from the data. |

```js
const links = new UnionLinks([local, new RemapLinks(server, new OffsetAddressMap(1n << 24n))]);
const byUuid = new RemapLinks(links, new ExternalIdMap(ids, 'uuid'));
```

- The union reads the client's links and the server's links that alice may
  read. The server's links are moved by 2^24 (`OffsetAddressMap`), so neither
  store shadows the other.
- A new link goes to the first member, the client's store. It may refer to a
  link of the server by its address in the union.
- A write to a server link goes to the server, whose rules refuse what alice
  may not write. The error names the address on the server (`update 14`).
- `RemapLinks` over an `ExternalIdMap` reads every link under a UUID and
  gives one to each link it sees first. The example passes a counter as
  `newId` so its output is stable; by default it calls `crypto.randomUUID()`.

```sh
cargo build --manifest-path rust/Cargo.toml --bin clink
node examples/composition/client-and-server.mjs rust/target/debug/clink

dotnet build csharp/Foundation.Data.Doublets.Cli
node examples/composition/client-and-server.mjs csharp/Foundation.Data.Doublets.Cli/bin/Debug/net10.0/clink.dll
```

Both servers print [expected-output.txt](expected-output.txt). The JS
client CI runs the example with both and compares the output.
