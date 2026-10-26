# Optional coordinator maintenance

`JournalMaintenanceStore` (Rust), `IJournalMaintenanceStore` (C#), and
`JournalMaintenanceStore` (TypeScript) extend the existing atomic journal
contract with a complete namespace scan and a generation-conditional delete.
The etcd adapter implements this capability in all three language surfaces.
Ordinary stores and the existing partitioned decorator retain their existing
API and costs. Maintenance codecs initialize on first use; JS loads the helper
module on demand. No scan, delete, timer or retained index runs automatically.

This is a provider capability. It supplies no object liveness rule. In particular,
listing an unreferenced object does not establish that an in-flight publisher
will never reference it. The existing `PartitionedJournalStore` continues to
retain immutable objects. Its objects must not be deleted using this API without
an independently verified publication/reader fencing protocol.

The separately opened [reclamation store](ENGINE-RECLAMATION.md) supplies epoch
fencing and coherent owned-reader refresh for dedicated new `blob2` namespaces.
Select that store explicitly when collection is required.

Use a dedicated caller-owned prefix. Every key in it must use the existing shard
name grammar: 1..128 ASCII letters, digits, dot, dash or underscore. A foreign,
malformed or unexpected key rejects the entire scan. These methods neither
acquire namespace ownership nor authorize deletion of another application's data.

```rust
use link_cli::engine::maintenance::{JournalMaintenanceStore, JournalScanOptions};
// `store` is an EtcdJournalStore configured with a dedicated prefix.
// let snapshot = store.scan_namespace(JournalScanOptions::default())?;
```

```csharp
IJournalMaintenanceStore maintenance = store;
var snapshot = maintenance.ScanNamespace(new(PageKeys: 128));
```

```js
const snapshot = await store.scanNamespace({ pageKeys: 128 });
// Only after an application independently proves that this object is retired:
// await store.compareDelete(snapshot[0].key, snapshot[0].version.generation);
```

The runnable examples require an initially empty dedicated prefix, write two
small journals, scan them one key per page, delete/recreate one key, reject its
stale deletion generation, and verify the other journal remains unchanged:

```sh
cargo run --manifest-path rust/Cargo.toml --features etcd \
  --example journal_maintenance -- http://localhost:2379 /maintenance-rust/
dotnet run --project examples/journal-maintenance-csharp -- \
  http://localhost:2379 /maintenance-csharp/
node js/client/examples/journal-maintenance.mjs \
  http://localhost:2379 /maintenance-js/
```

The JavaScript example also runs with Bun and with `deno run --allow-net`.
These HTTP operations do not require a WASM workspace; the capability can be
used alongside the existing Rust-backed JS/WASM cluster engine.

## Complete bounded scans

A scan returns owned `{key, version}` entries in ascending key order, or an
error. It never returns a partial successful result. `version` contains the
nonzero modification generation and journal records. Keys-only mode returns
empty record arrays while preserving keys/generations, avoiding journal payload
downloads. It does not mean that the underlying journal is empty.

Defaults are 4,096 keys, 8,388,608 decoded bytes and 128 keys per page. Page size
must be 1..256; the other limits must be positive finite integers. Accounted bytes
are the logical key's UTF-8 bytes, eight generation bytes, and each UTF-8 record's
bytes plus one separator. Array/string/container overhead is additional and
bounded by the number of keys and records. One page's bounded HTTP/JSON response
also exists while it is decoded; configure the adapter's existing request,
response and network deadline limits for the deployment. Raising a limit is an
explicit caller cost. No partial-result continuation token is exposed.

The first linearizable transaction captures the namespace revision. Every
subsequent range explicitly requests that same revision and starts after the
last returned key. Concurrent inserts, updates and deletes therefore cannot
mix revisions. A truncated page, inconsistent count/continuation, unordered or
foreign key, noncanonical base64/UTF-8, bad generation, malformed journal or
limit exhaustion rejects the snapshot. A generation newer than the pinned
revision is invalid. ProtoJSON's omitted zero count is accepted only for an
empty final page.

Read failover is finite, once per configured endpoint. An error, including
revision compaction, restarts the *entire* scan at the next endpoint and discards
all accumulated pages. Failure of every attempt returns an error. Each page has
the adapter's finite request deadline; the total number of pages is bounded by
`maxKeys + 1` per endpoint. There is no deadline extension or mutation retry.
Explicit ascending-key sorting may cause an etcd server to inspect the complete
remaining range; client key/byte limits do not bound the server's working set.

## Exact conditional deletion

`compare_delete(key, expected_generation)`, `CompareDelete(key, generation)` or
`compareDelete(key, generation)` sends one etcd transaction. It compares the
key's exact modification revision and deletes that single key only on equality.
Zero, noncanonical or out-of-signed-64-bit generations reject before I/O. Rust
and C# accept integer generations; JS/TS uses canonical decimal strings.

A valid rejection returns false, including when the key is absent. Acknowledged
success requires a valid response revision and exactly one single-key deletion.
A transport failure or malformed acknowledgment has an unknown outcome. Never
blindly replay that mutation; discover current state with a fresh read instead.
Deleting and recreating a key gives it a newer generation, so an old observation
cannot delete the replacement. Providers/restores must preserve the contract that
modification generations are never reused; a restored deployment that resets
identity requires a separately owned namespace and explicit migration.

The etcd implementation follows its primary
[Range and transaction API](https://etcd.io/docs/v3.6/learning/api/) and pinned
[range/count/deletion implementation](https://github.com/etcd-io/etcd/blob/v3.6.5/server/etcdserver/txn/txn.go).
Those contracts do not prove a higher-level reclamation protocol. See the
[maintenance plan](case-studies/issue-138/MAINTENANCE-PLAN.md) for capability
validation and the [reclamation plan](case-studies/issue-138/RECLAMATION-PLAN.md)
for the separately selected collector's proof obligations and evidence.
