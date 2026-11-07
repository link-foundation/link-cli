# Online backup and point-in-time copies

Copy a running engine to an empty log. Commits continue during the copy, and the
result opens like any other journal:

```rust
let revision = engine.backup_to(&mut destination, None)?; // current revision
let earlier = engine.backup_to(&mut other_destination, Some(42))?; // retained revision
let restored = Engine::<u32, _>::open(destination, options)?;
```

```csharp
ulong revision = engine.Backup(destination);
ulong earlier = engine.Backup(otherDestination, revision: 42);
var restored = new LinksEngine<uint>(destination, options);
```

```js
const revision = engine.backup(destination);
const earlier = engine.backup(otherDestination, { revision: 42n });
const restored = new WasmEngine(binding, options, destination);
```

Any commit log works as a destination; the tests use memory and file logs.
Backup is a method you call, so an engine that never calls it pays nothing.

## What happens

1. The destination is read. A non-empty destination is refused and left alone.
2. The commit mutex is taken. A poisoned engine refuses here. The retained snapshots
   up to the selected revision are encoded, and the mutex is released.
3. Records are appended and flushed with no engine lock held.
4. The destination is read back and compared with the copied records.
5. The revision the copy ends at is returned.

## Guarantees

| Guarantee | How it is provided |
| --- | --- |
| Consistent cut | Records are encoded from committed snapshots under the commit mutex. |
| Later commits stay out | The cut is taken once. Commits made during step 3 are not in the copy. |
| Private writes stay out | Uncommitted sessions and allocator reservations are never encoded. |
| Commits continue | The mutex is not held during destination I/O. Tests commit from inside the destination's `append`. |
| The engine is unaffected by the destination | A destination failure is returned to the caller. It does not poison the engine, and a retry on a new destination works. |
| Reopen matches | The copy reopens at the copied revision with its rows, names, retained history and allocator position. |
| Flushed | The destination's flush completes before success is returned. |
| Read-back | A destination that lost or changed records fails verification. |

## Costs and limits

- **Mutex hold time.** Step 2 is proportional to the retained snapshot contents. With
  history `all`, every retained revision is encoded in full. Commits wait during
  that encoding, and only then. Select a history bound if this matters.
- **Memory.** All copied records are held in memory until the copy finishes.
- **Failed copies.** A failure in steps 3 or 4 can leave a shorter copy that still
  opens as an older state. Discard the destination after any failure.
- **Durability follows the destination.** A volatile destination gives a volatile
  backup, and a flush that lies cannot be detected. Read-back proves the log returned
  the records, not that they would survive power loss.
- **Point in time means retained revisions.** A revision evicted by the history policy
  is refused with "revision is not retained". To reach revisions the running engine no
  longer holds, keep an archive and [restore it to a revision](#restore-to-a-revision).
- **Restoring earlier starts a new timeline.** The restored allocator is that
  revision's, so addresses the original engine used later can be issued again. Nothing
  tracks this divergence, so do not merge the two timelines.
- **`backup_to` makes full copies.** It copies everything it selects. Use an
  [incremental backup](#incremental-backup) to add to an earlier copy.
- **One engine.** A cluster has no combined cut. Shards in etcd or a partition store
  are copied with that store's own tools.
- **Exclusive destination.** Nothing else may write to the destination during the
  copy. A concurrent writer normally fails verification.
- **JavaScript.** The destination must be synchronous, like every WebAssembly commit
  log. An adapter that throws a non-string value is reported as a generic log failure.

`export_checkpoint_at(revision)`, `ExportCheckpoint(revision)` and
`exportCheckpoint(revision)` return the same records when you want to publish them
yourself. The record formats are those of
[retained-history checkpoints](ENGINE-CHECKPOINTS.md), so readers that predate
`engine3` reject them.

## Incremental backup

Bring an earlier backup up to the current revision. Only the revisions it lacks are
appended, each as the rows that changed. An empty destination gets a full copy, so the
same call can start and continue a backup:

```rust
let revision = engine.backup_incremental_to(&mut destination)?;
```

```csharp
ulong revision = engine.BackupIncremental(destination);
```

```js
const revision = engine.backupIncremental(destination);
```

1. The destination is read and replayed, with no engine lock held. A record that does
   not decode is refused.
2. The commit mutex is taken to select the retained snapshots from the destination's
   last revision onward, then released. Nothing is encoded under the mutex.
3. The destination's last revision must still be retained and hold the same rows and
   allocator position. Otherwise it is refused and left unchanged.
4. Changed rows are encoded, appended and flushed, then the whole destination is read
   back and compared.

The reopened copy has the same rows, retained history between its revisions and
allocator position as the source had at that revision. Commits made during the copy
stay out of it and are picked up by the next call.

| Situation | Result |
| --- | --- |
| Nothing new since the last call | Nothing is appended. The current revision is returned. |
| The destination's revision was evicted, or history is `none` and commits followed | "revision is not retained". Take a full backup to an empty log. |
| The destination is ahead of this engine | "revision is not retained". |
| The rows or allocator position differ at that revision | "backup destination is not a copy of this engine". |
| The destination is not a valid journal | The decoding error is returned. |
| Append, flush or read-back fails | The error is returned and the engine is not fenced. The destination holds a valid prefix unless a record was torn or changed. Calling again continues from where it ended. |

Limits:

- **Reads the whole destination.** Each call reads and replays every record, twice in
  the read-back case, so its cost grows with the backup, not only with the change.
- **Needs retained history.** An increment can cover only what the history policy still
  holds. With a bound of N, call it before N revisions pass, or take a new full copy.
  History `none` can only confirm that a copy is already current.
- **Compares only the last revision.** Earlier records of the destination must decode
  but are not compared with this engine. A different timeline that reaches the same
  rows and allocator position at the same revision is accepted.
- **Grows until compacted.** Increments are appended to one log, with no merge into a
  shorter full copy. Opening it with a smaller history bound replays all of it.
- **Restoring a chain.** All records live in one log, so losing or corrupting one record
  makes that log unusable from there. This is not a set of separate backup files.

## Restore to a revision

Copy an archive to an empty log, stopping at one revision, then open the copy. The
archive can be a backup, an incremental chain or a live journal of a stopped engine. No
engine has to be running, and leaving the revision out copies the whole archive:

```rust
let revision = restore_backup::<u32, _, _>(&mut archive, &mut destination, Some(42))?;
let engine = Engine::<u32, _>::open(destination, options)?;
```

```csharp
ulong revision = LinksEngine<uint>.RestoreBackup(archive, destination, 42);
var engine = new LinksEngine<uint>(destination, options);
```

```js
const revision = WasmEngine.restoreBackup(binding, archive, destination, { revision: 42n });
const engine = new WasmEngine(binding, options, destination);
```

1. The destination must be empty, otherwise it is refused and left unchanged.
2. The archive is replayed up to the selected revision. Any record that does not decode,
   or that breaks the revision or allocator order, refuses the restore before anything
   is written.
3. The records up to that revision are appended unchanged and flushed, then the
   destination is read back and compared.

| Situation | Result |
| --- | --- |
| The revision is in the archive | The copy reopens with that revision's rows and allocator position. The history it keeps follows the options you open it with. |
| The revision is newer than the archive's end, or older than its first checkpoint | "revision is not in the backup", nothing is written. |
| A record after the revision is damaged | The restore succeeds, because that record is never read. |
| A record up to the revision is damaged | The decoding error is returned and nothing is written. |
| Append, flush or read-back fails | The error is returned. The destination may hold a shorter copy, so discard it. |

Limits:

- **Revisions, not time.** Records carry no timestamp, so a restore names a revision. To
  find one, restore a candidate and look at its rows.
- **What the archive holds.** A bounded history starts the archive at a checkpoint, and
  earlier revisions cannot be reached. To reach everything, start the archive with
  history `all`.
- **A new timeline.** The restored allocator is that revision's, so addresses the
  original used later can be issued again. Nothing records the divergence, so do not
  merge the timelines or continue the old archive from the restored engine.
- **Constraints are checked on open.** The restore does not know your options. Opening
  the destination validates each revision against the constraints you select.
- **One log.** The archive is read into memory in full, and only one engine's journal
  can be restored. A cluster has no combined cut.

## Compared with other engines

Checked on 2026-11-07 against each project's documentation.

| Engine | Their behaviour | Here |
| --- | --- | --- |
| [SQLite online backup](https://www.sqlite.org/backup.html) | The copy can be incremental, so the source is locked only "for the brief periods of time when it is actually being read from". A write from another connection usually restarts the copy, and with enough writes it "may never run to completion". | `backup_to` takes one cut and never restarts. The lock is held for all the encoding at once, rather than in small steps. |
| [RocksDB BackupEngine](https://github.com/facebook/rocksdb/wiki/How-to-backup-RocksDB) | "Backups are normally incremental". It keeps several backups and purges old ones. Restore compares checksums and returns `Status::Corruption` on a mismatch. | `backup_incremental_to` appends changed rows to one log, which holds one backup with its revisions. Each record has a checksum checked on reopen, and the whole log is read back at backup time. |
| [PostgreSQL continuous archiving](https://www.postgresql.org/docs/current/continuous-archiving.html) | A base backup plus archived WAL can restore "to its state at any time since your base backup was taken". Each recovery starts a new timeline, so later history is not overwritten. | An archive restores to any revision it holds, chosen by number rather than time, and the copy ends there. The timeline is not tracked. |

To get close to these with what exists today:

- **Keep several backups:** use one destination per backup and delete old ones yourself.
- **Reach many points in time:** keep one archive with `backup_incremental_to` and [restore it to a revision](#restore-to-a-revision), or select history `all` or a bound and copy any retained revision.
- **Continuous copy to other logs:** add a [replicated log](ENGINE-STRATEGIES.md) as well. This is replication, and it also copies mistakes.
