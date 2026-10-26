# Optional shared immutable engine state

Select the engine's in-memory rows independently of concurrency, isolation,
history, constraints, journal encoding and replication. Existing options default
to `Flat`, which uses the existing sorted map. Bare stores acquire no engine
state. `Shared` uses a persistent ordered tree: a candidate retains the immutable
root and replaces paths to changed entries. Earlier snapshots remain immutable.

```rust
use link_cli::engine::{Engine, EngineOptions, MemoryLog, StateStructure};
let engine = Engine::<u64, _>::open(MemoryLog::default(), EngineOptions {
    state_structure: StateStructure::Shared,
    ..Default::default()
})?;
```

```csharp
var engine = new LinksEngine<ulong>(new MemoryCommitLog(), new() {
    StateStructure = StateStructure.Shared
});
```

```javascript
const engine = new WasmEngine(binding, { stateStructure: 'shared' });
```

The Rust-backed JS/TS/WASM option accepts `flat`, `shared` and independently
selected [immutable sorted runs](ENGINE-SORTED-RUNS.md); unknown values
fail before opening the engine. C# also rejects undefined enum values. Sessions
keep the same links/names API, visibility, conflict validation, rollback and
unknown-commit behavior. The choice changes allocation and update costs, without
adding a worker, queue, timer or new durability guarantee.

## Composition and compatibility

All representations default to complete `engine1` revisions; independently
selected [change records](ENGINE-CHANGE-RECORDS.md) encode changed rows. Existing
journals reopen under any choice. Full-snapshot shared recovery compares each
decoded revision with its predecessor and reuses unchanged entries. Change-record
recovery retains the preceding root and applies changed paths directly.

History retention and active snapshot pinning remain separate choices. A
read-only commit publishes no new root. Rollback discards the candidate. A
failed append or flush preserves previously pinned snapshots and fences the
engine until reopening discovers the journal's actual committed prefix.

Each branch selects its own representation. Forking between shared engines can
retain the existing root; a fork to the other representation converts the rows.
Merge applies changed rows to the parent's selected map. Branch conflict checks,
allocator recovery, saved merge bases and constraints retain their existing
behavior. Group preparation uses the same choice for every accepted revision,
while retaining separate outcomes and the shared flush boundary.

Rust `Snapshot.get` and `Snapshot.iter` borrow entries directly. `Snapshot.links`
allocates the returned link vector. The existing `Snapshot.rows() -> &BTreeMap`
API remains available: the first call on a shared snapshot materializes O(N)
rows and names, cached for that snapshot and its clones. Mutated candidates have
their own empty cache. Internal engine reads, validation, encoding, recovery,
branch operations and WASM export use borrowed iteration and do not request this
compatibility map. Use `iter` for allocation-sensitive native consumers.

C# `EngineSnapshot.Rows` keeps its `IReadOnlyDictionary` interface. Shared
snapshots expose an immutable sorted dictionary; its `SetItem` and `Remove`
methods return separate dictionaries. Flat snapshots retain a read-only wrapper.
JS snapshots are serialized values and retain their existing boundary copies.

## Selected costs

Let N be the number of rows, W the staged writes, H retained revisions, and B
the encoded row/name bytes. These costs describe the in-memory map; the complete
journal encoder and optional constraints have their own costs.

| Operation | Flat, default | Shared, selected |
| --- | --- | --- |
| Snapshot handle / pin | O(1) | O(1) |
| Candidate root | O(N) row copies | O(1) root sharing |
| Apply W writes to candidate | O(W log N) | O(W log N) path copies and changed entries |
| Scan / name lookup | Copies a complete candidate, then scans | Shares a root, overlays W writes, then scans |
| Retain H small-edit revisions | Up to O(HN) map entries | Initial O(N), then O(W log N) additional paths per revision |
| Encode a default snapshot revision | O(B) | O(B) |
| Recover a snapshot revision | Complete decode | Complete decode/comparison plus changed paths |
| Encode selected change records | Changed addresses/bytes | Changed addresses/bytes |
| Recover change records | Full candidate copy, then changed rows | Retain root, apply changed paths |
| First native Rust `rows()` | Existing borrowed map | O(N) compatibility copy, including row names |

Rust uses the maintained MIT-licensed `rpds` ordered Arc-based map. Unchanged
entries retain their allocation identities. C# uses .NET's immutable sorted
dictionary; frozen AVL nodes share unchanged subtrees, and strings remain
immutable references. Both components use logarithmic tree paths. The runtime
choice has fixed map-dispatch/candidate metadata even in flat engines; the table
describes scaling costs, not identical total allocation to older engine builds.
Bare stores do not construct these engine maps. Shared maps add reference-counted
nodes and indirection; for tiny or write-heavy disposable
states, the flat choice can be cheaper. No timing advantage is asserted without
a workload measurement.

Default complete journal serialization and selected constraint scans remain O(N/B).
Choose [change records](ENGINE-CHANGE-RECORDS.md) and
[indexed constraints](ENGINE-CONSTRAINT-INDEX.md) independently to avoid those
complete visits during small local commits.
The delta decorator reduces physical bytes through a complete-record comparison;
it does not turn this engine into a page WAL or LSM tree. Group preparation also
buffers selected accepted records and copies history/conflict metadata. Sharing
does not remove those costs or add physical torn-page repair.

## Runnable checks and evidence

Add `--shared` to the existing examples, independently of `--changes`, `--delta`, `--read-set`
and `--pessimistic`:

```sh
cargo run --manifest-path rust/Cargo.toml --example engine_strategies -- --shared
dotnet run --project examples/engine-csharp -- --shared
node js/client/examples/engine.mjs --shared
```

The JS example uses the usual generated `js/pkg` binding; `CLINK_WASM` may select
an independently generated web-package directory. Supply an exclusive journal
path to each example to verify reopening. The journal representation can change
on reopening without changing its records.

Finite allocation controls use 256 named rows and sixteen single-row commits.
The prior flat engine shares zero of 4,080 unchanged Rust row references; selected
shared state retains all 4,080, while the default continues to share zero.
C# controls count actual immutable component nodes and require unchanged
subtrees across live and replayed revisions. Neither check relies on timing. The reusable native control is
[`state-copy-probe`](../experiments/issue-138/state-copy-probe/README.md).
Independent 96-step row/name oracles retain every earlier snapshot, and portable
contracts cover admission/isolation choices, failures, constraints, history,
mixed branch choices, group commits and existing wire vectors. Actual-WASM and
Node/Bun/Deno checks exercise the same option through the JS boundary.

See [the work plan](case-studies/issue-138/SHARED-STATE-PLAN.md) and
[pinned component audit](case-studies/issue-138/RESEARCH.md) for exact source,
verification progress and remaining issue criteria.
