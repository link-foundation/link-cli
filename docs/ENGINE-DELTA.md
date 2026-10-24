# Optional delta journals

`DeltaLog` (Rust), `DeltaCommitLog` (C#) and `DeltaCommitLog` (JS/TS) decorate
the existing synchronous commit-log contract. The engine still commits a complete
logical revision. This optional adapter stores a smaller physical record when a
single changed span is shorter than the full revision. Bare stores and engines
without the adapter pay no compression cost.

```rust
let physical = FileTransitionLog::open_exclusive(path)?;
let log = DeltaLog::new(physical, 128)?;
let engine = Engine::<u32, _>::open(log, EngineOptions {
    require_durable_flush: true, ..Default::default()
})?;
```

```csharp
using var physical = new FileCommitLog(path);
var engine = new LinksEngine<uint>(new DeltaCommitLog(physical, 128),
    new() { RequireDurableFlush = true });
```

```js
const physical = new FileCommitLog(path); // separate engine-file-log entry point
const engine = new WasmEngine(binding, { requireDurableFlush: true },
  new DeltaCommitLog(physical, { checkpointInterval: 128 }));
// Close sessions and engine before physical.close().
```

The interval is a positive u32. There are at most interval minus one consecutive
delta records between checkpoints; 1 writes only complete records. A larger
delta always falls back to a complete record. Reopening accepts any valid mix of
checkpoints and deltas, independently of the newly selected interval.

Compose `Engine(Delta(Replicated(primary, followers)))` to compress once and
replicate identical physical bytes. Alternatively,
`Engine(Replicated(Delta(primary), [Delta(follower)]))` compares logical prefixes
and lets each physical log choose its checkpoint interval independently. Both
orders retain the replication decorator's acknowledgement and failure semantics.
One writer must own and fence each physical log. Durability capability passes
through unchanged; the decorator adds no flush or ownership mechanism. Cluster
journal stores use a different atomic multi-key interface, and cannot be wrapped
by this single-stream adapter.

Always reopen a physical delta stream **through the decorator**. A raw engine
rejects its delta records. Append or flush errors fence the decorator: the write
may have happened, so create a fresh decorator and engine over the same physical
log to discover the outcome. Complete corrupt records fail closed. A failed
reread requires successful revalidation before another append. File adapters
retain their existing incomplete-final-line repair. Checksums detect accidental
corruption and do not authenticate writers.

## Portable physical format

The [independently generated vectors](protocol/engine-delta-vectors.json) are
consumed by native Rust, C#, JS unit tests and actual Rust/WASM integration.
Full checkpoints use canonical `engine1` records: decimal fields have no leading
zeros, and checksums are lowercase. Existing engines emit this representation;
the decorator rejects noncanonical forms even where a legacy full-record decoder
could accept them. A delta is:

```text
delta1|BASEHASH|REVISION|NEXT|PREFIX|SUFFIX|MIDDLE|NEWHASH|CHECKSUM
```

All characters must be printable ASCII. `BASEHASH` is the immediately preceding
logical full record's 16-digit lowercase FNV-1a-64 checksum. `REVISION` is a
positive u64 and `NEXT` a positive u128, both canonical decimal. `PREFIX` and
`SUFFIX` are canonical unsigned u32 decimals describing nonoverlapping spans
of the preceding full record's **row payload**, excluding its envelope. Name
payloads are already UTF-8 hex, so byte and character offsets coincide.
Reconstruct the new row payload as old prefix + `MIDDLE` + old suffix.
`NEWHASH` must verify the reconstructed
`engine1|REVISION|NEXT|ROWS` record. `CHECKSUM` is lowercase FNV-1a-64 over every
preceding field and separator in the physical delta. Validate the envelope and
span bounds before slicing. The first logical revision is 1; subsequent revisions
must increase by exactly one. The engine also validates rows, allocator
monotonicity, address range and configured constraints.

## Costs and runnable evidence

Compression scans the old and new row payloads for the longest common prefix,
then a nonoverlapping common suffix, capped at u32 spans. It retains one full
record and constructs one candidate delta: O(N) time and transient space. The
engine still clones/encodes full states. Reading expands every retained revision
and remains O(total logical history); checkpointing does not compact history or
implement an LSM tree. Distant edits can copy the large span between them.

Run the existing examples with `--delta` and use the flag every time that journal
is reopened:

```sh
cargo run --manifest-path rust/Cargo.toml --example engine_strategies -- /existing-directory/delta.log --delta
dotnet run --project examples/engine-csharp -- /existing-directory/delta.log --delta
node js/client/examples/engine.mjs /existing-directory/delta.log --delta
```

Stop each example before starting the next. The native advisory locks and JS
writer-directory lock do not fence one another; mixed-language deployments need
one external writer fence. The universal JS adapter has no Node imports.

After building `js/pkg`, run the finite probe with
`node --max-old-space-size=256 experiments/issue-138/journal-amplification.mjs`.
For 1,000 rows and ten edits to one row, full records used 109,276 bytes and deltas
used 10,697 (90.21% fewer); the last update was 9,936 versus 77 bytes. Both
reopened with identical logical history. This measures record bytes, excluding
newline framing, filesystem overhead, latency and cluster network traffic.
See the [source study and experiment plan](case-studies/issue-138/DELTA-PLAN.md).
