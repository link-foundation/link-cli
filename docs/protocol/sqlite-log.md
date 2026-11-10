# SQLite journal storage protocol

The optional journal stores opaque ordered engine records in a dedicated SQLite
file. It owns its connection. One engine coordinates live sessions; independent
cached engines are not distributed transactions. Stale-owner checks prevent a
second owner from appending after another owner advanced its cached tail.

| Field | Required value |
| --- | --- |
| `PRAGMA application_id` | `0x4c4e4b4a` (`LNKJ`) |
| `PRAGMA user_version` | `1` |
| Runtime SQLite | At least `3.51.3` |
| `PRAGMA journal_mode` | `wal`, file backed |
| `PRAGMA synchronous` | `2` (`FULL`) |

An unclaimed database has zero application/user versions and no schema objects.
Claim it under `BEGIN IMMEDIATE`, with the following exact table declaration:

```sql
CREATE TABLE link_cli_log (sequence INTEGER PRIMARY KEY CHECK(sequence>0), record BLOB NOT NULL CHECK(typeof(record)='blob'))
```

The owned database has exactly this schema object, with the exact stored SQL
above. Reject other schema versions, changed declarations, triggers, views or
extra tables/indexes. Do not reinterpret an existing foreign database as a log.
Sequences start at one and are contiguous positive signed i64 values, independent
of engine revision/address types. Sequence exhaustion rejects before appending.
At open, positive unique integer keys with COUNT=MAX and MIN=1 (or all zero for
an empty table) establish continuity without reading every payload.

`record` contains strict UTF-8 BLOB bytes. Preserve empty strings, embedded NUL,
Unicode (including leading U+FEFF) and literal protocol delimiters. Physical CR/LF is invalid; logical
engine names already use encoded payloads. Reading validates each BLOB type,
UTF-8 payload, sequence and final expected tail before returning records. JS
returns the usual JSON array string at the existing commit-log boundary.

The connection caches its expected tail at open. First append begins an immediate
SQL transaction, verifies WAL/FULL and checks the actual tail while holding SQL
write ownership. It stages one sequence/record without committing. Later appends
share that transaction. Read includes the owner's pending records; independent
connections see only committed records. Flush COMMITs all pending records, then
releases SQL ownership; no pending records means no new durability operation.
Closing rolls pending work back and closes the connection without committing.

Read never resets the expected tail from an external writer. A stale tail or
component I/O error fences the adapter until close/reopen, because errors can
follow successful writes/COMMIT. Best-effort ROLLBACK releases a still-active
transaction without deleting committed rows. Reopening discovers the actual
committed prefix. Direct external table/connection mutation is outside ownership.

Busy waits and automatic checkpoints are selected explicitly. Rust/JS default
to SQLite busy_timeout=0; C# also bounds the provider's separate command retries
to one second by default (zero would mean unlimited). Auto-checkpoint defaults to
1000 pages; zero selects explicit maintenance. Passive/full/restart/truncate
checkpoint requests reject pending appends and return actual busy, log-frame and
checkpointed-frame counts. A blocked reader is reported, without an unbounded
retry. Checkpointing transfers pages; it does not discard logical journal rows.

[Independent vectors](sqlite-log-vectors.json) define payload bytes, malformed
UTF-8, schema and sequence controls. The component needs a same-host filesystem
with SQLite WAL shared-memory/locking and truthful VFS synchronization; retain
all SQLite sidecars. Process exits and copied-frame damage are distinct from
physical power loss. See the [active plan](../case-studies/issue-138/SQLITE-JOURNAL-PLAN.md).
