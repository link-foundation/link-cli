# Coordinated engines

The native file example requires no server or SQL provider:

```sh
dotnet run --project examples/cluster-csharp -- --native-lsm direct
dotnet run --project examples/cluster-csharp -- --native-lsm partitioned
dotnet run --project examples/cluster-csharp -- --native-lsm chunks
dotnet run --project examples/cluster-csharp -- --native-lsm reclaiming
```

Each finite example uses private temporary doublets directories, actual links
and Unicode names, coordinated retained-history checkpoints, stale group
rejection, compaction and closed-store reopening. The reclaiming composition
also verifies that retired objects are collected. Files are removed after every
handle closes. POSIX supports synchronized namespace changes; Windows reports
`SupportsDurableFlush = false`. A process-exit test does not certify physical
power-loss durability. See the [native directory contract](../../docs/protocol/native-lsm-store.md).

The `chunks` composition explicitly selects two-record immutable chunks on the
ordinary partitioned journal store. Default whole-journal writes remain available.
Its cache, staging reads, full replay and retention costs are described in the
[chunk protocol](../../docs/protocol/chunked-journal.md); chunk reclamation is a
separate unfinished combination.

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

Append east and west endpoint lists to either command to route bulk journals to
independent consensus groups. The original endpoint list then serves only the
atomic catalog. See the independent-data recipe in the deployment documentation;
partition namespaces must remain immutable and old objects are retained.
# Optional checkpoints

Add `--checkpoint` to publish retained-history checkpoints after every changed
revision using the same atomic store. Defaults remain append-only. Upgrade every
reader before enabling engine3 journals. See
[configuration and costs](../../docs/ENGINE-CHECKPOINTS.md).
