# Optional SQLite page-WAL journal

Select SQLite at the existing ordered-log boundary when the staged engine needs
a physical pager, checksummed page WAL, atomic SQL group commits and explicit
checkpoint maintenance. The engine still supplies links, names, isolation,
history, branches and logical revision encoding. The default store adds no
SQLite provider. This component does not replace legacy mapped Doublets files.

| Surface | Selected component | Unselected cost |
| --- | --- | --- |
| Rust | `sqlite` feature; `engine::sqlite::SqliteLog`; bundled rusqlite | No SQLite build/dependency |
| C# | `SqliteCommitLog` takes an opened, dedicated `DbConnection` | Core adds no SQLite NuGet provider |
| JS/TS | Separate `engine-sqlite-log` entry; synchronous `exec/prepare/all/run/close` connection | Universal entry imports no provider |
| WASM | Existing JS commit-log bridge, with that JS connection | No bare-WASM filesystem/VFS durability claim |

### Native paths and dependency checks

SQLite is an optional provider at the ordered-log boundary. The following
paths use this project's doublets and journal implementations without SQL:

| Selected behavior | Native implementation |
| --- | --- |
| Bare cache and mapped doublets | `LinkStorage` / `DoubletsStorage` in Rust; the ordinary C# doublets store; in-memory Rust-backed WASM |
| Staging, validation, immutable history and branches | The links engine over its selected in-memory or persistent doublets rows |
| Durable ordered logical records | Rust `FileTransitionLog`, C# `FileCommitLog`, JS `FileCommitLog`; WASM uses the JS host bridge |
| Logical pre-effect undo in legacy stores | Selected full logical row/name images and native transaction journals |
| Replica acknowledgements and catch-up | Ordered-log replication decorators over the chosen native logs |
| Native shadow pages | Optional Rust `PagedLog`/`native-pages` file device, C# `PagedCommitLog`/`FilePageDevice` and JS/TS `FilePagedCommitLog`, actual Rust-backed WASM; redundant repair in progress; see [native page protocol](ENGINE-NATIVE-PAGES.md) |
| Shards and atomic catalog publication | Native immutable-object, routing and catalog adapters; the optional etcd adapter delegates consensus to etcd |

The CI guard in `.github/scripts/check-sqlite-optional.mjs` resolves actual
Cargo production dependency trees for default, bare and WASM targets, with an
explicit SQLite feature as its positive control. It checks the C# library's
resolved direct/transitive packages independently of SQL test/example providers.
Its JS guard rejects SQLite module resolution while importing the root entry,
replicating in memory and reopening a native file journal; a deliberate import
of the optional SQL entry verifies that the guard can detect a dependency.
Rust CI also exercises mapped doublets and file journals with all optional
providers disabled.

These checks establish dependency optionality. Native logical records and undo
images do not supply a physical pager or repair unreadable/torn mapped pages.
Native page repair, page undo/doublewrite, LSM, persistent object retention,
incremental whole-journal replay, automatic balancing and the remaining
distributed algorithms are unfinished requirements in the
[acceptance inventory](case-studies/issue-138/REQUIREMENTS.md). The selected
SQLite pager does not complete those native requirements.

The common [storage protocol](protocol/sqlite-log.md) uses exact schema and
identity versions, contiguous signed-i64 sequences and strict UTF-8 BLOBs.
Empty strings, NUL, leading U+FEFF, Unicode and protocol delimiters round-trip.
Malformed UTF-8, unpaired UTF-16 and physical CR/LF reject. SQLite record sequence
numbers are independent of engine revision/address widths. Independent
[vectors](protocol/sqlite-log-vectors.json) specify bytes and invalid controls.

## Selecting the component

```rust
use link_cli::engine::sqlite::{SqliteLog, SqliteOptions};
use link_cli::engine::{Engine, EngineOptions};
let journal = SqliteLog::open("engine.db", SqliteOptions::default())?;
let engine = Engine::<u64, _>::open(journal, EngineOptions {
    require_durable_flush: true,
    ..Default::default()
})?;
```

Enable Cargo feature `sqlite`. Rust owns its connection and rolls pending records
back on drop. The native module is excluded from `wasm32` builds.

```csharp
using var connection = new Microsoft.Data.Sqlite.SqliteConnection(
    "Data Source=engine.db;Pooling=False;Default Timeout=1");
connection.Open();
using var journal = new SqliteCommitLog(connection);
var engine = new LinksEngine<ulong>(journal,
    new() { RequireDurableFlush = true });
```

The application selects its C# provider. The log takes ownership of the already
open connection, including disposal on failed construction. Use a dedicated
connection without an existing SQL transaction; do not mutate it externally.
Examples/tests disable pooling so disposal releases database files on every OS.
Every explicit command has a positive finite `CommandTimeoutSeconds` (default
one). Microsoft.Data.Sqlite has its own busy/locked retries; zero command timeout
means unlimited retries and is rejected. Custom SQLite busy waits are separately
finite. These are component retry limits, without a hard scheduler deadline.

```js
import { DatabaseSync } from 'node:sqlite';
import { SqliteCommitLog } from '@link-foundation/link-cli-client/engine-sqlite-log';
using journal = new SqliteCommitLog(new DatabaseSync('engine.db'));
using engine = new WasmEngine(binding, { requireDurableFlush: true }, journal);
```

Bun can pass `new (await import('bun:sqlite')).Database(path)`; Deno uses its
`node:sqlite` compatibility module. The adapter imports neither provider. It
requires synchronous statements with positional BLOB and BigInt bindings. A
runtime SQLite version check rejects versions below 3.51.3, including an older
system SQLite selected by a runtime. Core Node compatibility remains separate
from the Node version/provider needed for this optional entry.

## Commit, recovery and maintenance

First append starts `BEGIN IMMEDIATE` and checks the cached expected tail while
holding SQLite write ownership. Subsequent appends share the SQL transaction.
`Read` sees the owner's pending records; independent readers see committed
records. `Flush` issues one SQL COMMIT for a pending group, with WAL/FULL checked
before publication. An empty flush makes no durability request. Closing rolls
pending appends back; it does not silently commit them.

SQLite's writer lock is held over a pending group, not the lifetime of an engine.
One engine must coordinate live sessions and immutable state. A second cached
owner that observes a changed tail fails and fences itself. This is not a
distributed engine, automatic cache refresh or cross-engine MVCC. Direct
external mutation of the owned connection/table violates ownership.

Component I/O, malformed read data and stale ownership fence subsequent reads,
appends, flushes and maintenance. A failed COMMIT may already have committed;
best-effort rollback releases any remaining transaction without erasing committed
rows. Close and reopen to discover the exact committed prefix before deciding
whether to retry. Pure invalid append input and pending-checkpoint rejection do
not poison an otherwise usable writer. A failed JS/C# provider close can be
retried. No SQLite error is silently converted into a successful commit.

| Option/operation | Default and cost |
| --- | --- |
| Busy timeout | Rust/JS `0` ms; fail immediately on SQLite contention |
| C# command timeout | `1` second, in addition to provider/SQLite behavior |
| Automatic checkpoint | `1000` WAL pages; zero selects explicit maintenance |
| Open | Validate identity/schema and sequence aggregates, O(H) for H records, without materializing payloads |
| Read | O(H + payload bytes), materializes the journal for the existing engine replay boundary |
| First append | Writer admission plus indexed MAX-tail lookup and payload insertion |
| Following append | Insert into the same SQL transaction; payload/pager costs remain |
| Flush | One pending SQL COMMIT; SQLite determines actual WAL synchronization/checkpoint calls |
| Checkpoint | Work proportional to pages transferred; reader pins can block progress |

Rust `checkpoint(CheckpointMode::...)`, C# `Checkpoint(SqliteCheckpointMode...)`
and JS `checkpoint('passive'|'full'|'restart'|'truncate')` return actual busy,
log-frame and checkpointed-frame counters. Default is passive. Pending appends
reject maintenance before SQL I/O. A pinned reader can make truncate report
busy; release it before retrying. Automatic and explicit checkpoints transfer
physical pages; neither removes logical journal records. Database/replay costs
still grow with retained history. Logical compaction is not implemented here.

WAL/FULL is requested and read back. FULL supplies SQLite's documented commit
sync behavior only on a filesystem/VFS/device that honors locking and sync.
SQLite WAL requires same-host shared memory; a network filesystem is unsupported.
Keep database sidecars with their owner. Use a consistent SQLite backup procedure
for a live database; copying only the main file can omit acknowledged WAL data.
This component adds no cross-host database-file protocol.

## Verification and source reuse

The pre-implementation contracts cover all four actual providers. Native tests
also cover stale/busy owners, pinned readers, setting drift, corrupt schemas and
sequences, malformed payloads, before/after-COMMIT errors, bounded SQLITE_FULL,
group pins, replication/delta composition and exact following writes. Each
native language has forty finite actual process-exit controls across maps,
logical encodings and physical delta selection. SQL pending groups recover as
all-or-none. Copied-WAL controls separately compare an intact commit against a
torn final frame and changed frame checksum, then run quick_check and a following
commit. These are controlled frame/process tests, not physical power-loss tests.

The persistent examples verify exact earlier row/name/revision data while
Rust -> C# -> Node -> Bun -> Deno reopen the same database. Run
`cargo run --manifest-path rust/Cargo.toml --features sqlite --example engine_sqlite -- engine.db`,
then `dotnet run --project examples/sqlite-csharp -- engine.db`, then each runtime
with `js/client/examples/engine-sqlite.mjs engine.db` and `CLINK_WASM` set. Add
`--flat`, `--snapshot` or `--delta` consistently to select all eight compositions.
Add `--references` consistently for scanned reference validation, or
`--indexed-constraints` for indexed reference validation. Both validators enforce
references in the sixteen compositions exercised across all five providers. Repeated
provider names/pairs are intentionally unconstrained in this example.
The JS integration workflow executes the provider and interoperability controls.
Complete local/CI evidence and the wider unfinished criteria remain tracked in
the [active plan](case-studies/issue-138/SQLITE-JOURNAL-PLAN.md).

Reuse is the maintained SQLite pager/WAL/checkpoint implementation, with no
copied amalgamation in this repository. Audited rusqlite v0.41.0 tag
`a60e76b5e86ee60442213efe79758a6c2ca8cf3d` uses libsqlite3-sys 0.39.1 / SQLite
3.54.1. The reviewed WAL paths checksum commit frames, sync FULL commits before
WAL-index publication, and check WAL salts under the reader lock during checkpoint
restart. Final C# tests/examples use Microsoft.Data.Sqlite 10.0.14 / SQLite 3.53.3
(the initial component probe used 10.0.13). Actual JS probe runtimes:
Node 26.12.1 / 3.54.0, Bun 1.4.3 / 3.53.4 and Deno 2.9.7 / 3.53.2. These are
observations of the tested versions, not permanent runtime specifications.

Primary references: [SQLite WAL and WAL-reset fix](https://www.sqlite.org/wal.html),
[synchronous](https://www.sqlite.org/pragma.html#pragma_synchronous),
[checkpoint](https://www.sqlite.org/pragma.html#pragma_wal_checkpoint),
[rusqlite release](https://github.com/rusqlite/rusqlite/releases/tag/v0.41.0),
[Microsoft.Data.Sqlite custom providers](https://learn.microsoft.com/en-us/dotnet/standard/data/sqlite/custom-versions),
[provider errors/timeouts](https://learn.microsoft.com/en-us/dotnet/standard/data/sqlite/database-errors),
[connection ownership/pooling](https://learn.microsoft.com/en-us/dotnet/standard/data/sqlite/connection-strings),
[Node synchronous API](https://nodejs.org/download/release/latest-v24.x/docs/api/sqlite.html)
and [Bun SQLite](https://bun.sh/docs/runtime/sqlite).

SQLite supplies a selected physical page-WAL path. Legacy torn mapped pages,
doublewrite, LSM storage, all physical power-loss schedules, full-project 100%
coverage and the remaining distributed/competitor criteria stay unfinished.
