# Serializable validation choices

Choose validation independently of memory/file/delta/replicated journals, history
retention and constraints. The default remains whole-revision validation. Bare
stores keep their existing behavior and pay no session overhead.

| Surface | Default | Point-read validation |
| --- | --- | --- |
| Rust `EngineOptions` | `SerializableValidation::Revision` | `serializable_validation: SerializableValidation::ReadSet` |
| C# `EngineOptions` | `SerializableValidation.Revision` | `SerializableValidation = SerializableValidation.ReadSet` |
| JS/TS `WasmEngine` | `serializableValidation: 'revision'` | `serializableValidation: 'read-set'` |

Both choices use serializable begin-time snapshots plus own writes. Selecting
read sets with read-committed or snapshot isolation is rejected before opening
the journal. Exclusive admission still fences concurrent sessions and merges;
it needs neither read sets nor row generations.

## Independent profile edits

Seed two rows, then begin sessions A and B at the same revision. A reads/edits
row 1; B reads/edits row 2. Revision validation aborts B after A commits.
Read-set validation lets both commit. If A also reads row 2 and B reads row 1,
A's commit invalidates B's read and B aborts: the write-skew cycle cannot commit.
Retry with a new session; a conflict closes the old one without publishing writes.

`get_link`/`GetLink` and name-by-address reads track addresses, including absent
rows. A later insert, delete/recreate, name change or branch publication at that
address invalidates a stale reader even if the final value equals its old value.
Mutations validate their written addresses too. Reads after a session closes
preserve the existing API but do not allocate new dependencies.

Scans, counts, queries and name-to-address lookups conservatively observe the
whole revision. Any intervening commit invalidates them, including queries with
an address restriction. This prevents phantoms without a range index, at the
cost of rejecting unrelated edits. Read-only point transactions validate their
reads; an empty read set can commit despite unrelated changes. Out-of-session
engine snapshots are immutable inspection values, not transaction dependencies.

## Cost and composition

Selected optimistic read-set sessions keep O(R) distinct observed addresses,
with O(log R) Rust set insertion and expected O(1) C# hash-set insertion. Rust
locks selected read sets for shared point reads, preserving the session type's
existing Send/Sync bounds; unselected sessions have no read-set mutex. Commit
checks each observed address against the change-generation map while holding
the publication mutex. The map tracks changed/deleted addresses only while other
sessions can be stale and is reclaimed after the last session validates. A
continuously overlapping population can keep many generations alive; there is
no oldest-reader pruning yet. Default revision validation tracks no point reads
and uses one revision comparison. Exclusive read-set selection skips both maps.

The implementation still clones O(N) rows and encodes a complete logical
revision per write commit. Optional delta encoding changes physical journal
bytes, not these CPU/memory costs. There is no per-key lock, range index,
FoundationDB resolver, SSI dependency graph or distributed read-set protocol.
`ClusterEngine` separately validates all selected shard generations; local
read-set selection does not narrow that distributed conflict boundary.

The design draws on FoundationDB's documented read/write conflict footprints;
[pinned source observations](case-studies/issue-138/RESEARCH.md) record the seams
studied and differences. Tests include a separate bounded serial-history oracle:
216 read/write profiles times six commit orders in each native implementation.
For acknowledged transactions, it enumerates serial executions and checks both
observed values and final rows. These 1,296 histories are finite evidence, not a
proof over arbitrary schedules or hardware failures.

## Run

After building the usual native/WASM artifacts:

```sh
cargo run --manifest-path rust/Cargo.toml --example engine_strategies -- --read-set
dotnet run --project examples/engine-csharp -- --read-set
node js/client/examples/engine.mjs --read-set
node js/client/examples/serializable-points.mjs
```

Add a journal path and `--delta` to compose the existing durable/delta examples.
Use one fenced writer per journal and the deployment rules in
[ENGINE-STRATEGIES.md](ENGINE-STRATEGIES.md).
