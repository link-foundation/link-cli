# Optional immutable sorted-run state

Select `StateStructure::SortedRuns` in Rust, `StateStructure.SortedRuns` in C# or
`stateStructure: 'sorted-runs'` in JS/TS/WASM. The engine keeps its existing links,
names, session, snapshot and branch API. Flat remains the library default;
[shared trees](ENGINE-SHARED-STATE.md) remain available independently.

```rust
use link_cli::engine::{Engine, EngineOptions, MemoryLog, StateStructure};
let engine = Engine::<u64, _>::open(MemoryLog::default(), EngineOptions {
    state_structure: StateStructure::SortedRuns,
    ..Default::default()
})?;
```

```csharp
var engine = new LinksEngine<ulong>(new MemoryCommitLog(), new() {
    StateStructure = StateStructure.SortedRuns
});
```

```javascript
using engine = new WasmEngine(binding, { stateStructure: 'sorted-runs' });
```

Each effective point edit starts a sorted one-entry run. An occupied tier merges
into the next tier, like a binary carry. Equal keys retain the newer value.
Lower tiers always cover newer edits than higher tiers. A deletion tombstone
terminates lookup, hiding older values; compaction removes tombstones only when
no older run remains. Pinned snapshots keep their original immutable arrays.
No-op writes preserve the runs, while an accepted engine write still retains
the existing revision and allocator semantics.

This policy groups edit counts, rather than current live byte sizes. It is a
small in-memory alternative inspired by sorted-run designs. RocksDB additionally
provides memtables, on-disk SSTables, a WAL, filters and configurable background
compaction. Its [universal compaction policy](https://github.com/facebook/rocksdb/wiki/Universal-Compaction)
uses different size and run selection constraints. Selecting this representation
does not create those physical storage components.

## Costs and composition

Let M be the number of effective edits, D the entries retained across current
runs, N the live rows and K the number of occupied tiers, at most O(log M).
These are representation costs, excluding the engine's selected guarantees.

| Operation | Selected sorted runs |
| --- | --- |
| Snapshot handle / pin | O(1), retaining its existing revision |
| Candidate root in Rust | O(log M) shared array handles; no row/name copies |
| Candidate root in C# | O(1) reference to an immutable run container |
| Point lookup | Binary search each newest-to-oldest run, O(K log D) upper bound |
| Effective point write | Copy tier handles and merge occupied lower tiers; a carry can rewrite all retained entries |
| First ordered scan in a revision | Materialize an ordered map, O(D log D) time, up to O(D) temporary entries and O(N) retained rows |
| Later ordered scans of that revision | Reuse its materialized view, then O(N) iteration |
| Retain older revisions | Share unchanged arrays; retain replaced arrays and any materialized views until their owners release them |
| Representation conversion | Visit rows to construct the receiving representation |

Rust merges clone older values, including owned name strings. C# copies value
entries and retains immutable string references. Rust candidate clones start
with an empty scan cache; C# retains the same immutable container until an
effective edit. Ordered engine encoding, scans, name lookup, default constraint
validation and Rust's compatibility `rows()` can trigger materialization.
There is no constant-time-write or general throughput improvement claim.
The representation choice adds fixed dispatch/container metadata even in flat
engines. Bare stores do not construct engine row containers.

Admission, isolation, serializable validation, history, constraints, logical
[change records](ENGINE-CHANGE-RECORDS.md), physical delta encoding and log
providers remain independently selected. Complete snapshot encoding and replay
still visit complete records. Selected change records apply changed addresses
to the previous runs. Branches may select any of the three representations;
merge uses the parent's choice. Group commits preserve separate revisions and
their existing shared-flush boundary. A failed append/flush preserves pinned
views and requires reopening to discover the actual durable prefix.

## Runnable persistence example

Build the [native page examples](ENGINE-NATIVE-PAGES.md), then use a dedicated
database path and a compiled web WASM package:

```sh
rust/target/debug/examples/engine_native_pages example-directory/journal --sorted-runs
dotnet examples/native-pages-csharp/bin/Debug/net10.0/NativePagesExample.dll example-directory/journal --sorted-runs
CLINK_WASM="$PWD/js/pkg" node js/client/examples/engine-native-pages.mjs example-directory/journal --sorted-runs
```

Each invocation verifies prior exact names, commits two sessions, checks the
pinned predecessor and closes its owned files. Bun and Deno accept the same
example. Add `--snapshot`, `--delta`, `--indexed-constraints` or `--mirrored`
independently; retain the same physical encoding and mirror mode on reopening.
The interchange script includes all three representations in 48 configurations.

Finite map oracles, immutable-array identity controls, independent replay vectors,
branch/concurrency/fault matrices and actual-WASM execution verify this selected
algorithm. See [the plan and evidence](case-studies/issue-138/SORTED-RUNS-PLAN.md).
Physical SSTables, legacy mapped-file repair, background disk compaction and
physical power-loss certification remain unfinished requirements of issue 138.
