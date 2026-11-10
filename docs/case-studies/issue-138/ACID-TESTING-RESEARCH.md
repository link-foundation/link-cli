# How open-source databases provide and test ACID under concurrency (2026-11-09)

This answers two requests from the
[latest PR feedback](https://github.com/link-foundation/link-cli/pull/139#issuecomment-6739948171):
"check all popular database solutions with open-source code and check their
strategy on providing ACID guarantees in concurrent environments", and "check
how they do testing". It covers the most used open-source databases and the
most used ways to run concurrent writes. Exotic algorithms and performance are
out of scope, as the feedback asks.

Each quote below was read from the linked page on 2026-11-09. Where a row says
what we do, it names the file that does it. The comparison is by design, not
by code: we reuse none of these projects' code or formats.

## Part 1. Concurrent writes: what each database does and what we offer

| Database | How concurrent writes are handled (quoted) | Closest option here |
| --- | --- | --- |
| SQLite WAL ([wal.html](https://sqlite.org/wal.html)) | "since there is only one WAL file, there can only be one writer at a time" | `Exclusive` concurrency: a second `begin()` fails while a session is open. The optional SQLite log keeps SQLite's own single writer. |
| PostgreSQL ([MVCC intro](https://www.postgresql.org/docs/current/mvcc-intro.html), [isolation](https://www.postgresql.org/docs/current/transaction-iso.html)) | "reading never blocks writing and writing never blocks reading". Under READ COMMITTED "the would-be updater will wait for the first updating transaction to commit or roll back". REPEATABLE READ rolls back with "could not serialize access due to concurrent update". SERIALIZABLE rolls back with "could not serialize access due to read/write dependencies among transactions". | Snapshot isolation (readers see a begin-time state and never block). `Pessimistic` concurrency with `lock_wait` is the waiting updater. A write/write conflict aborts the later commit, as in REPEATABLE READ. `Serializable` with read-set validation is the read/write dependency check, at address rather than predicate granularity. |
| MySQL InnoDB ([locking](https://dev.mysql.com/doc/refman/8.4/en/innodb-locking.html)) | "A next-key lock is a combination of a record lock on the index record and a gap lock on the gap before the index record" | `Pessimistic` point locks, plus `lock_for_update` like `SELECT ... FOR UPDATE`. A missing address is reserved, but there are no gap or range locks. |
| RocksDB ([Transactions](https://github.com/facebook/rocksdb/wiki/Transactions)) | "RocksDB supports both pessimistic and optimistic concurrency control". "Optimistic Transactions do not take any locks when preparing writes. Instead, they rely on doing conflict-detection at commit time" | Both: `Optimistic` (the default) checks at commit, and `Pessimistic` reserves at write time. `lock_for_update` is the counterpart of `GetForUpdate`. |
| MongoDB WiredTiger ([storage engine](https://www.mongodb.com/docs/manual/core/wiredtiger/)) | "WiredTiger uses document-level concurrency control for write operations" and "uses optimistic concurrency control". On a conflict, "one will incur a write conflict causing MongoDB to transparently retry that operation" | `Optimistic` concurrency with per-address conflict checks. We do not retry for the caller: a refused commit closes the session, and the caller retries with a new one. |
| Redis ([transactions](https://redis.io/docs/latest/develop/using-commands/transactions/)) | "WATCH is used to provide a check-and-set (CAS) behavior". "If at least one watched key is modified before the EXEC command, the whole transaction aborts". "Redis does not support rollbacks of transactions" | `Optimistic` + `Serializable` read-set validation: a read address that changed aborts the commit, like a watched key. Unlike Redis, a failed commit leaves nothing applied. |
| etcd ([API](https://etcd.io/docs/v3.6/learning/api/)) | Txn is "guarded (i.e., if) based on the contents of the key-value store. Transactions can be used for protecting keys from unintended concurrent updates, building compare-and-swap operations" | `ClusterEngine` + `EtcdJournalStore`: a distributed commit is one compare-and-swap of the catalog in etcd (see [distributed transactions compared](../../ENGINE-CLUSTERS.md#distributed-transactions-compared)). |
| FoundationDB ([developer guide](https://apple.github.io/foundationdb/developer-guide.html)) | "FoundationDB provides global, ACID transactions with strict serializability using optimistic concurrency". "not_committed is thrown whenever there was a conflict" | `Serializable` read-set validation checks the addresses a session read at commit, and refuses with a conflict. There is no distributed resolver or key-range conflict set. |

So the five common ways to run concurrent writes are all selectable, and each
can be combined with any isolation level:

1. **One writer at a time** (SQLite): `Exclusive`.
2. **Optimistic, checked at commit** (RocksDB optimistic, MongoDB, Redis WATCH,
   FoundationDB): `Optimistic`, the default.
3. **Pessimistic row locks, fail at once or wait** (PostgreSQL, InnoDB, RocksDB
   pessimistic): `Pessimistic`, with an optional bounded `lock_wait` that refuses
   deadlocks.
4. **Reading under a lock** (`SELECT ... FOR UPDATE`, `GetForUpdate`):
   `lock_for_update` in a pessimistic session.
5. **Compare-and-swap across machines** (etcd Txn): `ClusterEngine` over etcd
   or another atomic store.

Snapshot reads (MVCC) are an isolation choice here, not a separate concurrency
mode. [ENGINE-STRATEGIES.md](../../ENGINE-STRATEGIES.md#concurrency-and-isolation)
explains how to choose, and [its comparison table](../../ENGINE-STRATEGIES.md#costs-and-comparison)
lists what each match lacks.

Left out on purpose: gap/next-key and predicate locks, PostgreSQL's SSI
dependency graph, `FOR SHARE` and `SKIP LOCKED`, timestamp ordering, and
deterministic or multi-version concurrency schemes beyond snapshot reads. Each
is either exotic for this project or a performance refinement of an option we
already have. Where they matter, the address-level options refuse more than
the database would (a conflict the caller retries), never less.

## Part 2. How they test it, and what we test in the same way

### SQLite

[How SQLite Is Tested](https://sqlite.org/testing.html) lists, among others,
"100% branch test coverage in an as-deployed configuration", "I/O error tests",
"Crash and power loss tests" and "Malformed database tests".

- Crash testing "spawns a child process which runs some SQLite operation and
  randomly crashes somewhere in the middle of a write operation. A special VFS
  randomly reorders and corrupts the unsynchronized write operations to
  simulate the effect of buffered filesystems".
- I/O error testing "seeks to verify that SQLite responds sanely to failed I/O
  operations".
- Malformed database tests "verify that SQLite finds the file format errors and
  reports them using the SQLITE_CORRUPT return code".

Ours:

- **Crashes.** The [Docker crash test](../../ENGINE-RECOVERY.md#the-crash-test)
  (`.github/scripts/docker-crash.mjs`, `.github/workflows/crash.yml`) runs the
  Rust, C# and JS writers in a container and sends SIGKILL at seeded random
  moments. A quarter of the kills land during open, replay and catch-up. After
  each kill, every acknowledged revision must be on both copies.
- **I/O errors.** Fault logs fail every write or flush in turn:
  `rust/tests/transactions_journal_failure_tests.rs`,
  `engine_recovery_tests.rs`, `engine_changes_fault_tests.rs`,
  `sessions_faults_tests.rs`, and in C# `EngineFailureTests.cs`,
  `TransactionJournalFailureTests.cs` and `JournalTailFaultTests.cs`.
  `rust/tests/query_fault_sweep_tests.rs` fails every query at each step in
  turn. The crash test also fills a 64 KiB tmpfs: the writer must stop with an
  indeterminate-commit error and continue after space is freed.
- **Malformed files.** The same crash test flips a bit, zeroes a sector and
  truncates the tail. Each damaged file must be refused, then repaired from the
  replica; with no replica, the run reports what was lost.
  `EngineJournalRecoveryTests.cs` and the JS conformance suite
  `js/client/test/conformance/journal-recovery.mjs` check the same refusal and
  repair in every language.
- **Coverage.** CI measures coverage in every language; see
  [COVERAGE-METHOD.md](COVERAGE-METHOD.md) for what is measured.

- **Power loss.** SQLite's crash VFS reorders and drops writes that were never
  synced, which is what a power loss does. SIGKILL alone leaves the page cache
  in place, so the crash test adds power-loss rounds
  ([details](../../ENGINE-RECOVERY.md#the-crash-test)): after a kill, every byte
  past the durable bound may be cut off, and each 512-byte sector past it reads
  back as zeros with probability 1/2. That lets a record's newline survive
  while a sector of the record itself does not. The first run of this model
  (seed 138) found a real bug: the writers refused to reopen such a journal.
  Trailing lines holding a NUL byte are now dropped as torn in Rust, C# and JS,
  as etcd's WAL does with a last record holding a zeroed sector.

Gap closed for the page device: the Docker power-loss model runs in the
driver, on the files, after the writer is dead, so it cannot reorder writes
inside a flush interval or model a disk that acknowledges a flush it did not
do. The [deterministic disk simulation](../../ENGINE-RECOVERY.md#the-deterministic-disk-simulation)
does both, in Rust, C# and JS, for the native page format and its mirror. The
journal-file, LSM and SQLite backends are still covered only by the Docker
model.

### RocksDB

The [stress test](https://github.com/facebook/rocksdb/wiki/Stress-test) restarts
the database and checks it: "after each restart, all the data is verified".
"We also write to an external log file. Between after restarts, the database
state is compared with the log file". Its "black box crash" simply calls Linux
"kill -9"; its white-box crash points sit before and after file-system
operations.

Ours: the Docker crash test is the black-box form. The writer prints `ack R`
only once revision R is durable, and that output is the external record the
files are compared with after each kill. The fault logs above are the white-box
form: they fail before and after each journal write and flush.

### PostgreSQL

The [isolation tester](https://github.com/postgres/postgres/blob/master/src/test/isolation/README)
runs scripted sessions step by step. When a spec lists no permutations, "the
test program automatically runs all possible interleavings".

### Jepsen Elle

[Elle](https://github.com/jepsen-io/elle) "is a transactional consistency
checker for black-box databases. Based purely on client observations of
transactions" it finds anomalies such as G0 (dirty write), G1a/G1b (aborted and
intermediate reads), G1c, G-single (read skew) and G2 (anti-dependency cycles,
such as write skew).

Ours, combining both: `rust/tests/engine_interleaving_tests.rs`,
`csharp/Foundation.Data.Doublets.Cli.Tests/EngineInterleavingTests.cs` and
`js/client/test/conformance/interleaving.mjs` (on the compiled WASM engine) run
six two-transaction workloads in every interleaving of their steps, under 12
configurations (optimistic, pessimistic and exclusive concurrency; read
committed, snapshot, and serializable with revision or read-set validation).
Like Elle, the check uses only what the clients saw: the reads each committed
transaction returned and the final state must equal some serial order of the
committed transactions. A refused step rolls that transaction back.

The table of what each level prevents is checked in both directions. What a
level prevents must never appear, and what it allows must appear in some
interleaving, so a test cannot pass by refusing everything. At least one
interleaving must commit both transactions. The tests run 2,080 interleavings
in all. The counts are the same in Rust and WASM (interleavings / committed
both / not serializable):

| Workload | Read committed | Snapshot | Serializable (either validation) |
| --- | --- | --- | --- |
| Lost update: both read 1, then write 1 | 20 / 2 / 0 | 20 / 2 / 0 | 20 / 2 / 0 |
| Read skew: read 1 and 2, while the other writes both | 20 / 20 / 3 | 20 / 20 / 0 | 20 / 11 / 0 |
| Write skew: both read 1 and 2, each writes one | 70 / 70 / 64 | 70 / 70 / 68 | 70 / 2 / 0 |
| Write skew through a scan | 20 / 20 / 18 | 20 / 20 / 18 | 20 / 2 / 0 |
| Disjoint: each reads and writes its own row | 20 / 20 / 0 | 20 / 20 / 0 | revision 20 / 2 / 0, read-set 20 / 20 / 0 |
| Write skew with `lock_for_update` (pessimistic only) | 70 / 2 / 0 | 70 / 2 / 0 | 70 / 2 / 0 |

These numbers are for optimistic and pessimistic concurrency, which agree.
Exclusive concurrency always commits both transactions in exactly 2
interleavings (one after the other) and never shows an anomaly. The disjoint row
shows the cost of the default revision validation (it refuses independent work
whenever another commit happened) and why read-set validation exists. The
locked row shows that `lock_for_update` closes write skew under every isolation
level.

Real threads: `rust/tests/engine_lock_wait_tests.rs` and
`EngineLockWaitTests.cs` check bounded waiting and deadlock refusal of
pessimistic writes with native threads.

Long histories: `rust/tests/engine_history_tests.rs`, `EngineHistoryTests.cs`
and `js/client/test/conformance/history.mjs` (on the compiled WASM engine) do
what Elle does. A seeded scheduler runs 4 clients over 3 links, 60 transactions
per history and 20 seeds per configuration. Every write stores a unique value,
so each read names the transaction it saw, and commit revisions give each
link's version order. From these the checker builds Adya's dependency graph
(`ww`, `wr`, `rw` and real-time edges) and searches it for cycles. It reports
G1a, G1b, reads of values nobody wrote, lost writes, G0/G1c (a cycle without an
`rw` edge), G-single (exactly one `rw` edge) and G2 (any other cycle). As in the
interleaving tests, an anomaly a level allows must show up in some history.
The counts are pinned and equal in all three languages (committed transactions
/ histories with G-single / with G2, out of 20 histories):

| Concurrency | Read committed | Snapshot | Serializable (revision) | Serializable (read-set) |
| --- | --- | --- | --- | --- |
| Optimistic | 847 / 19 / 0 | 847 / 0 / 20 | 573 / 0 / 0 | 662 / 0 / 0 |
| Pessimistic | 635 / 17 / 2 | 635 / 0 / 19 | 453 / 0 / 0 | 515 / 0 / 0 |
| Exclusive | 148 / 0 / 0 | 148 / 0 / 0 | 148 / 0 / 0 | 148 / 0 / 0 |

Read committed shows G-single (and, under pessimistic concurrency, G2, which
it also allows); snapshot shows only G2, as Adya's levels allow. Serializable shows neither, and read-set validation commits more than
revision validation.

Distributed histories: the [network simulation](../../ENGINE-CLUSTERS.md#reproduce-verification)
feeds the same checker with histories from three `ClusterEngine` members whose
links to the journal store drop requests, lose replies, deliver late and are
cut. It must find no anomaly, including real-time edges between members, so
the cluster is checked to be strictly serializable.

### etcd

The [robustness tests](https://github.com/etcd-io/etcd/blob/main/tests/robustness/README.md)
inject "Network partitions, node crashes, disk failures". "The collected history
is validated against the etcd model", using gofail failpoints and Antithesis.
They check that "Etcd provides strict serializability for KV operations".

Ours: `experiments/issue-138/etcd-cluster.mjs` (CI step "Verify actual
consensus and language interoperability" in `.github/workflows/js-client.yml`)
starts a real three-member etcd v3.7.3 cluster. It commits distributed
transactions from Rust, C# and JS, kills the leader with SIGKILL, checks that the
surviving majority elects a leader and commits, and checks that a group without
a majority publishes nothing. Before that, it pauses the leader with SIGSTOP
(Jepsen's "pause" fault). To its peers a paused leader looks cut off by a
partition, and on SIGCONT it returns still believing it leads. The majority
must elect another leader and commit. A transaction staged before the pause
must not commit after it, and a read through the resumed member alone must
show the majority's write.

Network partitions: `experiments/issue-138/etcd-partition.mjs` cuts the Raft
links of the real cluster through per-member proxies that classify connections
by etcd's `X-Server-From` header. It runs a two-way isolation of the leader,
one-way cuts in both directions, a bridge, slowed links (50 ms on every link,
then the leader's links 1500 ms late, past the election timeout), and six
seeded random rounds of cuts and delays with a second fault injected
mid-round while four clients commit. Every member must
agree. Acknowledged transactions must be kept, refused ones absent, and
unknown ones all or nothing, and reads must respect real time. Building it
found that etcd's prober opens keep-alive connections without the sender
header that the pipeline then reuses for `POST /raft`, so a proxy that
classifies only the first request leaks a one-way cut.

Clock faults and liveness under random faults: a real etcd cluster's clocks
cannot be skewed from a test, so the
[Raft draft](../../ENGINE-CLUSTERS.md#the-raft-draft) is simulated instead.
Every replica's clock runs at 0.5x to 2x, jumps both ways and changes rate.
Replicas crash and restart, and links drop, delay and reorder messages or make
a replica isolated, deaf or mute. With one replica faulty at a time, faults
never stop, yet a probe must get an answer at least every 1,500 real-time
ticks (the longest gap over 20 seeds was 1,189). Turning PreVote and
CheckQuorum off on the same schedules is the negative control: the gap grows
past the bound (2,292) and terms climb from 8 to 64. With any number of
replicas faulty at once, progress may stop, but the Elle-style checker finds
no anomaly. The real cluster's history is checked against the stored state
and real time.

### FoundationDB

[FoundationDB's testing](https://apple.github.io/foundationdb/testing.html)
runs a "deterministic simulation of an entire FoundationDB cluster within a
single-threaded process. Determinism is crucial in that it allows perfect
repeatability". It injects faults such as "swizzle-clogging" of the network.

Ours: the crash test is seeded, so a failing run can be replayed with the same
seed, but the process schedule and the operating system are not deterministic.
The simulation tests are deterministic. One splitmix64 source, the same in
Rust, C# and JS, drives the disk (sector reordering, torn sectors, failed and
lying syncs, power loss) and the network (dropped, lost, late and partitioned
messages). The real engine runs on top, single-threaded, and a seed replays
the same history exactly. The disk simulation found a real bug in the mirror's
metadata write order.

The simulation now covers the storage backends and the consensus layer as
well as the page device. The journal file and the physical LSM store run
their real code on the same sector-level disk model (`engine_power_loss_backends_tests.rs`).
A Raft group simulates what etcd gets from the operating system: per-replica
clocks and timers, process crashes and the network.

Gap: the Raft group is this repository's draft of etcd's algorithm, not etcd
itself, and the simulation does not schedule threads. Each language's engine
is single-threaded inside the simulation, as FoundationDB's actors are.

## Summary of gaps

Closed on 2026-11-10:

1. **Power loss below the file system for every backend.** The page device and
   its mirror, the journal file and the physical LSM store run on the
   sector-level disk model. SQLite runs in WAL mode with `synchronous=FULL`,
   which a test asserts, and SQLite's own crash VFS covers it.
2. **Clock faults.** Skewed, jumping and rate-changing clocks drive the
   simulated Raft group's timers and client deadlines.
3. **Liveness during random faults.** Bounded progress is asserted while one
   replica at a time keeps failing, with a negative control.
4. **Whole-system simulation.** The Raft draft's timers, processes and network
   are simulated, not only the atomic journal store.

Still not claimed as covered:

1. A real etcd binary under skewed clocks. Its clock comes from the operating
   system, and only the partition runner exercises it, with real time.
2. File-system metadata beyond the LSM table namespace, such as renames and a
   lost directory entry of the journal file.
3. Thread scheduling: the simulations run each engine single-threaded. The
   concurrency tests use real threads, without a deterministic scheduler.
