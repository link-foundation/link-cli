# Checking and repairing a damaged journal

The staged engine's journal is its database: every committed revision is one
line, `engine2|<revision>|...|<checksum>`, and opening the engine replays the
lines in order. Opening refuses a journal with any bad record. It never skips a
record or guesses. A damaged journal therefore stops the engine instead of
silently opening a different history. This page covers what to do next, and
how CI kills and damages the engine on purpose to check it.

## What opening already handles

| Damage | What opening does |
| ------ | ----------------- |
| A writer killed mid-append (torn final line, no newline) | The exclusive file logs (`FileTransitionLog`, `FileCommitLog`) drop the incomplete last line. That commit was never acknowledged, because acknowledgement follows the flush. |
| Power loss that kept the newline of a record that was never flushed while a sector before it reads back as zeros | Records never contain NUL (appends refuse it), so the exclusive file logs drop trailing lines that hold one, like a torn final line. etcd's WAL treats a last record with a zeroed sector the same way (`isTornEntry`). A NUL line in front of an intact line is damage: opening refuses it and repair decides. |
| A flipped bit, a zeroed sector, invalid UTF-8, a reordered or foreign record | Refused: `journal checksum mismatch`, or the revision, allocator, address or constraint rule that failed. The checksum is 16 lowercase hex digits, and a checksum that differs only in letter case is refused too. |
| A primary that lost a tail its replica still holds (a truncated file, an old backup) | `ReplicatedLog` / `ReplicatedCommitLog` refuse to open: `replica 0 holds N records the primary lacks; repair the primary from it before writing`. Writing on top would fork the history and lose those commits for good. An unreadable or divergent replica is ignored here, because it proves nothing about the primary. |
| A full disk | The append or flush fails as an **indeterminate commit**: the engine stops accepting writes, and reopening shows whether the commit landed. Once space returns, the writer resumes. |
| A writer killed holding its lock | The Rust and C# `.lock` files take OS locks, which end with the process. The JS adapter's `<path>.writer` lock directory stays behind. Remove it only once the old process is known to be dead (fencing), never while it may still run. |

## Check, recover, repair

The same three steps exist in every language. They apply the replay rules of
opening, so a journal that passes the check opens.

| Step | Rust (`link_cli::engine::recovery`) | C# (`EngineJournal<T>`) | JS (`WasmEngine`, `engine-file-log.js`) |
| ---- | ----------------------------------- | ----------------------- | --------------------------------------- |
| Report the longest prefix that replays and the first bad record | `check_journal::<T>(&entries, &options)` | `Check(records, options)` | `WasmEngine.checkJournal(binding, journal, options)` |
| Pick the longest valid prefix among copies | `recover_journal::<T>(&copies, &options)` | `Recover(copies, options)` | `WasmEngine.recoverJournal(binding, copies, options)` |
| Rewrite a file journal from it | `repair_journal_file::<T>(path, &[copies], &options)` | `RepairFile(path, copies, options)` | `repairJournalFile(path, recover, { copies })` |
| Read a damaged file without failing on invalid UTF-8 | `read_journal_file_lossy(path)` | `ReadFileLossy(path)` | `readJournalFileLossy(path)` |

```rust
let repair = repair_journal_file::<u64>(Path::new("links.journal"), &[Path::new("replica.journal")], &options)?;
println!("revision {} from copy {}", repair.recovery.revision, repair.recovery.source);
```

```csharp
var repair = EngineJournal<ulong>.RepairFile("links.journal", ["replica.journal"], options);
```

```js
const repair = repairJournalFile(
  'links.journal',
  (copies) => WasmEngine.recoverJournal(binding, copies, options),
  { copies: ['replica.journal'] },
);
```

`recover` takes the damaged journal first, then any replicas, mirrors or
restored backups. It keeps the longest valid prefix. Each other copy's valid
prefix must be a prefix of it, and copies that disagree are refused, because
each may hold commits the other lacks. A copy compacted into an `engine3`
checkpoint disagrees with an uncompacted copy of the same history, so choose
one of them yourself.

The file repair:

1. takes the journal's writer lock, so it fails with `busy` while an engine
   owns the journal;
2. keeps the original bytes in `<path>.damaged-<n>`, never overwriting earlier
   evidence;
3. writes the repaired journal beside the original, flushes it, renames it over
   the path and flushes the directory;
4. reports `changed: false` and touches nothing when the file already holds
   exactly the recovered journal.

Copies are only read; their writers must be stopped.

## Which repair to run

- **A record in the middle is damaged and a replica is intact**: repair the
  primary with the replica as a copy. Nothing acknowledged is lost. The crash
  test's `bit-flip` and `zeroed-sector` phases check this.
- **The primary is behind its replica** (the open refusal above): repair the
  primary with the replica as a copy. This is the `truncated-tail` phase.
- **The replica's extra records are themselves damaged**: the writer keeps
  refusing, because the replica still has more lines, and the primary's repair
  has nothing valid to add. Repair the replica with the primary as its copy,
  then start the writer
  (`a_replica_with_a_damaged_extra_record_is_repaired_before_the_primary`).
- **No other copy exists**: the repair keeps the valid prefix and drops
  everything from the first bad record on. The result says so: each check's
  total records minus its valid records (`discarded_records()` /
  `DiscardedRecords`) counts the dropped records. The
  dropped commits were acknowledged, so treat this like any restore from an older
  backup. This is the `no-copy` phase. Keep replicas or
  [backups](ENGINE-BACKUP.md) if that loss is not acceptable.

Limits:

- The checksum detects accidental damage, not tampering.
- Damage to the final newline turns the last record into a torn tail, which
  opening drops by design. Without a replica that commit is gone, as after a
  crash before its flush.
- A truncation that ends exactly on a record boundary looks like a shorter,
  intact journal. Only a replica can show that commits are missing. That is why
  the replicated log refuses a primary behind its replica.

## How other databases do it

| Database | Damaged log or file | Repair tool | What the tool can lose |
| -------- | ------------------- | ----------- | ---------------------- |
| Redis AOF | A truncated last command is dropped on load when `aof-load-truncated` is on (the default); corruption in the middle aborts startup | `redis-check-aof --fix` | "all the AOF portion from the invalid part to the end of the file may be discarded" ([Redis persistence](https://redis.io/docs/latest/operate/oss_and_stack/management/persistence/)) |
| SQLite WAL | A frame is valid only if its salts and its checksum, "computed consecutively" over the frames before it, match ([file format](https://sqlite.org/fileformat2.html)) | `.recover` / the recovery extension | "Some content might be permanently deleted", "Previously deleted content might reappear", constraints may not hold ([recovery](https://sqlite.org/recovery.html)) |
| PostgreSQL | Replay stops at the end of valid WAL; `pg_amcheck` "checks for corruption" and repairs nothing ([pg_amcheck](https://www.postgresql.org/docs/current/app-pgamcheck.html)) | `pg_resetwal`, "only as a last resort" | "the database might contain inconsistent data due to partially-committed transactions"; dump, `initdb` and restore ([pg_resetwal](https://www.postgresql.org/docs/current/app-pgresetwal.html)) |
| This engine | Torn tail dropped; any other bad record refused; a primary behind its replica refused | `repair_journal_file` / `RepairFile` / `repairJournalFile` | Records from the first bad one on, and only when no copy holds them; the original bytes are kept |

The engine follows the Redis and SQLite-WAL rule (keep the valid prefix, drop
the rest) rather than PostgreSQL's reset or SQLite's salvage. A prefix of
whole commits is still a consistent revision, so the repaired journal never
contains half a transaction. Unlike `redis-check-aof --fix`, the repair also
reads replicas and keeps the damaged bytes.

## The crash test

`.github/scripts/docker-crash.mjs` runs a writer for one language (the Rust
example `engine_crash_writer`, the C# project `examples/engine-crash-csharp`, or
the JS writer `js/client/examples/engine-crash-writer.mjs` on the WASM engine) in
a Docker container. The writer commits forever to a replicated journal with two
synchronous acknowledgements and prints `ack R` once revision R is durable on
both files. Revision R holds links 1..R, each pointing at itself, so a torn
transaction shows up as a link pointing at nothing.

1. **Kills**: SIGKILL at a seeded random moment. A quarter of the kills land
   within 400 ms of the start, while the writer opens, replays and catches its
   replica up; the rest land between and inside commits. After each kill, the primary must
   hold every acknowledged revision plus at most the one being written, and the
   replica must hold every acknowledged revision. Both must open without a torn
   transaction.
2. **Power loss** (`--power-losses`, 8 by default): more kills, each
   followed by the damage a power cut could leave in the files. Bytes before the durable bound survive. Past it, the file keeps
   none of its tail, all of it, or a random-length part (one third each). Each
   512-byte sector past the bound then reads back as zeros with probability
   1/2, so a newline can survive while the record before it does not. The
   replica's bound is the end of `max(acknowledged, replica revision)`. The
   primary's bound is the end of `max(acknowledged, primary revision, newest
   intact replica record)`, because a commit reaches the replica only after
   the primary flushed it. The replica is cut first. The driver predicts the
   revision each file must open at and checks that the writer's verifier agrees.
   If the replica then refuses to open, the driver checks the refusal and
   repairs it from the primary. With `--seed 138 --rounds 2 --power-losses 16`,
   the Rust writer from before the trailing-NUL fix refuses to reopen its
   journal; the fixed writer passes.
3. **Damage**, each checked to be refused, then repaired, with the damaged bytes
   kept:
   - `bit-flip`: a flipped bit in a record that is not the last;
   - `zeroed-sector`: a zeroed 512-byte sector;
   - `truncated-tail`: a truncated primary. A writer must refuse to start, then
     the repair from the replica restores it;
   - `no-copy`: damage with the replica deleted. Only this phase may lose
     acknowledged revisions, and the run reports how many.
   After them, the restarted writer must rebuild the replica, and three more
   kills follow. One clean commit must then leave the two files byte for byte
   identical.
4. **Full disk**: the writer runs on a 64 KiB tmpfs. It must stop with an
   indeterminate-commit error, both files must open at acknowledged revisions,
   and after space is freed the writer must continue from where it stopped.

The **Crash tests** workflow (`.github/workflows/crash.yml`) runs the driver for
Rust, C# and JS on the runner's Docker daemon. It runs the Rust writer once more
from inside a `docker:dind` container, which starts its own daemon. It runs on
pull requests that touch the engine, on `main`, and weekly with new seeds. Every
run prints its seed, and a failed run uploads its damaged files. To replay
locally:

```sh
cargo build --release --manifest-path rust/Cargo.toml --example engine_crash_writer
node .github/scripts/docker-crash.mjs --target rust --seed 5 --rounds 12
```

For C#, build with `dotnet build -c Release examples/engine-crash-csharp`. For
JS, run `wasm-pack build --target web --out-dir ../../js/pkg rust/wasm`. If
your Docker host itself runs in a container, append `--storage-driver=vfs` to
the `docker:dind` command.

What it does not cover: a real power cut (the power-loss rounds model one at
the file level, the way LazyFS or dm-flakey would, but in the driver, not below
the file system), lost directory entries, and a machine whose clock or kernel
misbehaves. SIGKILL ends the process but leaves the page cache in place, so the
kill rounds check the engine's write and flush order, not the disk's. The
engine's unit tests cover flush failures with fault logs instead, and the
simulation below covers reordering inside a flush interval and disks that lie.

## The deterministic disk simulation

FoundationDB and TigerBeetle run their real storage code against a simulated
disk driven by one seeded random source, so every fault schedule replays
exactly. `rust/tests/engine_simulation_tests.rs`,
`csharp/Foundation.Data.Doublets.Cli.Tests/EngineSimulationTests.cs` and
`js/client/test/engine-simulation.test.mjs` (on the WASM engine) do the same for
the [native page format](ENGINE-NATIVE-PAGES.md). They share one random
generator (splitmix64), the same seeds and the same fault model.

A simulated `PageDevice` keeps three files (pages and two metadata slots).
Each file holds its durable bytes and the writes issued since its last sync. A
simulated power loss keeps, for every 512-byte sector independently, the
durable bytes or any prefix of the pending writes to that sector, sometimes
half of the next one (a torn sector), and a file length from any prefix of the
pending operations. That is every reordering and loss a volatile disk cache may
perform within a flush interval. Writes and syncs also fail at random, and the
failed call may or may not have reached the cache. Three clients commit through
the real paged log and engine until the power fails at a random moment.
Recovery itself truncates and repairs, so injected errors can hit it too; the
power then fails again and recovery is retried. 24 seeds run 8 crash cycles each, under four configurations
(optimistic and pessimistic concurrency, exclusive concurrency with
`require_durable_flush`, change-encoded revisions, shared and sorted-run
states, bounded history).

After every crash the database is reopened, and it must hold the exact state
of one revision between the last acknowledged and the last attempted commit:

- **Honest disk.** No acknowledged commit is lost and opening never refuses.
  Commits whose flush failed are found both applied and not applied.
- **Lying disk** (half of its syncs are acknowledged but persist nothing). No
  engine can keep its promise on such a disk. The test requires the next best
  thing: recovery either refuses the damaged database or opens the exact state
  of an earlier revision, never a mixture of two.
- **Mirror** ([mirrored mode](protocol/native-pages-mirrored.md)) of two disks,
  one of which may lie. No acknowledged commit is lost. With an honest primary
  the mirror always opens, because the secondary copy of a metadata decision is
  written only after the primary copy is durable, so power loss alone tears at
  most one copy.

The simulation found this ordering problem. Mirrors used to write both
metadata copies before syncing either, so a power loss could tear both copies
of an unacknowledged decision. Opening then refused a database whose other
slot still held the acknowledged decision.
The fix orders the writes and opens the other slot only in the one
both-unreadable state that power loss can leave: a torn primary next to a
secondary that was never written. Two damaged copies, which need a second
fault, are still refused rather than silently rolled back.

### The journal-file and LSM backends

`rust/tests/engine_power_loss_backends_tests.rs` puts the two other durable
backends on the same disk model (`rust/tests/engine_simulation/disk.rs`):

- **Journal file.** The real `FileTransitionLog` writes newline-framed entries
  to a temporary file, and every append and sync is mirrored to a simulated
  file. Each append is two writes, the entry and its newline, so a power loss
  can keep an entry without its newline or a newline after a zeroed entry. At
  power loss the simulated crash image becomes the real file. The real
  `FileTransitionLog::open` (its torn-tail repair) and `Engine::open` then
  recover it. The repair's truncation is itself an unsynced length change, so
  a later power loss can undo it. Three configurations (optimistic with a sync
  per append, pessimistic snapshot with change-encoded revisions, exclusive with
  `require_durable_flush`) run 48 seeds of 8 crash cycles each. The checks are
  the same as for the page device. On an honest disk no acknowledged commit is
  lost and opening never refuses. On a lying disk the log rolls back to the
  exact state of an earlier revision or is refused.
- **LSM.** The real `PhysicalLsmJournalStore` keeps its checkpointed write-ahead
  log (a root and two banks) and every sorted table on simulated devices.
  Creating and deleting a table are durable directory operations, as the
  namespace contract requires. Random groups of keys are compare-exchanged and
  single keys compare-deleted, with stale guards for conflicts, between
  flushes and compactions. Power fails at a random moment, including inside a
  flush or compaction. The reopened store must read exactly the key state
  after one acknowledged or attempted group. On an honest disk nothing is lost
  or refused.

SQLite is not simulated. The engine opens it in WAL mode with
`synchronous=FULL`, which `engine_sqlite_tests.rs` asserts, and SQLite's own
crash tests already replay power loss through a VFS that loses and tears
unsynced sectors. The simulations model no file-system metadata beyond the
LSM table namespace (no renames, no lost directory entries of the journal
file).
