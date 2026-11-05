# DeepClient against clink (issue #153)

`run.sh` serves a database holding a fresh Deep install
([`docs/protocol/graphql/deep-fresh.json.gz`](../../docs/protocol/graphql)) and
runs the real `DeepClient` of `@deep-foundation/deeplinks` against it: it logs
in as a guest, selects, inserts, updates a value, receives a subscription
update and deletes links. It ends with `E2E OK`.

```sh
cargo build --release --manifest-path rust/Cargo.toml
examples/deep-client/run.sh rust/target/release/clink

dotnet build -c Release csharp/Foundation.Data.Doublets.Cli
examples/deep-client/run.sh csharp/Foundation.Data.Doublets.Cli/bin/Release/net10.0/clink
```

See [docs/GRAPHQL.md](../../docs/GRAPHQL.md).
