# Selected owned causal projection

Status: production ports pass five matching contracts in native Rust, C#,
Node, Bun, Deno and actual WASM, including all 132 independently generated
semantic vectors. The complete 686-test Node suite enforces 100% production
line, branch and function coverage. Complete validation finishes
2026-10-27T19:07:39Z: 1231 native tests plus six doctests, 757 actual-WASM,
2496 C# tests per Debug/Release configuration and 221 repository checks pass.
The projection covers all 69 Rust lines, 16 branches and nine functions on
native/WASM, and all C# source lines and 42 branches in both configurations.
Complete project coverage remains below 100%; raw regions, instantiations and
every denominator are retained in the [receipt](../dev/log/issues/138/pulls/139/causal-projection.txt).
The separately selected [mutable session](ENGINE-CAUSAL-SESSIONS.md) now stages
ordinary links/names; its full query parity and fresh platform CI remain active.

The projection consumes causal versions from an owned snapshot and requires an
explicit conflict policy. `reject` refuses a row with multiple retained
concurrent siblings. `max-dot` chooses the greatest fixed-width ordinal author,
then the greatest numeric counter. It is an explicit deterministic application
choice, with no claim that this order represents wall-clock time or globally
serializes independent writers.

Selection includes deletion markers. Only after selecting a version does the
projection decide whether an ordinary row is visible. Filtering markers first
can resurrect a losing live value. A winning live version can still be visible
under `max-dot`; every losing sibling and deletion remains in `versions()`.
Use `selected()` to inspect the chosen versions, including deletions, and
`rows()` for visible link/name values in numeric address order. Calls return
owned collections and names, independent of caller mutation or later reads.

Every supplied version is canonically validated before pruning. Reuse of one
author/counter dot for different values or addresses rejects, and equal-clock
contradictions reject even beneath a newer dominating version. The projection
does not remove evidence from its source owner, authorize deletion from absence,
perform I/O, fence authors, assign addresses, commit writes or synchronize peers.

Selected bounds apply to the complete input, including duplicates and dominated
versions: `maxVersions` is 1..4096, `maxBytes` is 1..8 MiB of canonical ASCII
version encodings, with at most 64 supplied versions per row. Existing per-version
clock/name/wire limits also apply. The exact 4096-entry and 8 MiB boundaries are
tested with finite inputs; the next supplied version is rejected. Transport
adapters must bound raw incoming data before constructing these values.
Empty input is valid; the positive lower bounds apply to configured capacities.
Within those capacities, identical versions are accepted and retained once.

Use an already admitted owner snapshot when rendering a read-only cache. In JS:

```js
import { CausalProjection } from '@link-foundation/link-cli-client/engine-causal-projection';
const snapshot = await batch.snapshot();
const view = new CausalProjection(snapshot.versions, 'max-dot');
const rows = view.rows();             // Visible ordinary rows and names.
const retained = view.versions();     // Complete retained conflict evidence.
const selected = view.selected();    // Includes winning deletion markers.
```

Choose `reject` instead when the application must resolve every concurrent row
before showing a value. Rust selects the `causal-projection` feature and uses
`CausalProjection::new(&snapshot.versions, CausalConflictPolicy::Reject,
CausalProjectionOptions::default())?`; C# uses
`new CausalProjection(snapshot.Versions, CausalConflictPolicy.Reject)`.
These constructors share the semantic vectors in
[`protocol/causal-projection-v1.json`](protocol/causal-projection-v1.json).
The returned rows are a snapshot: obtain a new owner snapshot and construct a
new projection to see later local or replicated writes. Editing a returned row
does not write it back. Use a selected mutable session or the replica/batch
publication API for explicit observed writes.

The runnable [SQL-free cache example](../examples/native-lsm-js/projection-cache.mjs)
creates and removes its own temporary physical directory. Run it with Node or
Bun, or Deno with read/write/environment permission. It requires the physical
provider's durable-flush capability and verifies concurrent deletion,
explicit resolution through a batch, a refreshed view and an unchanged prior
view. No network membership or global allocation is inferred from this local
cache recipe.

JS uses canonical decimal strings for addresses and row endpoints, including
the full u64 range. Its row objects contain `index`, `source`, `target`, `name`.
Rust uses owned `Row<u64>` values and C# uses `EngineRow<ulong>` values, preserving
the existing links/name representation. Null names, empty names, Unicode, BOM
and NUL names retain their distinct values.

For V supplied versions, per-row counts r, and maximum encoding size W,
admission/grouping and complete causal merges cost O(V log V + Σr²W) plus
encoding/copying; each r is at most 64. Retained and selected owned snapshots
consume O(VW) space. Each read allocates its returned owned values. There is no
incremental index or history reclamation in this component. The separate Rust
`causal-projection` feature and JS `./engine-causal-projection` export avoid
adding JSON/SHA/SQL to a bare import;
projection requires the selected causal codec but no SQL or SHA implementation.

Mutable links/name sessions use a private standard engine, explicit address
reservations and one causal batch publication; full query parity remains active. Local batch
atomicity does not make independently transferred remote rows atomic as a group.
Global uniqueness, membership, safe reclamation and distributed serializability
remain separate guarantees requiring their own selected coordination.
