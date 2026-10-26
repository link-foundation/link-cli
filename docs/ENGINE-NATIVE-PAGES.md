# Native immutable doublets page journal

`FilePagedCommitLog` is an optional SQL-free JS/TS ordered-log provider for the
Rust-backed WASM engine in Node, Bun and Deno. It stores string chunks and record
links as native doublets, publishes one flush group through alternating roots,
and never overwrites committed data pages. The universal package entry imports
neither this filesystem adapter nor a SQL provider. The portable `PagedCommitLog`
codec is a separate `./engine-paged-log` entry accepting a synchronous `PageDevice`.

Rust provides the portable `engine::paged::PagedLog<D>` on native and WASM
targets. Feature `native-pages` selects `FilePageDevice`; it introduces no SQL
dependency and its native dependencies are excluded from WASM. Rust, Node, Bun
and Deno reopen the same files in all 16 example policy combinations. C# provides
`PagedCommitLog` over caller-owned `IPageDevice`, with `FilePageDevice` for native
files. The public C# library adds no SQL package. All five runtime examples
reopen the same files and verify the preceding writers' exact records.
Rust, C# and JS/TS also provide independently selected redundant repair. See
[mirrored mode](protocol/native-pages-mirrored.md). This provider does not implement
repair of legacy mapped Doublets files, LSM or efficient page undo. Alternating
metadata recovers torn unacknowledged writes under the stated model. Single-copy
mode rejects committed page/decision corruption instead of silently discarding an acknowledged
group. See the [binary protocol and exact persistence assumptions](protocol/native-pages.md).

```js
import { WasmEngine } from '@link-foundation/link-cli-client';
import { FilePagedCommitLog } from '@link-foundation/link-cli-client/engine-file-paged-log';
using log = new FilePagedCommitLog('existing-directory/journal', { maxPages: 1024 });
using engine = new WasmEngine(binding, { requireDurableFlush: true }, log);
await using session = engine.begin();
await session.create(0n, 0n);
await session.commit();
```

The caller closes the engine and its sessions before closing the log. Append
stages records; flush writes/synchronizes only new pages, then writes/synchronizes
the alternate metadata, and only then acknowledges. A failed mutation or read
fences the handle; close/reopen and inspect the outcome before retrying. An empty
flush performs no I/O. Strict UTF-8 preserves BOM/NUL/Unicode; invalid UTF-16 and
CR/LF records reject before mutation. A selected positive `maxPages` quota rejects
new records before staging when they would exceed that limit.

Each database uses its data path, `.meta0`, `.meta1` and an atomic exclusive-create
`.writer` ownership file. Parent directories must exist. Cooperating native
providers use the same ownership name. An existing live/stale owner rejects
immediately. Never delete an owner file based on age: externally fence the old
writer first. A close error retains the owner file because the old handles may
still be open. POSIX initialization synchronizes the parent directory; Windows
uses file synchronization and does not claim POSIX directory synchronization.
The filesystem/controller must honor successful synchronization and preserve
previously durable files outside subsequent writes.

Costs are O(new UTF-8 bytes/pages) per flush, one 4,096-byte page buffer plus
pending records/encoded bytes, and a fixed 128-byte metadata write. A partial
committed page's unused cells are never reused: small groups can waste 251 cells.
Open/read validate O(history), and the existing ordered-log interface returns all
records. This is not incremental engine replay. `stateStructure`,
`revisionEncoding`, constraints, history, delta encoding and replication remain
independently selectable; replicated flush still has its documented membership
and acknowledgement costs and does not provide consensus or automatic failover.

Run the public example with the generated web WASM package and a dedicated path:

```sh
CLINK_WASM="$PWD/js/pkg" node js/client/examples/engine-native-pages.mjs existing-directory/journal
CLINK_WASM="$PWD/js/pkg" bun js/client/examples/engine-native-pages.mjs existing-directory/journal
CLINK_WASM="$PWD/js/pkg" deno run --allow-all js/client/examples/engine-native-pages.mjs existing-directory/journal
```

Each invocation verifies all prior exact names and commits two sessions in one
flush, retaining their separate revisions and pinned predecessor. `--flat`,
`--snapshot`, `--delta`, `--references` and `--indexed-constraints` select independent
policies. Reopen with compatible constraints and the same physical delta choice.
`--mirrored` selects two-copy physical repair when creating a database; retain
the same immutable mode on every subsequent open in every language.

The Rust example accepts the same path and policy flags:

```sh
cargo run --manifest-path rust/Cargo.toml --no-default-features --features native-pages --example engine_native_pages -- existing-directory/journal
dotnet build examples/native-pages-csharp
dotnet run --project examples/native-pages-csharp -- existing-directory/journal
CLINK_WASM="$PWD/js/pkg" bash examples/native-pages/run-interop.sh
```

The interchange script exercises 32 physical/policy configurations. It reopens
each file through Rust, Node, Bun, C#, Deno and Rust, verifying all earlier exact
names and twelve final revisions. In mirrored mode it damages one closed copy
between writers, alternates the damaged side across configurations, and checks
both repaired data and metadata byte for byte after each writer closes.
These destructive controls use dedicated temporary databases only.

Rust callers pass `FilePageDevice::open(path)?` to `PagedLog::open`, then use the
log through the existing `TransitionLogStore` interface. After releasing the
engine, `log.into_device().close()?` reports close errors. Device extraction does
not flush pending records. Dropping the file device attempts cleanup; explicit
close lets the caller observe uncertain ownership and retain the fencing file.

C# keeps device ownership explicit:

```csharp
using var device = new FilePageDevice("existing-directory/journal");
ICommitLog journal = new PagedCommitLog(device, maxPages: 1024);
var engine = new LinksEngine<ulong>(journal,
    new() { RequireDurableFlush = true });
```

Keep the device alive while using the engine and sessions. Disposing it closes
every acquired handle and reports cleanup failures as an `AggregateException`;
uncertain close outcomes retain the ownership file. Disposing never silently
flushes staged records.

Tests consume independently generated wire vectors, inject finite durable-image
write/sync failures, reject malformed checksummed structures, reopen actual
files and bound real process-exit probes. The normal actual-WASM conformance
runner exercises 16 combinations under all three JS runtimes, including two
real-file opens, shared flushes, replica acknowledgement, rejected constraints
and following writes. Mirrored mode adds another sixteen combinations with
damaged data/metadata between opens. Finite tests are not physical power-loss certification.

Select two-copy repair in JavaScript with a separate import:

```js
import { FileMirroredPagedCommitLog } from '@link-foundation/link-cli-client/engine-file-mirrored-pages';
using log = new FileMirroredPagedCommitLog('existing-directory/mirrored-journal');
```

The portable `./engine-mirrored-pages` entry exports `MirroredPageDevice`, which
wraps two distinct caller-owned `RepairPageDevice` instances. It exposes the same
`PageDevice` contract to `PagedCommitLog`. A repair-capable device can overwrite
existing pages or fill a contiguous missing suffix; the decorator invokes this
privileged operation only after validating the complete logical prefix. Native
files use `path` and `path + '.mirror'`, each with metadata and its own exclusive
writer file. A plain reader rejects mirrored metadata rather than dropping the
second-copy requirement. Every language example accepts `--mirrored` for new
databases.
Use the same immutable physical mode on every subsequent open. Both copies must
be synchronized before acknowledgement; selected storage and writes double.
One-copy damage is repairable only while the expected valid page/decision
survives in the other copy. Losing both copies is outside that guarantee. This
strategy adds neither consensus nor physical power-loss certification.

Rust selects the same strategy at the device boundary:

```rust
use link_cli::engine::paged::{MirroredPageDevice, PagedLog};
let device = MirroredPageDevice::open("existing-directory/mirrored-journal")?;
let log = PagedLog::open(device)?;
// Use the existing TransitionLogStore API; release engine users before close.
log.into_device().close()?;
```

`MirroredPageDevice<D>` can also own two caller-supplied `RepairPageDevice`
implementations. Extracting devices does not flush staged records. A native
explicit close attempts both devices and preserves uncertain ownership files.

C# selects an owned native pair through `FileMirroredPageDevice`:

```csharp
using var device = new FileMirroredPageDevice("existing-directory/mirrored-journal");
ICommitLog journal = new PagedCommitLog(device, maxPages: 1024);
```

Portable `MirroredPageDevice` instead borrows two distinct caller-owned
`IRepairPageDevice` implementations. Keep both alive until all log/engine users
finish. Disposing the native pair attempts every acquired handle, aggregates
cleanup failures and retains uncertain owner files; it never commits staged data.
