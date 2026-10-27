# Physical LSM journal store

`PhysicalLsmJournalStore` composes the [native-lsm1 WAL](native-lsm-wal.md)
with actual [immutable doublets tables](native-sorted-table.md). Select
`physical-lsm` in Rust, the class in C#, or the separate
`@link-foundation/link-cli-client/engine-lsm-store` module in JavaScript.
The ordinary engine defaults and SQLite dependency selection are independent.

The supplied WAL must support atomic complete-log replacement, synchronous
append/read/flush, and durable flush when durable operation is requested.
The namespace must provide exclusive access for the lifetime of the store:

| Operation | Required behavior |
| --- | --- |
| List(maxEntries) | Complete, strictly increasing positive u64 IDs, or fail within the bound. Never return a partial successful listing. |
| Create(id) | Exclusively create a new device; never reuse an existing name. A failed call may leave an unselected partial device, but must release any lease it did not return. |
| Open(id) | Open an existing device exclusively; never create a missing selected file. |
| Close(device) | Release its lease without flushing pending writes or committing staged repair. Permit an explicit cleanup retry after an uncertain close result. |
| Delete(id) | Retire an unleased device. A successful durable operation must synchronize its namespace change. |

The namespace owns naming, directory synchronization and physical device
lifetimes. Its durability capability is stable while selected; a durable
namespace returning a volatile table device is rejected. Merely implementing
this interface does not establish an operating system persistence guarantee.
JavaScript operations must return synchronously; append/flush/close/delete and
recovery completion return no value. Its WAL replacement returns a boolean
indicating whether physical replacement occurred; either result is successful.

The public atomic journal contract reads all selected keys together and checks
every expected generation, including keys with no update. One changed group
advances a global nonreused u64 generation. A conflict or exact no-op has no
journal/namespace mutation. `null` in a CAS update leaves that key unchanged;
maintenance deletion uses CompareDelete and retains a positive-generation
tombstone. A live empty journal is distinct from a tombstone and appears in
ScanNamespace. Returned arrays are owned snapshots. Scans either return the
complete admitted live namespace or fail; keys-only scans omit payload reads.

Total-key, logical-byte, buffer, selected-run and WAL bounds use the WAL options.
MaxNamespaceEntries defaults to 256 and permits 2..65536. A flush needs room for
the old selected runs and one new run until the publication decision is durable.
Tombstones continue consuming total-key/byte capacity. A single changed group
must fit the selected buffer even when prior contents can be flushed. Actual
encoded checkpoint, table and following-group sizes are checked before the
first maintenance mutation. The caller must also bound its provider's complete
listing/log allocation before returning those containers.

A successful changed CAS appends and flushes its complete WAL group before
publishing its live buffer. Flush reserves a new monotonically increasing table
ID through atomic WAL replacement, creates and synchronizes a sealed table,
then replaces the WAL with its manifest binding and covered generation. Compact
performs a full foreground merge; reaching MaxRuns also selects this merge.
Only after durable selection are obsolete leases closed and their files retired.
Every selected table retains its identity, digest, count and encoded byte length.
Any error after a mutation may have taken effect fences reads and writes until
actual recovery; the store does not blindly retry a publication.

Recovery validates the complete semantic WAL, ordered namespace, every selected
physical table and descriptor, all historical same-generation duplicates, the
combined latest capacity, and the presence of the global generation. It then
completes staged table repairs and deletes only reserved unselected devices.
A missing/corrupt selected table, foreign unreserved name or conflicting old
version is an error; another run never substitutes for that selected decision.
Pure validation failure does not complete staged table repairs. Cleanup errors
are retained alongside constructor errors.

Rust owns the WAL and namespace values, returns them through into_parts, and
closes table leases explicitly; Drop is best effort. JavaScript close and C#
Dispose release table leases without closing the caller's WAL or namespace.
Cleanup can be retried after a failed close. Retain shared provider handles when
constructor-failure cleanup must be retried, because no store is returned.

Disk payloads remain in immutable pages. The live index retains keys,
generations, tombstone flags, logical costs and owning table IDs; buffered rows
own their payloads. Startup reads all selected rows for validation. Point reads
use their selected table's page index; scans and full compaction read selected
payloads. Replacing a hot-key WAL history avoids forcing a table flush. Full
compaction rewrites the complete latest admitted set and retains tombstones;
there is no background worker, leveled merge policy, Bloom filter or compression.

The portable API starts with an already constructed WAL. A native directory
facade must validate physical WAL discovery and selected tables before calling
a constructor that repairs or truncates that WAL. The current generic store
cannot retroactively prevent effects performed by its caller before Open.
JavaScript supplies native exclusive directory ownership and physical process-exit
composition below. Rust/C# parity and cross-language whole-store file interchange
remain required work in
[the issue plan](../case-studies/issue-138/PHYSICAL-LSM-PLAN.md).

## JavaScript native directory selection

`FileLsmJournalStore` from `@link-foundation/link-cli-client/engine-file-lsm-store`
composes the portable store with actual native checkpoint and table files. Use
an existing dedicated directory. This explicit entry adds native filesystem
imports; ordinary root imports still omit the physical LSM and SQLite.

The constructor validates semantic limits and finite physical bounds before
acquiring `.lsm-writer` with exclusive creation. It then streams the directory
within a finite entry bound, accepting only `wal`, `wal.bank0`, `wal.bank1`,
canonical `table-<positive-u64>` names and their `.meta0`/`.meta1` companions.
`mirroredRoot`, `mirroredBanks` and `mirroredTables` independently admit the
corresponding `.mirror` copies; all default to false. Keep the selected mode
fixed for that directory. Foreign names, symlinks, stale child owners and table
identities beyond the WAL reservation fail without removing evidence. An
incomplete orphan still occupies its table identity.

Discovery reads existing regular files without creating missing companions,
allocating identities, completing repairs, truncating suffixes or retiring
orphans. The complete selected checkpoint prefix and semantic WAL, every
historical selected table binding, the latest generation and combined capacity
must validate first. A surviving valid metadata slot or complete mirror may
justify missing redundant files. A wholly uninitialized WAL requires an empty
table namespace. Only after closing temporary discovery handles does the
constructor acquire writable devices under the same outer owner and perform
ordinary recovery. This bounded second read trades opening work for validation
before recovery effects. It assumes all writers obey the ownership contract.

`maxPages` bounds checkpoint-bank discovery and publication to 1..65536 pages,
default 16384. `maxNamespaceEntries` bounds table identities to 2..65536, default
256. Semantic limits retain the portable defaults. Physical page admission also
includes padding at every acknowledged flush: a semantic record may fit the
byte limit while exceeding the independently selected physical page limit.
Such an unsuccessful write is unacknowledged and may fence the current owner;
reopen to establish its outcome. A single-page WAL can initialize the empty
checkpoint but cannot acknowledge a following group.

Creation synchronizes file contents and, on Unix, the parent directory. Table
retirement synchronizes the directory on Unix. Windows file-content flushing
alone does not establish durable namespace changes, so
`supportsDurableFlush` is false there. Process exits and injected syscall replies
are tested; these do not certify physical power-loss behavior on every device.

Close the engine and sessions before closing this store. Closing attempts all
known discovery/table/WAL handles before releasing the outer owner. A failed
acquisition or cleanup that may leave a handle live retains `.lsm-writer`;
uncertain raw closes are cached and never retried. A marker-removal error occurs
only after every known child and owner handle has closed; an error reported after
that removal can leave the marker absent, with the old facade already closed. Production opening never steals a fence. After a
crash, an external recovery authority must fence or reap the previous process
and establish that its handles are closed before explicitly removing recognized
owner markers. File hashes and corrupt images remain available for diagnosis.
Native Rust/C# facade parity and whole-store cross-language interchange remain
required follow-up work in issue #138.

The [runnable actual-WASM example](../../examples/native-lsm-js/README.md) verifies
direct, partitioned and epoch-reclaiming cluster consumers over these files in
Node, Bun and Deno, including coordinated checkpoints, stale conflicts and reopen.
