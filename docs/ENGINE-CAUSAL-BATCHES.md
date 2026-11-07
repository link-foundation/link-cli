# Selected local causal batches

Status: Rust, C#, JS/TS and actual WASM pass the focused batch contracts and
53 independently generated admission vectors. Five-runtime plain/mirrored
filesystem handoff and process-exit recovery checks pass. Complete local suites
finish at 2026-10-27T18:15:46Z: 1226 native tests plus six doctests, 752 actual
WASM, 2491 C# per configuration, 681 Node and 221 repository checks pass.
`causal-batch.txt` retains complete coverage denominators and remaining gaps;
fresh publication/platform CI is still required. Ordinary mutable
links sessions remain unfinished. This component supplies bounded local
publication; it does not complete the wider issue.

Import `CausalBatch` from `@link-foundation/link-cli-client/engine-causal-batch`.
Provide one coherent `AtomicJournalStoreProvider`, two distinct reserved keys
and the same unique author used by `CausalReplica` on the owner key. The caller
owns the store and closes it. SQLite is optional. Select
`requireDurableFlush: true` only with a capable underlying provider; a private
staging store does not add durability to the caller's provider.

Rust selects `causal-batches` and imports `engine::causal_batch::{CausalBatch,
CausalBatchEdit}`. C# exposes the matching `CausalBatch` and `CausalBatchEdit`
types in the Engine namespace. Both use u64/ulong addresses and generations;
JS/TS uses canonical decimal strings. The Rust feature depends on `causal`,
adds no SQL or SHA implementation, and remains absent from a bare build.

```js
const batches = new CausalBatch(store, 'replica', 'batches', author);
const base = await batches.snapshot();
const versions = await batches.write(token, base.generation, [
  { address: '1', observed: [], payload: { source: '0', target: '2', name: 'row' } },
  { address: '2', observed: [], payload: null },
]);
```

Addresses are canonical positive u64 decimal strings in strictly increasing
numeric order; each address occurs once. Endpoints may be zero. All returned
versions and snapshot data are owned. Observations and payloads become owned
before the first asynchronous store read. The owner journal remains readable
through the existing `CausalReplica`, including its standalone write/receive
operations. Those operations need not import the batch component.

## Publication and retry identity

Each call reads both journals at one coherent boundary. Existing owner replay
validates the complete retained history before receipt evidence is trusted.
The component stages each row with the existing single-write implementation,
then performs exactly one real two-key compare-exchange against the original
snapshot. A stale base, invalid later row, child-token collision or either
journal's capacity limit leaves all real keys unchanged. Private staging can
change before an error; it is discarded and never exposed as a real generation.

A successful request is identified by its nonzero lowercase 32-hex token,
original base generation and normalized edits. For edit i, the child retry
token is unsigned128(token)+i, encoded as 32 lowercase hex digits. Preflight
token-range overflow and reserve disjoint ranges between batches and standalone
writes. A matching old single-write token cannot be reused as a fresh batch
mutation: every new result must have the next expected global author counter.
The existing owner bounds and validates counters on every staged mutation,
before the real publication. Bounded retained records also bound attainable
local counters until a future safe compaction protocol exists.

A matching batch receipt is checked before rejecting its now-stale base. Retry
the original request after a lost reply, including its original base generation.
The result contains the original versions even after later edits. Full supplied
observation evidence is validated against retained history on retry, including
dominated observations. A changed normalized request rejects. CAS false means
conflict; exceptions or malformed replies mean an indeterminate publication.
There is no automatic mutation retry. Empty batches publish an owner header and
receipt, consume metadata capacity and retry without another CAS.

The snapshot generation is the underlying owner key's generation. Treat it as
an opaque guard obtained from `snapshot()`, rather than calculating the next
value. Providers may leave it unchanged for a receipt-only empty batch or
advance it by more than one when other keys share a global generation counter.
Both actual journal generations still guard the one coherent publication.

## Receipt wire and admission

The separate compact canonical ASCII JSON journal has this header:

`["causal-batches1",OWNER_KEY,AUTHOR]`

Each following entry is:

`["causal-batch1",TOKEN,BASE_GENERATION,START_COUNTER,EDITS]`

Each edit is `[ADDRESS,OBSERVED,PAYLOAD]`. OBSERVED is the canonical merged
version array; PAYLOAD is the existing causal-version payload array or null.
Names use hexadecimal UTF-8. Counter/generation fields use canonical u64 decimal
strings. A zero journal generation requires an empty entry array. Nonzero
metadata requires the exact owner/author-bound header and an existing matching
owner lineage. Every receipt is checked against its actual validated owner
write records: derived token, normalized request and contiguous counters.
Duplicate batch tokens and overlapping nonempty child-token ranges reject.

Both journals are independently bounded by `maxEntries` (2..4096) and
`maxBytes` (1..8 MiB). Each record is at most 1 MiB. A batch has at most 64 edits,
each with at most 64 observed versions; all limits apply together. Receipt
preparse admission rejects non-ASCII, escapes, object syntax, more than seven
array levels and more than 32768 unquoted separators before JSON allocation.
Existing owner admission retains its stricter five-level bound. Transport
adapters must bound raw input before constructing provider strings.

## Costs and guarantee limits

For E edits, H retained owner versions and maximum version wire size W, reuse
of complete owner replay costs O((E+1)H²W) in the worst case. Each staged write
copies the bounded owner journal. Loading also builds an owner-token index and
validates the complete bounded receipt journal. Publication copies both complete
journals before the underlying provider's cost. Receipts retain history and
consume capacity; indexed replay and safe reclamation remain open work.

Atomicity is local to the selected coherent store. Per-row Merkle transfer does
not preserve a batch as a remote atomic group. Independent authors still need
disjoint address allocation or an external allocator. Global uniqueness,
cascades, foreign-key integrity, distributed serializability, fencing offline
clones and hardware power-loss guarantees require their own selected strategies
and verification. Ordinary links/name projection and mutable sessions remain
unfinished; this API exposes causal versions directly.
