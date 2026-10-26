# Native immutable doublets page journal

`FilePagedCommitLog` is an optional SQL-free JS/TS ordered-log provider for the
Rust-backed WASM engine in Node, Bun and Deno. It stores string chunks and record
links as native doublets, publishes one flush group through alternating roots,
and never overwrites committed data pages. The universal package entry imports
neither this filesystem adapter nor a SQL provider. The portable `PagedCommitLog`
codec is a separate `./engine-paged-log` entry accepting a synchronous `PageDevice`.

Rust and C# matching providers, cross-language file interchange and independently
selected redundant page repair are in progress. This provider does not implement
repair of legacy mapped Doublets files, LSM or efficient page undo. Its two
metadata decisions recover torn unacknowledged writes under the stated model;
they do not repair corruption of an acknowledged decision. Committed data-page
corruption rejects open rather than silently discarding a previously acknowledged
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

Tests consume independently generated wire vectors, inject finite durable-image
write/sync failures, reject malformed checksummed structures, reopen actual
files and bound real process-exit probes. The normal actual-WASM conformance
runner exercises 16 combinations under all three JS runtimes, including two
real-file opens, shared flushes, replica acknowledgement, rejected constraints
and following writes. Finite tests are not physical power-loss certification.
