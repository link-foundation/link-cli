# Write-ahead undo for ordinary transactions

Use `RowUndoPolicy.WriteAheadImage` in C# or `RowUndoPolicy::WriteAheadImage`
in Rust when an ordinary, immediately mutating store must undo changes that
persist before its first callback. The existing `Observed` policy remains the
default. Bare stores acquire no transaction work. The staged
[engine](ENGINE-STRATEGIES.md) already keeps private revisions until publication;
its Rust-backed JS/WASM session does not need this ordinary-store option.
The Rust ordinary transaction policy also executes in actual WASM contracts.

```rust
use link_cli::{CommitMode, LogRetentionPolicy, RowUndoPolicy, TransactionsDecorator};
let tx = TransactionsDecorator::new(
    data, log, LogRetentionPolicy::Infinite, CommitMode::Sync, false,
)?.with_row_undo(RowUndoPolicy::WriteAheadImage)?;
```

```csharp
using var transactions = new TransactionsDecorator(
    data, log, rowUndoPolicy: RowUndoPolicy.WriteAheadImage);
```

Rust also permits `set_row_undo_policy` while idle; a rejected change preserves
an existing owner. C# selects the policy at construction. Existing constructor
signatures remain available, including their default behavior. New readers
always recover image metadata, regardless of the policy for subsequent writes.

Run persistent row/name rollback and version-checkout examples:

```sh
cargo run --manifest-path rust/Cargo.toml --example transaction_names -- /tmp/rust-undo --row-image
dotnet run --project examples/transaction-names-csharp -- /tmp/csharp-undo --row-image
```

## Ordering and ownership

Before an owner's first row or name write, the decorator captures its complete
logical rows and names, appends a mandatory undo image and flushes the journal.
Only then can the adapter change data. The image remains attached to the owner
through preparation errors, uncertain decisions and failed cleanup. Empty or
unobserved writes cannot detach it. Explicit names still have their separate
pre-effect record barrier.
An identical-name assignment still invokes the adapter, so selected owners
prepare their image and reconcile the result for that call too. A genuine
no-op adds no history record or version, although preparation and decision
durability still have a cost.

Successful writes compare actual rows/names with the initial image plus all
pending records. Corrections describe effects omitted by adapter callbacks,
including another owner's evicted name, deletion/allocation and names on absent
addresses. The complete record batch is retained before its first append.
Reconciliation finishes before automatic commit or version publication intent.
Complete history therefore includes those effects for committed redo and version
movement. Callback delivery and ordinary coalescing remain available.

Rollback restores the image through logical storage methods: clear references,
remove unknown allocations, reserve original addresses, remove allocator-created
holes, restore rows and replace the full name map. It verifies the resulting
image before acknowledging cleanup. An error retains admission for retry.
Recovery validates the entire journal before mutation, undoes unfinished images
in descending base-sequence order with intervening ordinary pending records,
and redoes the committed suffix that restoration could have overwritten.
Checkpoint markers cannot skip that suffix. Repeated recovery and following
edits are part of the regression contracts.

## Adapter and persistence requirements

One writer must own both the data and journal. Concurrent direct writes to the
wrapped store bypass this protocol. Enumerations and name capture must be
complete, detached logical snapshots. Replay must permit empty placeholders,
recreating original addresses and restoring their contents/names. Custom
adapters whose replay repeats unrelated side effects are incompatible; failed
image verification retains ownership rather than claiming successful rollback.

Durability requires truthful journal and data/name flushes in persistent adapters.
An uncommitted image can restore readable logical rows after process loss,
including a crash before any observer callback. It cannot make an unreadable or
torn physical storage file open successfully, or repair hardware that violates
flush ordering. Finite process exits leave the OS alive. They do not simulate
physical power loss. The default observer policy retains its pre-callback crash
window; choose the image policy or a staged engine for the corresponding
logical-write guarantee.

## Cost and wire compatibility

The first write captures/sorts N logical rows and M names, retains their strings,
and adds one journal append plus flush before effects. The encoded image and
owner memory cost O(N + M + name bytes), with sorting/map work O(N log N + M log M).
Every selected write rebuilds predicted state from the image and K pending
records, captures actual rows/names and computes corrections. Its work grows
with N, M, K and name bytes; this implementation does not provide a page-sized
undo cost. Captures/temporary maps add the same order of transient memory.
Only missing logical effects add correction records. Long explicit owners
repeat that prediction work. Image rollback scans/restores the full logical
store. The physical sidecar retains mandatory images; sized/chunked policies
continue to limit logical transition history and archives. Page undo and image
compaction remain separate unfinished efficiency work.

Images occupy the existing `__transactions:transition:` namespace, ensuring old
record readers reject this mandatory protocol instead of ignoring it:

```text
v3|undo|owner|baseSequence|rows|names
```

There are exactly six fields. Owner is a nonzero 32-digit lowercase hexadecimal
identifier; base is a canonical nonnegative signed-64-bit sequence. Rows are
strictly increasing nonzero addresses encoded as `id,source,target`, separated
by semicolons. Names use sorted `id,UTF16hex` pairs, including address zero and
absent rows. Uppercase four-digit UTF-16 units encode arbitrary delimiters;
an empty name field is a present empty map, while `-` means an unnamed adapter.
C# retains its unpaired UTF-16 domain; Rust rejects lone surrogates. Canonical
re-encoding rejects malformed or overflowing fields. Identical image retries
collapse; conflicting owner images, records at/before their owner's base and
ambiguous equal-base unfinished owners fail before mutation. Physical append
position cannot disambiguate images because named logs may move identical names.

New readers retain v1 row and v2 complete records. Upgrade readers before
selecting the new policy. History records, archives and movement intents retain
their existing codecs; undo images do not themselves allocate a history sequence.
[Shared fixtures](protocol/transaction-undo-vectors.json) specify canonical,
invalid, wide-address and string-domain cases.

SQLite's [rollback-journal ordering](https://www.sqlite.org/atomiccommit.html)
and [hot-journal recovery](https://www.sqlite.org/lockingv3.html) informed this
pre-effect ordering. SQLite operates on pager-managed physical pages. This
option uses the existing logical links adapter and has the distinct costs and
persistence limits stated above.
