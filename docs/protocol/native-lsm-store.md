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
Native exclusive directory ownership, physical process-exit composition and
cross-language whole-store file interchange remain required work in
[the issue plan](../case-studies/issue-138/PHYSICAL-LSM-PLAN.md).
