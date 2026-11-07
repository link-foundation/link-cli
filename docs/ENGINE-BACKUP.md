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
  is refused with "revision is not retained". There is no replay of a separate WAL
  archive to an arbitrary moment.
- **Restoring earlier starts a new timeline.** The restored allocator is that
  revision's, so addresses the original engine used later can be issued again. Nothing
  tracks this divergence, so do not merge the two timelines.
- **Full copies only.** Every backup copies everything it selects.
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

## Compared with other engines

Checked on 2026-11-07 against each project's documentation.

| Engine | Their behaviour | Here |
| --- | --- | --- |
| [SQLite online backup](https://www.sqlite.org/backup.html) | The copy can be incremental, so the source is locked only "for the brief periods of time when it is actually being read from". A write from another connection usually restarts the copy, and with enough writes it "may never run to completion". | `backup_to` takes one cut and never restarts. The lock is held for all the encoding at once, rather than in small steps. |
| [RocksDB BackupEngine](https://github.com/facebook/rocksdb/wiki/How-to-backup-RocksDB) | "Backups are normally incremental". It keeps several backups and purges old ones. Restore compares checksums and returns `Status::Corruption` on a mismatch. | Not incremental, and one destination holds one backup. Verification is a read-back at backup time, with no stored checksum at restore. |
| [PostgreSQL continuous archiving](https://www.postgresql.org/docs/current/continuous-archiving.html) | A base backup plus archived WAL can restore "to its state at any time since your base backup was taken". Each recovery starts a new timeline, so later history is not overwritten. | Retained revisions only, and the copy ends at the selected revision. The timeline is not tracked. |

To get close to these with what exists today:

- **Keep several backups:** use one destination per backup and delete old ones yourself.
- **Reach many points in time:** select history `all` or a bound, then copy any retained revision.
- **Continuous copy to other logs:** add a [replicated log](ENGINE-STRATEGIES.md) as well. This is replication, and it also copies mistakes.
