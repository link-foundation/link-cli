# Coordinated engines

Start a durable etcd deployment with its v3 HTTP JSON gateway enabled. For a
local disposable demonstration, `etcd --data-dir ./demo-etcd` listens on port
2379. Production deployments require TLS, access controls, independent durable
member disks, backups, and an appropriate odd-numbered membership. Never set
`--unsafe-no-fsync`. The application must not write the coordinator prefix
outside the engine protocol.

Run either example against the same namespace:

```sh
cargo run --manifest-path rust/Cargo.toml --features etcd --example cluster -- http://localhost:2379 /demo/
dotnet run --project examples/cluster-csharp -- http://localhost:2379 /demo/
```

Each example selects shards `a` and `b`, creates a row if absent, names both rows,
and commits both journals atomically. A concurrent change to either selected
shard rejects the entire stale transaction. Reopen a new transaction to retry
application work. A network error after sending commit has an unknown outcome:
read a fresh snapshot to discover the result before deciding whether to retry.
An independent address `1` in shard `a` is distinct from address `1` in shard `b`.

See [deployment and costs](../../docs/ENGINE-CLUSTERS.md) and the
[three-member failure experiment](../../experiments/issue-138/etcd-cluster.mjs)
for actual leader loss, majority loss, dropped responses, and language interop.
