# Compact retained history in native page files

Select a native checkpoint journal when an engine needs to replace its retained
history on disk. Each factory owns a publication root and both fixed page
banks. Root and bank mirroring are independently selectable. The existing
ordinary native journal remains available separately.

```rust
use link_cli::engine::paged::{FileCheckpointOptions, FileCheckpointPagedLog};
use link_cli::engine::{Engine, EngineOptions};

let log = FileCheckpointPagedLog::with_options("existing-directory/journal", FileCheckpointOptions {
    mirrored_banks: true, mirrored_root: true, max_pages: 1024,
})?;
let options = EngineOptions { require_durable_flush: true, ..EngineOptions::default() };
let engine = Engine::<u64, _>::open(log, options)?;
// Commit application transactions, then explicitly publish retained history.
engine.compact_journal()?;
let mut log = engine.into_log().map_err(|_| "engine still has active owners")?;
log.close()?;
```

```csharp
using Foundation.Data.Doublets.Cli.Engine;

using var log = new FileCheckpointPagedCommitLog("existing-directory/journal",
    mirroredBanks: true, mirroredRoot: true, maxPages: 1024);
var engine = new LinksEngine<ulong>(log, new() { RequireDurableFlush = true });
engine.CompactJournal();
```

```js
import { WasmEngine } from '@link-foundation/link-cli-client';
import { FileCheckpointPagedCommitLog } from
  '@link-foundation/link-cli-client/engine-file-checkpoint-pages';

using log = new FileCheckpointPagedCommitLog('existing-directory/journal', {
  mirroredBanks: true, mirroredRoot: true, maxPages: 1024,
});
using engine = new WasmEngine(binding, { requireDurableFlush: true }, log);
engine.compactJournal();
```

Rust requires feature `native-pages`. JS supports Node, Bun and Deno; its separate
entry keeps filesystem imports out of the universal client. None of these
factories requires SQLite. Close engines and sessions before closing their log.
The log exposes its selected publication epoch; JS uses `bigint`, C# uses
`ulong`, and Rust returns `Result<u64, LinkError>` so a closed owner rejects it.

## Files and ownership

A single-copy database uses `journal`, `journal.bank0` and `journal.bank1`.
Bank mirroring adds `journal.bank0.mirror` and `journal.bank1.mirror`; root
mirroring independently adds `journal.mirror`. Each device owns its data file,
`.meta0`, `.meta1` and exclusive `.writer` handle.

| Root copies | Copies per bank | Devices | Raw handles |
| --- | --- | --- | --- |
| 1 | 1 | 3 | 12 |
| 2 | 1 | 4 | 16 |
| 1 | 2 | 5 | 20 |
| 2 | 2 | 6 | 24 |

The original Rust `open`/`with_max_pages` and C# constructor signature retain
single-root behavior; JS defaults both selections to false. The C# overload
with `mirroredRoot` also requires `mirroredBanks`, allowing existing calls to
resolve unambiguously. Keep both persisted modes on every reopen. Changing a
mode requires export into a separately owned store; merely adding a mirror
file is rejected before existing bytes change. Root-mirrored decisions use
flags 2/3, which older single-root providers reject. Parent directories must
exist. See [root selection and repair](protocol/checkpoint-root-mirror.md).

The factory acquires the root and optional root mirror first, then every bank
and bank mirror before any
portable validation, bank reset or recovery repair. Opening any foreign-owned
component fails without changing previously committed bytes. Never open internal
bank paths independently. Constructor failure closes every acquired raw owner
in reverse order; explicit close attempts every owner and releases the root
last. Original initialization errors and all cleanup failures remain observable.
Rust retains independent raw leases through fallible construction; dropping a
partly built portable bank cannot conceal cleanup failures.

Close never flushes staged appends. It marks the owner closed before attempting
cleanup, and repeated close does not retry uncertain native handles. Failed
native close retains the affected `.writer` fence. A following constructor must
acquire every bank fence before repair even if the root fence was released.
Externally fence the old writer before removing an ownership file; age alone is
insufficient. The tests remove abandoned fences only after observing child exit.

These files use the [two-bank publication protocol](protocol/checkpoint-pages.md)
and the unchanged [native page format](protocol/native-pages.md). An ordinary
journal cannot be converted by reopening its path with this factory. Export a
checkpoint into a separately owned destination and verify it before retiring the
source. The compatibility tests reject in-place conversion in both directions
and verify the original journal remains readable and writable.

## Retention and costs

Choose engine history, state structure, logical encoding, constraints and
physical delta compression independently. Compaction holds the engine commit
lock through checkpoint construction and root synchronization. It preserves
committed allocator state, retained snapshots, pinned images, branch merge bases
and live private reservations. Reopening restores committed allocator state.
The [checkpoint protocol](protocol/engine-checkpoints.md) describes that boundary.

Compaction scans the current journal and writes complete retained snapshots to
the inactive bank. Two banks occupy disk until reused by later publication;
bank mirroring doubles bank I/O. Root mirroring adds one metadata write and
synchronization per replacement. Root recovery validates the entire selected
bank before repairing either root slot. Retaining all history does not bound its size.
Bounded retention limits record counts, while names and rows can still grow.
Rust's selected factory uses a small mutex per device operation to retain raw
ownership across fallible construction. Ordinary native journals incur none of
these root, second-bank, replacement or lease costs.

## Reproduce native interoperability

Build the native examples and actual web-target WASM package using the
[native page recipe](ENGINE-NATIVE-PAGES.md), then run:

```sh
CLINK_WASM=/absolute/path/to/pkg bash examples/journal-checkpoints/run-native-compact-interop.sh
```

The finite run uses dedicated temporary databases. It covers 96 configurations:
independently single/mirrored roots and banks, three state structures, two logical encodings, selected
constraints and optional physical delta compression. Rust, Node, Bun, C#, Deno
and Rust reopen the same files in sequence. Every writer commits two sessions,
compacts retained history, preserves a pin and branch base, rolls back private
allocation, then closes before the next writer. The 576 writer handoffs reach
epochs 1 through 6. Mirrored handoffs damage one closed selected-bank copy
and/or one acknowledged root copy and require repair before the following
group. Ordinary and imported-checkpoint controls retain their 48 configurations.

Native tests also terminate bounded child processes immediately before and
after actual writes, truncations and file synchronization during replacement
and root-only, bank-only or combined mirrored recovery. Fresh owners verify a complete old or new publication,
then append, flush, compact and reopen again. Device synchronization and
filesystem durability assumptions still apply. These finite process tests do
not certify hardware power loss or repair simultaneous corruption of both root copies.
