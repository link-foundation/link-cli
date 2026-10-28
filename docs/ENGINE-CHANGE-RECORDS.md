# Optional changed-row journals

Select changed-row serialization independently of the in-memory map. Existing
settings write complete snapshots; `Changes` writes only rows whose final value
differs from the preceding revision, plus revision/base/allocator metadata.
Transactions retain the same links/names API and commit semantics.

```rust
use link_cli::engine::{Engine, EngineOptions, MemoryLog, RevisionEncoding, StateStructure};
let engine = Engine::<u64, _>::open(MemoryLog::default(), EngineOptions {
    revision_encoding: RevisionEncoding::Changes,
    state_structure: StateStructure::Shared,
    ..Default::default()
})?;
```

```csharp
var engine = new LinksEngine<ulong>(new MemoryCommitLog(), new() {
    RevisionEncoding = RevisionEncoding.Changes,
    StateStructure = StateStructure.Shared
});
```

```javascript
const engine = new WasmEngine(binding, {
  revisionEncoding: 'changes', stateStructure: 'shared'
});
```

Both choices reopen existing and mixed journals. Unknown options fail before
opening. Each fork chooses its own encoding; saved merge bases remain standalone
complete snapshots. Group commits prepare consecutive independent change
records, append accepted members and flush once before publishing. Append/flush
errors still produce indeterminate outcomes, preserve previous pinned views and
fence the engine until reopening discovers its actual committed prefix. No new
durability guarantee follows from selecting a smaller representation.

For W staged addresses among N rows and C changed row/name bytes, change encoding
sorts/deduplicates W addresses and compares their previous/final rows:
O(W log W + W log N + C), plus row/name equality comparisons. Its record buffers
contain changed bytes rather than every row. Shared recovery retains the previous
root and updates O(W log N) paths; flat recovery still copies N rows. Full
snapshot encoding remains O(total row/name bytes). Selected constraints, scans,
branch comparisons, conflict/history metadata copies, log reads and retained
journal growth retain their separate costs. The physical delta decorator passes
changes through, avoiding a second complete-payload comparison; it can still
compress consecutive complete snapshot records in a mixed stream.

Finite serialized-input controls use 256 named rows and sixteen one-row updates.
Complete encoding visits 4,096 update rows and emits 966,063 bytes; changes
encode sixteen rows and 4,246 bytes, under both flat and shared state. Old pins
and reopening retain identical untouched rows. This measures logical UTF-8
record bytes, excluding framing, storage overhead, latency and network traffic.
It makes no timing or physical power-loss claim. Run the reusable control after
building a web package:

```sh
CLINK_WASM=/absolute/path/to/web-package node --max-old-space-size=128 experiments/issue-138/revision-record-probe.mjs --changes --shared
```

Add `--changes` to each strategy example, independently of `--shared`, `--delta`,
`--read-set` and `--pessimistic`. For a persistent journal, stop each writer before
starting the next and use an external writer fence across different file adapters:

```sh
cargo run --manifest-path rust/Cargo.toml --example engine_strategies -- /existing-directory/journal --changes --shared
dotnet run --project examples/engine-csharp -- /existing-directory/journal --changes --shared
node js/client/examples/engine.mjs /existing-directory/journal --changes --shared
```

The JS example supports the usual generated `js/pkg` binding or `CLINK_WASM`.
See [the canonical protocol](protocol/engine-changes.md),
[work plan](case-studies/issue-138/CHANGE-RECORDS-PLAN.md) and
[primary source analysis](case-studies/issue-138/RESEARCH.md).
