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

Gap: SQLite's crash VFS reorders and drops writes that were never synced, which
is what a power loss does. SIGKILL leaves the operating system's page cache in
place, so our crash test checks the engine's write and flush order, but not a
disk that loses or reorders unsynced writes.

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

Gap: Elle checks long random histories of many transactions for dependency
cycles. Our check is exhaustive but only for two short transactions, and it
compares against serial orders by brute force rather than building a
dependency graph.

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
a majority publishes nothing.

Gap: no network partitions or clock faults, and no history checked against a
model beyond the stored state.

### FoundationDB

[FoundationDB's testing](https://apple.github.io/foundationdb/testing.html)
runs a "deterministic simulation of an entire FoundationDB cluster within a
single-threaded process. Determinism is crucial in that it allows perfect
repeatability". It injects faults such as "swizzle-clogging" of the network.

Ours: the crash test is seeded, so a failing run can be replayed with the same
seed, but the process schedule and the operating system are not deterministic.

Gap: there is no deterministic simulation of the whole system. Building one
would mean writing every I/O and timer path against a simulated runtime. That
is a large separate project, not a test we can add.

## Summary of gaps

None of these is claimed as covered:

1. **Power loss below the file system.** We test process death; reordered or
   dropped unsynced writes and disks that lie about flushes are not tested.
2. **Long histories.** Serializability is checked exhaustively for two short
   transactions, not by dependency-cycle search over long random histories.
3. **Network faults.** Members are killed, but networks are not partitioned or
   slowed.
4. **Deterministic simulation.** Seeds replay the fault plan, not the thread or
   process schedule.
