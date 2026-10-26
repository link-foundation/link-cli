# Optional retained-history checkpoints

Select checkpoint publication independently of state structure, logical encoding,
history, constraints, physical journals, replication and object caching:

```rust
let cluster = ClusterEngine::<u32, _>::new(store, options)?
    .with_journal_checkpoint_interval(64)?;
```

```csharp
var cluster = new ClusterEngine<uint>(store, options,
    journalCheckpointInterval: 64);
```

```js
const cluster = new ClusterEngine(binding, store, {
  history: 2, revisionEncoding: 'changes', journalCheckpointInterval: 64,
});
```

The default is disabled. Intervals must be positive; JS accepts safe integers.
Every changed shard whose committed revision is divisible by the selected
interval publishes a checkpoint journal through the selected atomic store. A
group still compares every selected shard and publishes all changes atomically.
Read-only groups do not compact. The existing native and JS cluster examples
accept `--checkpoint` to select interval one for a short reproducible run.

With history disabled, interval K limits the current journal to K records after
the first checkpoint. Last N limits it to N + K - 1 records. All deliberately
retains all snapshots and does not bound history. These are record bounds, not
byte bounds: each checkpoint contains complete retained snapshots, whose names
and rows may grow. A checkpoint can increase bytes compared with changed-row
records. Export runs synchronously and has a cost proportional to retained
snapshot contents. Default commits perform no checkpoint export.
Selected history retains one committed allocator value per snapshot so saved
branch merge bases keep their exact metadata after export. History-disabled
owners use the existing current allocator value. A branch base that has been
evicted cannot be recovered from that checkpoint.

The [finite payload example](../examples/journal-checkpoints/measure.mjs) uses the
actual compiled WASM engine, all three state structures, both encodings and three
history policies for 32 revisions. It reports current record count and UTF-8 JSON
journal bytes read/published for disabled versus interval-four publication.
It measures logical payloads, not network framing, disk I/O or latency.

Partitioned stores publish a new immutable object and atomically change catalog
pointers. Old objects remain immutable; this feature does not reclaim them or
provide reader leases. The catalog remains the atomic publication boundary.
Local append-only file/page journals are unchanged; callers can separately export
an independent checkpoint via the public engine APIs. See the
[wire and ownership contract](protocol/engine-checkpoints.md).
The physical delta decorator accepts an initial engine3 record unchanged, then
enforces consecutive engine1/engine2 revisions. Complete checkpoint rows can be
the base for the next physical delta1; logical changes interrupt that dependency.
A later engine3 is rejected even when its revision is consecutive. Either
replication order works: place delta outside replication to copy identical
physical records, or inside each replica to compare logical records with
independently selected compression intervals. See the
[finite composition example](../examples/journal-checkpoints/delta.mjs).

This reduces current replay and transfer histories when retention is bounded.
It does not remove full-snapshot checkpoint cost, catalogue contention or the
need for safe object retention. Durability still depends on the selected atomic
store and truthful synchronization; logical checkpoints do not certify physical
power-loss behavior.
