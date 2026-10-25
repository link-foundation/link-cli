# Optional indexed constraint validation

Choose `Scan` (the default) or `Indexed` independently of constraints, isolation,
admission, history, state structure, logical encoding and physical journal:

```rust
use link_cli::engine::{Constraint, ConstraintValidation, EngineOptions};
let options = EngineOptions {
    constraints: vec![Constraint::References, Constraint::UniquePairs, Constraint::UniqueNames],
    constraint_validation: ConstraintValidation::Indexed,
    ..Default::default()
};
```

```csharp
var options = new EngineOptions {
    Constraints = new[] { EngineConstraint.References, EngineConstraint.UniquePairs, EngineConstraint.UniqueNames },
    ConstraintValidation = ConstraintValidation.Indexed
};
```

```js
const options = {
  constraints: ['references', 'unique-pairs', 'unique-names'],
  constraintValidation: 'indexed',
}; // Pass to WasmEngine, or a cluster's workspace options.
```

Both strategies enforce the same final-state rules. Zero is an absent reference;
other source/target values must address existing rows. Self-references and cycles
are valid. Pairs compare both endpoints; names compare exact strings. Missing
names do not participate; empty names do. Names are not normalized. Neither
strategy cascades deletions or merges duplicate pairs. If several constraints
fail, which error is reported first is unspecified.

## Algorithm and ownership

The engine gate protects validation and publication. Optional persistent ordered
maps hold pair-to-owner, name-to-owner and address-to-incoming-reference-count
indexes. Only selected constraints retain a map. The reference count includes
both endpoint positions, including two occurrences in the same row. Valid bases
and complete changed-address sets let a deletion detect references from unchanged
rows without scanning them.

A candidate clones index roots, removes all changed old keys, then adds changed
new keys. This order permits pair swaps and name transfers in one commit. A
name-only edit reuses the pair/reference roots. Changed rows must reference
existing final rows; deleted addresses must have zero final incoming count.
Rejected candidates discard private roots. Journal append and flush precede
publishing both rows and indexes. Group preparation retains private roots, and
an uncertain physical write preserves the original committed roots while fencing
the engine. Reopen discovers the persisted prefix and rebuilds its indexes.

Recovery still validates every historical record under the selected constraints;
a later repair cannot hide an invalid earlier record. Final indexes are built
once after replay. The wire format does not record this local validation choice,
so scan and indexed engines can reopen each other's valid journals. Branches
validate their own configured constraints; parent merge validates the parent's
constraints. Cluster workspaces retain whole selected-shard generation comparison
and atomic catalog publication; these local indexes add no distributed locks.

## Costs and composition

Let N be live rows and W the number of changed addresses. Ordered operations are
O(log N), with additional string comparison/copy costs for names.

| Selection | Work and retained memory |
| --- | --- |
| No constraints, either strategy | No index maps or constraint scan |
| Scan | Visits all rows per selected constraint. Rust uniqueness uses temporary ordered sets; C# uses temporary hash sets. Reference validation uses row lookups. No retained validation index. |
| Indexed open/replay | Historical complete constraint validation, followed by O(N log N) final index construction |
| Indexed write | O(W log N) row/index operations and O(W) changed-address buffer, shared with change-record encoding when selected |
| Indexed retained roots | O(N) entries per selected constraint; pair/name owners and nonzero incoming reference counts; names retain their bytes |
| Indexed group | Private roots and changed paths per provisional revision, retained until the shared flush result |

Rust reuses `rpds::RedBlackTreeMapSync`; C# reuses
`System.Collections.Immutable.ImmutableSortedDictionary`. Both share unchanged
nodes. Scan remains useful when retained index memory is undesirable or writes
already replace most rows. Indexed validation does not remove flat-map candidate
copies, complete snapshot encoding, branch comparison scans, full journal replay
or physical I/O. Select shared state and change records separately to reduce
those two other local commit costs. Retained history and concurrent sessions
have their existing costs.

Run `cargo run --manifest-path rust/Cargo.toml --example engine_constraint_cost`
for a finite allocation example. Eight one-name commits with all constraints,
shared state and change records request 54,805 versus 17,845 allocation bytes
at 128 rows, and 329,269 versus 20,117 at 1,024 rows (scan versus indexed in the
recorded native Debug run). Setup/recovery is excluded. These are allocation
requests, including reallocations, rather than live memory, timing or durability
measurements; toolchains can change the exact totals. C# deterministic controls
forbid candidate enumeration and bound one-row preparation to seven row lookups
at both 8 and 512 rows. Native controls verify unchanged root identity.

The SQLite interoperability examples accept `--references` to select scanned
reference checks, or `--indexed-constraints` to select indexed reference checks.
They select only this constraint, since successive providers intentionally repeat
pairs and names. The workflow selects references in all sixteen scan/indexed, flat/shared,
snapshot/changes and plain/delta compositions, reopening each database through
Rust, C#, Node, Bun and Deno. See the
[execution plan](case-studies/issue-138/CONSTRAINT-INDEX-PLAN.md), independent
constraint oracles and failure matrices for verification scope. Complete project
coverage and the wider issue acceptance criteria remain unfinished.
