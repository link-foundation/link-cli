# Independent sessions with one durability flush

`Engine::commit_group`, `LinksEngine.CommitGroup` and `WasmEngine.commitGroup`
are opt-in operations on existing staged sessions. The caller chooses a bounded
group. No worker, queue, timer or grouping delay is added to ordinary commits or
bare stores. This supplies the independent-session flush sharing described in
the [InnoDB source study](case-studies/issue-138/RESEARCH.md); it does not supply
InnoDB's page WAL, doublewrite repair or performance characteristics.

The coordinator holds the engine gate, validates each member in input order
against private tentative state, appends each accepted write's separate revision,
and flushes once. Only then does it publish the resulting state and history.
Unchanged read-only sessions need no record; empty and entirely aborted groups
do no log I/O. Every member retains the selected isolation, constraints, conflict
generations and point ownership rules. Whole-revision serializable validation
can abort overlapping members even when their writes are disjoint. Select
snapshot isolation or serializable point-read validation when appropriate.

```javascript
const first = engine.begin(), second = engine.begin();
const a = await first.create(0n, 0n);
await second.create(a.index, 0n);
const outcomes = await engine.commitGroup([first, second]);
// Under snapshot isolation: two committed revisions, one adapter flush.
```

Results follow input order. `Committed(revision)` / status `committed` is an
acknowledgement after the shared flush. `Aborted(error)` / `aborted` is a known
validation failure before that member appends. Later valid members can still
commit and aborted members do not consume revision numbers. Allocation can leave
address gaps, as with individual rollback.

An append or flush error returns `Indeterminate(error)` / `indeterminate` for
**every provisionally accepted member**, including read-only members. Known
aborts remain aborted. The coordinator retains the old visible state, closes
the group members and fences the engine. Reopen its authoritative log to discover
the valid persisted prefix before retrying business operations. The group is
not an atomic transaction: after an error or crash, zero, some or all accepted
revisions may exist. An error does not undo a completed append. Retained snapshots
and outside read-committed sessions never expose private tentative revisions.

Rust consumes a vector of sessions. A foreign member causes an outer error
before I/O; dropping the consumed vector rolls back its open members. C# and JS
reject null, duplicate, foreign or invalid arguments before consuming any member.
Already closed C# or Rust members yield known aborts. JS requires open transport
handles, reserves them synchronously, drains requests/controls already queued,
and transfers ownership to the native group. Later requests and controls reject
with `outcome: 'not-sent'`. `close()` waits for the transfer and never frees a
consumed native handle twice. The group owns an engine clone, so closing the
engine wrapper while queues drain cannot invalidate it. Freeing an uncommitted
native group rolls back its members. Group results are values, rather than a
single `LinksError`, because different members may have different outcomes.

## Durable and replicated construction

Rust's `FileTransitionLog` defaults to syncing on append. Set
`set_sync_on_append(false)` **before** wrapping/opening it to amortize sync calls;
the group still performs a durable `flush_log` before acknowledging. C# and JS
file commit logs already separate append from durable flush. Set
`require_durable_flush` / `RequireDurableFlush` / `requireDurableFlush` when
durability is required. Memory adapters remain volatile.

Optional delta encoding composes outside the selected file/replicated log.
Each independent revision remains decodable; a delta references its preceding
revision. Fixed-primary synchronous replication flushes the primary once, copies
the complete missing prefix and flushes each follower once before counting it.
Explicit catch-up still flushes the primary before reading its prefix. Replica
acknowledgements retain their existing unknown-outcome and membership semantics.

Runnable durable examples, each with optional `--delta`:

- Rust: `cargo run --manifest-path rust/Cargo.toml --example engine_group_commit -- /tmp/group-rust.log`
- C#: `dotnet run --project examples/group-commit-csharp -- /tmp/group-csharp.log`
- JS: `CLINK_WASM=/path/to/web-package node js/client/examples/engine-group-commit.mjs /tmp/group-js.log`

The native examples and JS example use the same portable journal format. Use
separate paths for independent databases. One writer owns each file; close all
handles before another implementation opens the same journal.

## Costs and verification

For K accepted writes, preparation still clones/validates/encodes each full
revision: O(sum(N_i + W_i)) work plus existing ordered-map costs and encoded name
bytes. It copies retained history and conflict/ownership metadata, and buffers
the K logical records until publication. C# also pins each accepted member's
snapshot so its closed handle retains ordinary committed-read behavior. The
extra memory therefore includes full encoded revisions and retained snapshots,
not just K small queue entries. Use bounded groups; there is one serialization
gate through preparation, append, flush and publication. This reduces flush
calls; it does not remove full-state CPU work, implement parallel validation,
compact journals, or promise a throughput multiplier.

Portable contracts cover separate revisions, constraints, conflicts, read
dependencies, history, admission and uncertain prefixes. Native tests exercise
real exclusive files, delta journals, two durable replicas, following commits
and a bounded blocked flush. Ten abrupt child exits per native language cover
snapshot/delta logs before and after append/flush, with an acknowledged control,
two actual reopenings, persisted allocation gaps and a following commit.
Actual-WASM tests inject synchronous JS journal
faults; Node/Bun/Deno conformance exercises real wasm-bindgen handle transfer and
queued requests. Finite fault images test the declared journal contract rather
than claiming physical device power-loss coverage.
