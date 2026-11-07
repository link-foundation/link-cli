# Names in ordinary transactions and version history

The ordinary Rust and C# transaction decorators record `SetName`/`set_name`
and `RemoveName`/`remove_name`. Rollback restores names, including both owners
when assigning a unique name moves it. Undoing deletion restores the row's
name and names removed by cascading deletion. Names assigned to absent rows,
address zero and empty strings retain their distinct meanings.

The ordinary version decorator publishes name-only changes as versions.
Checkout and branch switching replay complete row/name records. Pending or
failed commit, undo, recovery and movement owners fence further named writes
until cleanup succeeds. A logical no-op name assignment adds no version.
Row records capture the actual resulting name. C# journals pending observed
rows before a callback writes a name, so that name write follows its triggering
row during replay. Ordinary callbacks retain their existing coalescing and
delivery before journal errors. A failed post-effect name capture retains
observed row undo and fences an explicit owner until rollback completes.

Run the persistent examples with separate directories:

```sh
cargo run --manifest-path rust/Cargo.toml --example transaction_names -- /tmp/rust-names
dotnet run --project examples/transaction-names-csharp -- /tmp/csharp-names
```

The staged engine already includes names in its immutable revisions; its
Rust, C# and Rust-backed JS/WASM sessions retain those APIs and guarantees.
See [engine strategies](ENGINE-STRATEGIES.md) for cache, persistent and
distributed configurations.

## Complete journal payload

The existing Rust `GenericTransition<T>` remains `Copy`. C#'s `Transition<T>`
retains its original fields. Their row-only v1 serialization and public
`log()` / `Log` snapshots remain available. A name-only transition projects to
an update with empty before/after rows. That row projection cannot restore
names: use `journal()` / `Journal` and `JournalRecord` for complete history,
or `apply_record` / `ApplyRecord` and their revert counterparts for unlogged
replay. The same admission rules apply to these complete replay methods.

Records without name metadata still serialize exactly as v1. Complete records
use nine pipe-separated fields:

```text
v2|owner|sequence|unixMilliseconds|kind|beforeRow|afterRow|mode|names
```

The first seven fields retain the row codec's meanings, with `v2` as the
version. Mode `n` means names only, requiring an update and empty rows. Mode
`r` replays a row and its associated name state. Name changes are sorted by
numeric address, separated by semicolons, and encoded as
`address,beforeName,afterName`. `-` means absent; an empty field means a
present empty string. Names use uppercase four-digit UTF-16 code-unit hex,
so delimiters, NULs, newlines and Unicode never change record boundaries.
The complete codec requires canonical fields, unique sorted addresses and
timestamps representable by C# `DateTimeOffset`. C# preserves its existing
unpaired UTF-16 string domain; Rust rejects those values before replay.
[Shared fixtures](protocol/transaction-name-vectors.json) specify both cases.

New readers accept v1 journals and old row-only movement intents. Old readers
reject v2 transaction payloads; upgrade readers before writing named history.
Existing v1 records contain no historical names, which cannot be reconstructed.
Chunk archives and movement intents preserve complete v2 records. Recovery
validates all recognized records before replacing live history or changing
rows or names, including conflicting payloads at the same sequence.

## Persistence and cost

Explicit name writes append and flush their complete undo record before
changing the name sidecar. Commit still requires the journal decision and
data/name checkpoint; an uncertain decision retains its cleanup owner.
Recovery redoes committed records and undoes unfinished name writes using
the complete payload. Fault images and actual finite process exits test these
boundaries. Filesystem flush contracts remain dependent on the configured
adapter and hardware. The existing row mutation before its observer-journal
append remains a crash window in the default observer policy. Select
[write-ahead logical undo](TRANSACTION-UNDO.md) to prepare an image before row
effects and reconcile unreported changes before commit/version publication.

The selected transaction wrapper scans and decodes the named sidecar before
each row or name operation to capture names of absent rows and cascade victims.
Row writes also capture the resulting names. C# callbacks that write names
drain their observed row batch and capture its names before that name write;
their cost grows with the number of such writes as well as sidecar size.
Sorting costs O(N log N) for N sidecar rows, plus decoded name bytes and
logical-map insertion costs; name edits also clone the
logical map, retain changed strings and add a pre-effect journal flush.
Complete history snapshots copy retained name payloads. Row-only records use
an empty name collection. Bare stores do not run this bookkeeping. Rust
unnamed adapters return `None` without a name-map allocation and reject named
records before replaying their rows. Rust recovery probes this capability once
for a validated journal containing names, rather than scanning per record.

C# custom `INamedTypesLinks<T>` implementations inherit row-based name capture.
Adapters supporting names of absent rows must override `CaptureNames()` to
include them. `ReassignsNames` defaults to unique-name reassignment; adapters
supporting shared names override it to `false`, as `NamedLinksDecorator` does.
Both existing built-in name policies are preserved.
Custom capture methods return a detached snapshot. Complete journal bytes do
not change an adapter's uniqueness policy; replay requires a compatible policy.
