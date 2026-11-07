# Strategy research (2026-10-24)

## Native shadow-page candidate (2026-10-26)

The latest feedback requires native implementations independently of the
optional SQL adapter. Inspected LMDB's primary
[`mdb.c` at `74dd6c3`](https://github.com/LMDB/lmdb/blob/74dd6c3ec648c31857fe0a100f11c2a34619e2d7/libraries/liblmdb/mdb.c):
metadata occupies two alternating pages (1261–1294); commit flushes dirty pages,
synchronizes them, then publishes metadata (4214–4220). Metadata publication
selects synchronous writes unless explicitly weakened and fences failed writes
(4385–4514). The newest metadata determines the opened snapshot (4529–4535).
Source SHA-256:
`2940ef69123c2aee24c1cc716e7d6c433fec9d9ae5196a65147d6f5f7b1e8517`.

Design inference: a separately selected native doublets page store could keep
committed pages immutable, synchronize new pages before publishing a checked
root, and recover without opening a partially overwritten tree. This supplies
a different physical strategy from SQLite's WAL and the existing in-place
mapped store. It requires its own shared wire specification, torn-write and
uncertain-sync tests, ownership, bounded graph validation, language parity and
explicit space/reclamation costs. It is a candidate, not an implemented guarantee
or a claim of LMDB format compatibility. The native physical-storage criterion
remains unfinished.

This is a bounded survey of established designs, not a claim to have inspected
every successful DBMS. Each row records a primary reference, relevant source,
and an implementation implication. Similar settings do not imply equivalent
performance, failure handling, or guarantees.

| System / primary reference | Source seam inspected | Design and implication |
| --- | --- | --- |
| [SQLite atomic commit](https://www.sqlite.org/atomiccommit.html), [WAL](https://www.sqlite.org/wal.html) | [wal.c](https://github.com/sqlite/sqlite/blob/master/src/wal.c) | Journal persistence and database checkpoint ordering are separate boundaries. A checkpoint marker must follow durable data; staging avoids exposing partial transactions. |
| [PostgreSQL isolation](https://www.postgresql.org/docs/current/transaction-iso.html) | [predicate.c](https://github.com/postgres/postgres/blob/master/src/backend/storage/lmgr/predicate.c) | Snapshot isolation permits write skew. Serializable validation must account for reads and predicates, not only written keys. A whole-store revision check is conservative but safe. |
| [RocksDB transactions](https://github.com/facebook/rocksdb/wiki/Transactions) | [optimistic_transaction.cc](https://github.com/facebook/rocksdb/blob/main/utilities/transactions/optimistic_transaction.cc), [pessimistic_transaction.cc](https://github.com/facebook/rocksdb/blob/main/utilities/transactions/pessimistic_transaction.cc) | Offer pessimistic exclusion and optimistic validation explicitly. Locks, version tracking, retries, and history have different costs. |
| [Redis replication](https://redis.io/docs/latest/operate/oss_and_stack/management/replication/) | [replication.c](https://github.com/redis/redis/blob/unstable/src/replication.c) | Asynchronous primary/follower copies improve read scale. Waiting for replica acknowledgements alone does not provide consensus or safe automatic failover. |
| [etcd API guarantees](https://etcd.io/docs/v3.6/learning/api_guarantees/) | [raft.go](https://github.com/etcd-io/raft/blob/main/raft.go) | Quorum intersection, a fenced leader, ordered durable logs, and safe membership changes are necessary for consensus. A fan-out log must not advertise Raft guarantees. |
| [TiKV transactions](https://tikv.github.io/sig-transaction/doc/tikv/index.html) | [transaction](https://github.com/tikv/tikv/tree/master/src/storage/txn) | Cross-shard atomic writes require coordination and recovery beyond per-shard atomicity. Raw and transactional writes cannot be mixed safely. |
| [FoundationDB transactions](https://apple.github.io/foundationdb/developer-guide.html), [read/write path](https://apple.github.io/foundationdb/read-write-path.html) | [NativeAPI.cpp](https://github.com/apple/foundationdb/blob/dbf079f58df53df898eee906738a506cb80a21a6/fdbclient/NativeAPI.cpp), [ConflictSet.cpp](https://github.com/apple/foundationdb/blob/dbf079f58df53df898eee906738a506cb80a21a6/fdbserver/resolver/ConflictSet.cpp) | Track observed reads, including absent keys, and validate against intervening writes. Optional local point-read validation permits disjoint edits; scans remain conservative whole-revision predicates. No FoundationDB protocol or range index is copied. |
| [Apache Iceberg optimistic concurrency](https://iceberg.apache.org/spec/#optimistic-concurrency) | [format specification](https://apache.googlesource.com/iceberg/+/HEAD/format/spec.md), optimistic concurrency and filesystem operations | Immutable data plus atomic metadata publication separates data storage from the serialization point. The partitioned journal decorator stages objects in independent groups, then compares all selected catalog generations before publishing pointers. Data files and table formats are not copied. |
| [MySQL/InnoDB redo architecture](https://dev.mysql.com/doc/dev/mysql-server/latest/PAGE_INNODB_REDO_LOG.html), [doublewrite](https://dev.mysql.com/doc/refman/9.7/en/innodb-doublewrite-buffer.html) | [log0write.cc](https://github.com/mysql/mysql-server/blob/561753af986c3c006c4b6c7caa8359748fb55b22/storage/innobase/log/log0write.cc#L1058) | A requested log position, OS-written position and durable position are distinct. One flush can release several commit waiters. Doublewrite repairs torn data pages; checksums alone only detect damage. Group commit and page repair are separate optional designs, not supplied by revision compression. |
| [MongoDB write concern](https://www.mongodb.com/docs/manual/reference/write-concern/) | [replication_coordinator_impl.cpp](https://github.com/mongodb/mongo/blob/7a45cb24e938fa1a18dad3b1969444d5d80f8d09/src/mongo/db/repl/replication_coordinator_impl.cpp#L2194) | Durable acknowledgements and applied replica state differ. Write-concern timeout does not undo the primary's write. Preserve explicit uncertain outcomes and discover publication before retrying; a replica counter does not copy MongoDB's replication protocol. |
| [Cassandra Dynamo architecture](https://cassandra.apache.org/doc/latest/cassandra/architecture/dynamo.html), [hints](https://cassandra.apache.org/doc/latest/cassandra/managing/operating/hints.html) | [StorageProxy.java](https://github.com/apache/cassandra/blob/05853eaeab006bd1e834331ac8b1dfdc464d1f31/src/java/org/apache/cassandra/service/StorageProxy.java#L978) | Ordinary writes use consistency-level response handlers and timestamp reconciliation. Hints are best effort; anti-entropy remains necessary. Quorum intersection alone is not a complete transaction isolation protocol. Our fixed-primary log replication does not implement Dynamo repair or timestamp conflict resolution. |
| [Cassandra 6.0 / Accord](https://cassandra.apache.org/_/events/20261011-community-over-code-glasgow.html) | [transaction dispatch](https://github.com/apache/cassandra/blob/05853eaeab006bd1e834331ac8b1dfdc464d1f31/src/java/org/apache/cassandra/service/StorageProxy.java#L1269), [CoordinateTransaction.java](https://github.com/apache/cassandra-accord/blob/ce855de13902e7e3ebfa4356d265f0c49be1bb8e/accord-core/src/main/java/accord/coordinate/CoordinateTransaction.java#L123) | Current Cassandra includes an Accord route as well as ordinary writes. Accord chooses transaction paths using quorum responses, dependencies, timestamps and topology epochs. Our atomic catalog provides one serialization point; it does not supply Accord's leaderless protocol or its latency properties. |

## Existing reusable components

The project already depends on `doublets` / `Platform.Data.Doublets` for raw
links, allocation, uniqueness, and cascade policies. Its storage traits and
`INamedTypesLinks` are extension seams. `FileTransitionLog`, advisory locks,
TCP `RemoteLinks`, and the JS client transport contract should be reused.
The optional staged layer can compose validation, history, and replication
without imposing their allocations or I/O on a bare in-memory cache.

For production consensus, existing maintained implementations such as
[etcd/raft](https://github.com/etcd-io/raft),
[OpenRaft](https://github.com/databendlabs/openraft), and
[DotNext Raft](https://dotnet.github.io/dotNext/features/cluster/raft.html)
provide election and log-replication machinery. They are not interchangeable
with a replica acknowledgement counter. JavaScript's client transport should
advertise backend guarantees rather than inventing them client-side.

## Source observations, pinned for reproducibility

Source was downloaded through GitHub's contents API; the functions below were
read. Line numbers apply to these commits. Design observations were reused;
no competitor implementation was copied.

| Repository / commit | Observed code | Consequence for this implementation |
| --- | --- | --- |
| `sqlite/sqlite` `c6536c6e60dfa0cc6b07e4458df374aaaf1dcdce` | `src/wal.c:4285–4325`, `walFrames`, gates commit syncing on synchronization flags, pads sectors when required, and returns synchronization errors. | Flush is an explicit commit boundary. The staged log is authoritative, so no data checkpoint gets ahead of the journal. Hardware sector atomicity is not implied by a checksum. |
| `postgres/postgres` `126976a8ffcfa47f40d3df09243ebf4606001912` | `src/backend/storage/lmgr/predicate.c:4616–4697`, `PreCommit_CheckForSerializationFailure`, inspects read/write dependencies under a serializable transaction lock and aborts dangerous structures. | Write/write validation alone is insufficient. Whole-revision validation detects intervening scans and predicates, at the cost of more aborts than PostgreSQL SSI. |
| `facebook/rocksdb` `ea313605367281013c2c81158069e88411516357` | `utilities/transactions/optimistic_transaction.cc:60–104`, selects serial/parallel validation and invokes validation during a write callback. | Validation and publication share one serialization point. Validating before acquiring the commit lock would be unsafe. |
| `facebook/rocksdb` `ea313605367281013c2c81158069e88411516357` | `db/log_writer.cc:87–187,308–359`, optional compression and checksummed physical records, followed by append/flush and sequence advancement only on success. | Keep logical revisions separate from optional physical compression. The portable delta decorator reduces localized journal bytes while preserving flush boundaries and uncertain outcomes; its wire format is independent. See [measurements and limits](DELTA-PLAN.md). |
| `etcd-io/raft` `c51f010c851edd0dada5af41bafbf68e54617767` | `raft.go:772–789`, `maybeCommit`, advances a leader's log using its term and the tracker quorum's committed index. | Fixed-primary replication lacks leader terms, membership and fencing. It cannot claim equivalent consensus or automatic failover. |
| `redis/redis` `c039dd784ca6139314880631de162f65dda25d45` | `src/replication.c:4877–4952`, distinguishes processed offsets from AOF-fsynced offsets and waits for acknowledgements of the client's write offset. | An adapter must distinguish copies from durable copies. Replication errors require outcome discovery because the primary may have committed. |
| `tikv/tikv` `47037121d3e60d81ed77c04de3a14b5205c1d2b6` | `src/storage/txn/commands/prewrite.rs:40–80`, declares primary/secondary locks, timestamps, TTLs and separate 1PC/2PC paths. | Independent shard commits cannot be presented as an atomic multi-shard transaction. Coordination and lock recovery require additional protocols. |
| `apple/foundationdb` `dbf079f58df53df898eee906738a506cb80a21a6` | `fdbclient/NativeAPI.cpp:3548–3568,3908–3931`, `Transaction::get` records a single-key read conflict for nonsnapshot reads; `addReadConflictRange` validates bounds and retains the range. `4755–4820` merges conflict footprints while preparing commit. | Record point dependencies even when an address is missing. Own writes and read sets validate under the same commit mutex. An unselected engine keeps no per-read set. |
| `apple/foundationdb` `dbf079f58df53df898eee906738a506cb80a21a6` | `fdbserver/resolver/ConflictSet.cpp:799–847`, `ConflictBatch::addTransaction` gathers read/write ranges and a read snapshot, rejecting snapshots too old for validation. | Keep generations until every potentially stale local session finishes, including deletes/recreates. Local full-revision scan invalidation replaces range tracking; this does not implement FoundationDB's distributed resolver or its performance. |
| `facebook/rocksdb` `ea313605367281013c2c81158069e88411516357` | `utilities/transactions/pessimistic_transaction.cc:60–90,1039–1255`, initializes lock/timeouts, releases new savepoint locks, sorts batch keys and acquires before snapshot validation in `TryLock`, releasing a new lock after failed validation. [Pinned source](https://github.com/facebook/rocksdb/blob/ea313605367281013c2c81158069e88411516357/utilities/transactions/pessimistic_transaction.cc#L1039). | Optional local point-write ownership acquires/checks generations under the publication mutex and releases at session end. Immediate busy errors avoid waits/deadlock machinery. Serializable read validation remains independent. The cluster rejects this strategy without a distributed lock manager; no RocksDB code or LSM protocol is copied. |
| `mysql/mysql-server` `561753af986c3c006c4b6c7caa8359748fb55b22` (2026-10-20) | `storage/innobase/log/log0write.cc:1058–1165,2428–2527`: `log_write_up_to` distinguishes waiting for written versus flushed LSN. `log_flush_low` captures the written LSN, fsyncs, then publishes the durable LSN and notifies relevant waiters. | The opt-in group coordinator now prepares independent sessions in order, appends a separate revision per accepted writer and publishes only after one shared flush. Each member receives a known abort, acknowledged revision or uncertain outcome; recovery discovers the persisted prefix. This uses the existing serialized engine gate rather than InnoDB's concurrent LSN wait queues. Page WAL and doublewrite repair remain separate work. |
| `mongodb/mongo` `7a45cb24e938fa1a18dad3b1969444d5d80f8d09` (2026-10-24) | `src/mongo/db/repl/replication_coordinator_impl.cpp:2194–2338`: `_doneWaitingForReplication` selects applied/durable opTime and numeric/tagged concern; majority also considers the committed snapshot as configured. `awaitReplication` bounds its wait and removes an abandoned waiter. | A timeout describes acknowledgement, not a reversed write. Our uncertain-outcome discovery tests exercise that boundary; they do not establish MongoDB protocol equivalence or causal replica reads. |
| `apache/cassandra` `05853eaeab006bd1e834331ac8b1dfdc464d1f31` (2026-10-23) | `src/java/org/apache/cassandra/service/StorageProxy.java:978–1040,1205–1315`: ordinary mutations send to replicas and wait for handlers; ANY can use hints. Current dispatch splits Accord and ordinary mutations, preserves timestamp choices and retries protocol/topology changes. | Describe the chosen write path. A global claim that current Cassandra supports only eventual consistency would be incorrect. We do not implement its hints, anti-entropy, last-write-wins rules or Accord migration. |
| `apache/cassandra-accord` `ce855de13902e7e3ebfa4356d265f0c49be1bb8e` (2026-10-10) | `accord-core/src/main/java/accord/coordinate/CoordinateTransaction.java:98–204`: computes routes/topologies, merges preaccept timestamps and dependencies, then selects fast execution, medium proposal or slow proposal. Dependencies from older topology epochs may prevent the fast path. | Inference: selected-shard catalog CAS offers atomic publication through a shared serialization point. Claiming Accord's leaderless multi-shard algorithm, failure recovery or WAN round-trip costs requires a separate implementation and verification. |

## Additional design gaps exposed by this audit

Caller-selected independent sessions now share a durability flush while retaining
separate revisions; see [ENGINE-GROUP-COMMIT.md](../../ENGINE-GROUP-COMMIT.md).
This is an engine coordinator with input-order validation and persisted-prefix
discovery after unknown I/O. It adds no automatic scheduling or InnoDB page WAL.
Page WAL/doublewrite repair, Dynamo anti-entropy and Accord remain unfinished
within the issue's broad strategy request. The existing checksummed journals,
explicit flushes, outcome discovery and atomic catalog each supply narrower guarantees.
They are reusable seams for further work, not evidence that the other algorithms
or their performance have been reproduced. MySQL's durability also depends on
[storage honoring flushes](https://dev.mysql.com/doc/refman/8.4/en/innodb-init-startup-configuration.html);
a killed-process recovery test cannot establish real power-loss behavior.

## Ordinary-store undo barrier audit (2026-10-25)

Re-read SQLite's [atomic commit ordering](https://www.sqlite.org/atomiccommit.html)
and [rollback/hot journals](https://www.sqlite.org/lockingv3.html), then inspected
`src/pager.c` at the same pinned `c6536c6e60dfa0cc6b07e4458df374aaaf1dcdce`
commit used above. [`pager_write`, lines 6110–6189](https://github.com/sqlite/sqlite/blob/c6536c6e60dfa0cc6b07e4458df374aaaf1dcdce/src/pager.c#L6110)
journals original page contents before setting the writable flag; the sector
variant journals co-resident pages. [`syncJournal`, lines 4340–4445](https://github.com/sqlite/sqlite/blob/c6536c6e60dfa0cc6b07e4458df374aaaf1dcdce/src/pager.c#L4340)
propagates synchronization errors; commit phase one syncs the journal before
writing dirty pages (line 6674). The downloaded `pager.c` has SHA-256
`96d7bb9c4a5a6da0909ce0ab4714311e1ffa5f13007dc4c69f028a2760104ba4`.
A callback after mutation cannot substitute for this pre-effect boundary.

Our adapter has logical row enumeration/replay, without pager hooks controlling
all underlying page writes. The selected logical undo image uses those existing
seams and verifies complete restoration. It adds full-store capture and per-write
reconciliation costs; it does not implement SQLite's page/sector protocol.
Physical torn-page recovery and efficient page undo remain separate open work.
See [configuration, assumptions and costs](../../TRANSACTION-UNDO.md) and
[the row-barrier plan](ROW-WAL-PLAN.md). No SQLite implementation was copied.

## Immutable ordered state components (2026-10-25)

The existing flat maps copy every row into a transaction view or committed
candidate. A bounded baseline with 256 named rows and sixteen one-row commits
shares zero of 4,080 unchanged Rust row references. Evaluate reusable components
before introducing a second balancing-tree implementation:

| Component / pinned primary source | Observed implementation | Selection and limits |
| --- | --- | --- |
| [`rpds` 1.2.1](https://github.com/orium/rpds/blob/50a486ec8e4fade33a286af4bc35c2728b1f268f/Cargo.toml), MIT | [`red_black_tree_map/mod.rs`](https://github.com/orium/rpds/blob/50a486ec8e4fade33a286af4bc35c2728b1f268f/src/map/red_black_tree_map/mod.rs): node cloning at 130–142 shares entries/children; mutation at 839–887 copies affected paths; map cloning at 1001–1008 retains the root. | Select `RedBlackTreeMapSync`, whose Arc-based ownership preserves native Send/Sync. The default Rc map would change those capabilities. Version 1.2.1 declares Rust 1.85, below this project's 1.89 floor. Actual native/WASM compilation verifies supported targets; this is not a complete minimum-compiler test. |
| [.NET 10 immutable sorted dictionary](https://github.com/dotnet/runtime/blob/v10.0.0/src/libraries/System.Collections.Immutable/src/System/Collections/Immutable/ImmutableSortedDictionary_2.Node.cs), MIT | Frozen AVL nodes are sealed recursively at 429–439; `Mutate` and balancing at 745–777 copy frozen paths while preserving other children. | Reuse the existing target's `System.Collections.Immutable`; no new NuGet package. The public `IReadOnlyDictionary` interface can expose the immutable map directly. |
| [`im-rs`](https://github.com/bodil/im-rs), MPL-2.0 | Repository metadata reports archived, with last source push 2024-08-19. | Prefer the maintained MIT component. No code from this alternative is copied. |

Downloaded source SHA-256 values are `71189099680f42428692abb21eb19c6afe96da0e4695634f2866fb021a009e69`
for the rpds tree, `0a977678da90c4b602dc1ae4a78371a3841c04b3372d15287fb79c496939ff70`
for its manifest, and `ac0e39edb1414c7ddfd6f8bce7cd808f27f9f228a45c781558ae87069bbb5611`
for the .NET node source. Component versions and source commits are distinct:
the commit above identifies the published rpds 1.2.1 source, rather than an
unpinned development branch.

Locked native and WASM dependency graphs both resolve `rpds` 1.2.1 and `archery`
1.2.3 (MIT, declared Rust 1.85), `triomphe` 0.1.17 (MIT/Apache-2.0, declared
Rust 1.85), and `smallvec` 1.16.4 (MIT/Apache-2.0, no declared compiler floor).
Both `cargo metadata --locked` checks pass. Those declarations and successful
current-target builds do not substitute for running the entire project under
its minimum compiler.

The selected implementation preserves the existing complete `engine1` wire.
It changes candidate and retained-snapshot allocation costs; it does not make
the full revision encoder, constraint scans or delta comparison incremental.
Rust's existing `rows() -> &BTreeMap` requires a lazy O(N) compatibility copy;
borrowed `get`/`iter` and internal operations avoid it. Finite entry/node identity
controls and independent state oracles verify this distinction. See
[the shared-state recipe](../../ENGINE-SHARED-STATE.md) and
[implementation plan](SHARED-STATE-PLAN.md). Page WAL, LSM layout and physical
page repair remain separate open designs.


## Logical write-batch serialization (2026-10-25)

Shared roots remove complete candidate copies, but engine1 encoding still visits
every row/name. A finite actual-WASM control with 256 named rows and sixteen
one-row commits encodes 4,096 update rows / 966,063 bytes under either map.
The physical byte-delta decorator cannot remove serialization already performed.

RocksDB's [basic operations](https://github.com/facebook/rocksdb/wiki/Basic-Operations)
exposes atomic write batches. Its pinned
[db_impl_write.cc](https://github.com/facebook/rocksdb/blob/ea313605367281013c2c81158069e88411516357/db/db_impl/db_impl_write.cc)
collects accepted batches in `MergeBatch` at lines 1350–1435, serializes their
contents/checksum before `AddRecord`, and assigns write-group sequences before
optional synchronization at lines 2290–2435. Downloaded source SHA-256 is
`a839471e7904eeeed224a45b519d8369fcd96a431df047fbaa4e8ba245db9442`.

Inference for our publish seam: encode the selected changed addresses against
the current immutable root, then retain the existing append/flush/publication
boundary. Reuse our journal checksum and selected persistent map components; no
RocksDB code or physical format is copied. Default complete snapshots remain
available; selected engine2 records bind changes to the exact preceding revision
and preserve allocator progression, group outcomes and branch attribution.

SQLite's [WAL](https://www.sqlite.org/wal.html) and
[file format](https://www.sqlite.org/fileformat.html) describe page frames and
checkpoint ordering, which our logical redo does not supply. Changes alone add
no page recovery, torn-write repair, LSM, compaction or power-loss verification.
After implementation, the same finite control encodes sixteen update rows /
4,246 bytes under both map choices, preserving pins and exact reopening. This
measures logical bytes, without timing or physical durability claims. See
[canonical protocol](../../protocol/engine-changes.md),
[configuration/costs](../../ENGINE-CHANGE-RECORDS.md) and
[execution plan](CHANGE-RECORDS-PLAN.md).

## Reusing a physical SQLite journal component

The separate [SQLite adapter](../../ENGINE-SQLITE.md) reuses the maintained pager,
WAL and checkpoint code at the ordered-log seam. Rust selects rusqlite 0.41.0
(audited tag `a60e76b5e86ee60442213efe79758a6c2ca8cf3d`) with bundled
libsqlite3-sys 0.39.1 / SQLite 3.54.1. C# accepts an owned `DbConnection` without
adding a core provider dependency. JS accepts a synchronous connection from
Node, Bun or Deno through a separate entry point; native SQLite is excluded
from the WASM build. This is component reuse, without copied SQLite sources.

Final C# execution selects Microsoft.Data.Sqlite 10.0.14; the persistent example
reports SQLite 3.53.3. The repository's latest-dependency policy also requires
System.CommandLine 2.0.14. Both C# configurations pass with those versions; the
core library remains independent of a SQLite provider package.

The reviewed 3.54.1 amalgamation has SHA-256
`0c657666d819f261cc4007526ad57e1669de9949df7a8ea847aa284a2bf0ee52`.
`walFrames` / `sqlite3WalFrames` at lines 70996–71247 checksums frames and handles FULL commit
sync before publishing the WAL-index header. The checkpoint restart path at
69185–69248 checks WAL salts under the reader lock. Primary contracts are
[WAL](https://www.sqlite.org/wal.html),
[synchronous](https://www.sqlite.org/pragma.html#pragma_synchronous) and
[checkpoint](https://www.sqlite.org/pragma.html#pragma_wal_checkpoint).
Require runtime >= 3.51.3 for the documented WAL-reset race fix. Current CI's
Node 24.22.1 [bundled header](https://github.com/nodejs/node/blob/v24.22.1/deps/sqlite/sqlite3.h#L144)
specifies 3.53.4; the adapter checks the actual runtime rather than assuming a
Node version always supplies a particular SQLite version.

Inference for this seam: put a pending record group inside BEGIN IMMEDIATE /
COMMIT, retain exact UTF-8 BLOB bytes across providers, and check the expected
tail while holding write ownership. SQLite locking does not invalidate another
engine's cached immutable state; stale owners must reopen. A COMMIT error can
follow success, so the adapter fences and preserves discovery rather than
retrying blindly. Provider-specific finite retries remain explicit, especially
Microsoft.Data.Sqlite's independent command timeout. Database/checkpoint work
now has a selected physical implementation; legacy mapped page repair, LSM,
doublewrite and physical power-loss testing remain distinct open criteria.
See [protocol/vectors](../../protocol/sqlite-log.md) and
[execution plan](SQLITE-JOURNAL-PLAN.md) for tests and exact provider scope.


## Incremental constraint validation (2026-10-25)

PostgreSQL's [constraint documentation](https://www.postgresql.org/docs/current/ddl-constraints.html)
explains unique indexes and the separate cost of finding referencing rows during
referenced-row changes. At pinned commit `126976a8ffcfa47f40d3df09243ebf4606001912`,
[`ri_triggers.c:430–540`](https://github.com/postgres/postgres/blob/126976a8ffcfa47f40d3df09243ebf4606001912/src/backend/utils/adt/ri_triggers.c#L430)
builds an endpoint existence query with key-share locking; its
[`ri_restrict:795–915`](https://github.com/postgres/postgres/blob/126976a8ffcfa47f40d3df09243ebf4606001912/src/backend/utils/adt/ri_triggers.c#L795)
queries referencing rows for an old key. Downloaded source SHA-256 is
`a47eef6d2e94ae3921feb6539f5663147c655a2856eab71b65c74c90b37f297c`.

Inference for this engine: existence checks on changed rows alone cannot detect
unchanged incoming references to a deleted row. A selected address/count index
can detect them under the existing engine gate. Pair/name owner indexes remove
complete uniqueness scans. Reuse the already audited rpds/.NET persistent trees,
remove all changed old keys before inserting new keys, and publish private roots
with committed rows. This implements local final-transaction constraints, without
copying PostgreSQL code, SQL trigger semantics, key-share locks or distributed
foreign keys. Recovery retains complete historical validation. See
[configuration and measured costs](../../ENGINE-CONSTRAINT-INDEX.md).

## Immutable object reuse and bounded caching (2026-10-25)

Rechecked the [Iceberg specification](https://iceberg.apache.org/spec/),
optimistic concurrency, manifests and snapshots: immutable files can be reused
across atomically published metadata versions. Inference for this protocol:
already verified objects can be retained while catalog snapshots remain fresh.
This implements local object reuse, independently of Iceberg's file formats.
It does not supply manifests, incremental transfer or snapshot-aware reclamation.

Audited the downloaded `lru` 0.19.0 source at
[`3ed2e28cfc53059c5a29d6c4f73f5ddf26a29e98`](https://github.com/jeromefroe/lru-rs/blob/3ed2e28cfc53059c5a29d6c4f73f5ddf26a29e98/src/lib.rs):
`unbounded` (258–260), sentinel construction (300–319), `put` and replacement
(342–440), promotion in `get` (459–475), removal (1293–1318), and `pop_lru`
(1377–1383). The maintained component owns hash/list ordering, including its
internal unsafe pointer management; this engine adds weighted admission and
eviction through safe APIs. Cargo archive checksum is
`4a9f71e3591c7ec44d3e563e87bf14ddc2bada8519dca046e5b0e560cd004e2d`.
Read its MIT license and Cargo manifest: MSRV 1.85; disabling default features
uses `std::collections::HashMap` and adds no hashbrown dependency. The repository
requires Rust 1.89 or newer. C# and JS reuse their standard dictionary/list/Map
components instead of adding another platform package. No upstream code was
copied. Payload limits do not claim to measure container or temporary memory.
See [contracts, selected costs and limits](../../ENGINE-OBJECT-CACHE.md).

## Checked Unicode graph traversal (2026-10-25)

Audited the installed `Platform.Data.Doublets.Sequences` 0.6.5 source at
`ed32cddd072df0dace57f02dfb537b0eb8e2bfa5`, including its Unlicense,
[`SequenceWalkerBase`](https://github.com/linksplatform/Data.Doublets.Sequences/blob/ed32cddd072df0dace57f02dfb537b0eb8e2bfa5/csharp/Platform.Data.Doublets.Sequences/Walkers/SequenceWalkerBase.cs),
[`RightSequenceWalker`](https://github.com/linksplatform/Data.Doublets.Sequences/blob/ed32cddd072df0dace57f02dfb537b0eb8e2bfa5/csharp/Platform.Data.Doublets.Sequences/Walkers/RightSequenceWalker.cs),
[`ISequenceWalker`](https://github.com/linksplatform/Data.Doublets.Sequences/blob/ed32cddd072df0dace57f02dfb537b0eb8e2bfa5/csharp/Platform.Data.Doublets.Sequences/Walkers/ISequenceWalker.cs)
and the
[`UnicodeSequenceToStringConverter`](https://github.com/linksplatform/Data.Doublets.Sequences/blob/ed32cddd072df0dace57f02dfb537b0eb8e2bfa5/csharp/Platform.Data.Doublets.Sequences/Unicode/UnicodeSequenceToStringConverter.cs).
The walker follows source/target links without active-path validation. The
converter already accepts a walker interface, so the maintained converter can
be reused without a fork or package change. Rust's local walker has the same
cycle omission and additionally skips absent nodes.

Bounded pre-fix executions confirm nonterminating self-cycles in both native
languages and successful but truncated Rust output for a missing child.
Inference for this representation: maintain a per-traversal active-path set,
remove each node on exit, and reject a second entry while it remains active.
A global visited set would incorrectly discard repeated valid subtrees. Use
standard explicit stacks and hash sets, preserving symbol order and avoiding
depth-dependent recursion. This adds O(depth) metadata; shared DAG expansion
can still produce large output. Existing memoization, UTF-16 policies and stored
bytes remain unchanged. See [contract and limits](../../UNICODE-SEQUENCE-INTEGRITY.md)
and [execution plan](UNICODE-GRAPH-PLAN.md). No upstream implementation is copied.

## Source-spine output and reference-order compatibility (2026-10-25)

Audited the published
[generic Rust formatter](https://github.com/link-foundation/link-cli/blob/1a482ac1a4d51b122a4864a3361d44e7258b3a0c/rust/src/named_type_links.rs),
[concrete Rust formatter](https://github.com/link-foundation/link-cli/blob/1a482ac1a4d51b122a4864a3361d44e7258b3a0c/rust/src/link_storage.rs)
and [C# formatter](https://github.com/link-foundation/link-cli/blob/1a482ac1a4d51b122a4864a3361d44e7258b3a0c/csharp/Foundation.Data.Doublets.Cli.Library/LinoDatabaseOutput.cs).
They expand only the source spine, resolving the terminal source and then
ancestor target/index names while unwinding. Both recursion depth and repeated
whole-subtree string assembly grow with valid input depth. Bounded native
executions confirm stack overflow in all three entry points.

Inference from those observable call orders: retain explicit link frames,
resolve references deepest first, then emit prefixes root first and suffixes
leaf first. Reuse standard Vec/List/String/StringBuilder components and one
shared Rust implementation. An independent shallow oracle verifies read order,
name failures, caller paths, cycles and output. This preserves selected store
lookup behavior; linear output assembly does not make an unindexed store's
lookups constant time. See [contract and costs](../../STRUCTURE-OUTPUT.md) and
[execution plan](STRUCTURE-DEPTH-PLAN.md). No external implementation is copied.


## Immutable sorted runs (2026-10-26)

RocksDB's [architecture](https://github.com/facebook/rocksdb/wiki/RocksDB-Overview)
separates in-memory representations, WAL and on-disk sorted tables. Its
[universal compaction](https://github.com/facebook/rocksdb/wiki/Universal-Compaction)
merges adjacent time ranges with explicit write/read/space tradeoffs. Inspected
[`compaction_picker_universal.cc` at `014e1c4`](https://github.com/facebook/rocksdb/blob/014e1c43c12947c2efe15275b7dd371bce13e6f1/db/compaction/compaction_picker_universal.cc),
lines 150–260 and 1040–1190: run-count, size-amplification and size-selection
conditions are separate. The full downloaded source is retained locally.

Design inference: the existing row seam can independently select immutable sorted
arrays while keeping its journal and session contracts. A small binary carry
policy over effective edit counts needs no new library or SQL dependency.
Newest values win equal keys; tombstones hide older runs until complete
compaction. This policy is deliberately simpler than RocksDB's physical storage
and picker. Native Rust and C# implementations share immutable arrays with pinned
revisions; JS/TS selects the Rust implementation through WASM. Scans materialize
an ordered map, and carry merges can rewrite complete runs. No throughput or
constant-write-cost improvement is implied. See [costs](../../ENGINE-SORTED-RUNS.md)
and [the verification plan](SORTED-RUNS-PLAN.md). Physical SSTables, background
compaction and legacy mapped-file repair remain unfinished.

## Fenced immutable-object reclamation (2026-10-26)

Rechecked Iceberg's [snapshot retention and commit requirements](https://iceberg.apache.org/spec/#snapshot-retention-policy):
retention considers branch/tag references and retained ancestors; files may be
removed only when remaining live snapshots no longer need them. Reintroducing
deleted files can invalidate incremental cleanup. Metadata commits must use the
current version atomically. This motivates inventorying every durable root,
including retained history, before considering physical deletion.

Rechecked the [etcd v3.6 API](https://etcd.io/docs/v3.6/learning/api/):
`version` resets after deletion, whereas `mod_revision` identifies a modification.
Transactions evaluate comparisons and apply their selected operations atomically.
The maintenance adapter consequently uses modification revisions, including for
conditional deletion; restoring a deployment must preserve their non-reuse or
migrate into new namespaces.

Design inference for this journal representation: root discovery alone cannot
exclude a publisher that staged an object before discovery but publishes after
it. A separately selected catalog epoch fences those publishers. Epoch-specific
object names prevent later writers from resurrecting a candidate; exact object
generations protect replacements. Owned readers retry the whole captured group
once if retirement races fetching it. Current journals retain revision/branch
history, and no borrowed external physical root is exposed. These are local
protocol choices, rather than Iceberg compatibility or a claim that its cleanup
algorithm has been implemented. Independent bounded models include unsafe
negative controls, and public-API fault schedules test the implementation.
See [the protocol, proof assumptions and costs](../../ENGINE-RECLAMATION.md).

## Native physical sorted tables (2026-10-26)

Rechecked RocksDB's [architecture](https://github.com/facebook/rocksdb/wiki/RocksDB-Overview),
[WAL](https://github.com/facebook/rocksdb/wiki/Write-Ahead-Log-%28WAL%29) and
[MANIFEST](https://github.com/facebook/rocksdb/wiki/MANIFEST) primary documentation.
The design separates mutable memory, sorted immutable files and recovery logs;
a manifest records the selected physical state. Durable data files alone do not
identify the selected version after a failed multi-file update.

Design inference: our existing SQL-free immutable doublets pages can hold sealed
sorted journal records with retained key/address and page-hash indexes. Physical
point reads need not materialize every payload. This is a storage building block;
WAL/memtable/manifest publication and compaction still require independent fault
and restart verification. It neither copies RocksDB's code/format nor supplies
its full feature set. See [execution plan](PHYSICAL-SORTED-TABLE-PLAN.md).

## Bounded immutable journal chunks (2026-10-27)

Re-read the [Iceberg specification at e5f8ab19](https://github.com/apache/iceberg/blob/e5f8ab19c82fcf24be430cc6c0cecf9a23f5afe4/format/spec.md),
lines 90, 94–100, 130–135 and 1539: immutable files/manifests can be reused by
later snapshots, while checked metadata publication determines the selected
state. The locally retained source has SHA-256
`b8c9607bad6ac2bd42289a41d44befc56db79e9faa534f59bd120b0d7c0c1b89`.
RocksDB's [BlobDB design](https://github.com/facebook/rocksdb/wiki/BlobDB)
separates values from sorted keys and documents the resulting write, space and
garbage collection tradeoffs. Neither design removes the need to inventory
references before deletion.

Design inference for the existing journal seam: optionally split each update
at deterministic record/UTF-8 byte boundaries and reuse complete immutable
prefix chunks. Preserve the single checked catalog decision and fresh complete
staging comparison, including cached references. This reduces new payload
writes for unchanged complete prefixes; staging reads, full replay and catalog
growth remain explicit costs. Record counts, bytes and manifest length need
independent finite bounds. The existing epoch collector must reject the new
format until it can follow its complete reference graph safely. No source or
format is copied. See [protocol](../../protocol/chunked-journal.md) and
[execution plan](SEGMENTED-JOURNAL-PLAN.md).

## Owned mapped-view barriers (2026-10-27)

Microsoft's [FlushViewOfFile contract](https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-flushviewoffile)
requires flushing the view before flushing the file buffers for durable mapped
writes. Its [MapViewOfFile contract](https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-mapviewoffile)
distinguishes local view coherence from remote files and ordinary file I/O.
Downloads retained for this audit have SHA-256
`98facb3e5778cb5b8200604a8accbb763d51979bd734208361228fe8b78d1f63`
and `2388f63134e9640273823c4a9a29b568db382bb38f994d8881924b3ba941d15c`.

The published [platform-mem 0.3.0 source](https://docs.rs/crate/platform-mem/0.3.0/source/src/file_mapped.rs)
has SHA-256 `6c27511dd32308d6e061bbfde6d3b3bb399395a9c2b245a99e9503d311815337`.
Its mapping field is private; Drop synchronizes only the file handle. Our Rust
adapter also synchronized only a separately opened file. C# already owns an
ordered view/file barrier. A file path alone cannot recover a foreign view's
address or lifetime.

Design inference: retain an explicit flush owner alongside the existing public
mapped wrapper. Share its current view descriptor under a mutex; invalidate
before unmap, publish only after successful remap and close before owner Drop.
Carry that owner through storage decorators, and require explicit attachment
when adopting generic stores. Propagate each barrier error without publishing
the path's revision. This preserves the wrapped raw mapping API and avoids
changing ordinary link edits. The cost and adoption recipe are in
[ENGINE-STRATEGIES.md](../../ENGINE-STRATEGIES.md).

Finite API, lifetime and independent syscall-order controls verify this narrower
contract. Readback and successful process exits do not demonstrate hardware
power-loss recovery. The upstream failed-resize lifecycle and legacy torn-page
repair remain separate open criteria; see [the execution plan](MAPPED-VIEW-FLUSH-PLAN.md).

## Retaining mapped allocation ownership (2026-10-27)

The published [platform-mem 0.3.0 FileMapped implementation](https://docs.rs/crate/platform-mem/0.3.0/source/src/file_mapped.rs)
releases its old mapping before file metadata, extension or replacement mapping
can fail. Its logical buffer still retains the old address after a returned
error. A finite Linux quota child reproduces EFBIG followed by SIGSEGV through
safe allocation access; its within-quota control succeeds. A checked barrier
alone correctly rejects an invalid descriptor but cannot keep that pointer alive.

Prepare a separate owner while retaining the live view for ordinary types
without destructor glue. Publish it only after complete initialization. Capture
real initialized file bytes before minimum-page padding or any failed extension;
physical file size after an error is not proof that its new items were initialized.
No upstream private layout or local-only dependency patch is used. Generic
values with destructors keep their existing single-owner path; general failure
safety there and rejected shrink remain unfinished.

Microsoft's [CreateFileMapping contract](https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-createfilemappingw)
permits increasing the file through a new mapping object. Its
[SetEndOfFile contract](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setendoffile)
requires unmapping views and closing mapping objects before changing length.
The Windows preparation therefore uses a temporary mapping object to extend
while the old local view remains alive, then creates the replacement owner.
A failed preparation can leave extra physical bytes; logical capacity and the
known initialization bound remain unchanged. Overlapping views, a temporary
file handle and, on Windows, a temporary mapping handle are explicit transient
costs. None of these controls certify hardware power loss or page atomicity.

## Empty and rejected shrink boundaries (2026-10-27)

The same pinned platform-mem 0.3.0 source does not special-case zero removal:
it still retires the mapping and truncates to its logical capacity. Before
adoption that capacity can be zero even though the file contains initialized
items. Four failing finite regressions establish actual data loss and premature
flush invalidation. Returning before zero removal and checking excessive
removal before invalidation preserves the owner without allocation or I/O.
Complete shrink remains separately controlled: memmap2 0.9.12 already supports
empty maps on both platform paths. A purported zero-length Windows syscall
failure must not be inferred without tracing that inner layer. Actual failed
partial truncation and destructor-bearing growth remain distinct open hazards.

## Causal reconciliation and bounded anti-entropy source seams

[ANTI-ENTROPY-RESEARCH.md](ANTI-ENTROPY-RESEARCH.md) records the original
Dynamo causal-version/Merkle design, current Cassandra repair/tombstone
documentation and pinned Cassandra MerkleTree, RepairJob and LocalSyncTask
source. An independent finite model checks retained siblings, observed
resolution, deletion resurrection and merge laws. Canonical causal-version1
vectors specify an initial semantic boundary independently of runtime code.
Selected durable local ownership and bounded visible-content Merkle transfer
now have production Rust/C#/JS ports and shared actual-WASM contracts under
[ANTI-ENTROPY-PLAN.md](ANTI-ENTROPY-PLAN.md). Complete current-source validation
passes at 2026-10-27T15:59:59Z; fresh platform CI remains active. Ordinary-links projection, membership-aware
history reclamation and a complete repair protocol remain unfinished; causal
convergence does not replace globally linearizable coordinated publication.

## Refreshed mapped-owner dependency audit (2026-10-27)

The generic mapped-owner audit was refreshed on 2026-10-27 while those suites
ran. The [crates.io index](https://index.crates.io/pl/at/platform-mem) still lists
0.3.0 as the newest published version. Upstream main remains
`c373135da4dec66c946b504bb455af4aec732816`; its
[FileMapped source](https://github.com/linksplatform/mem-rs/blob/c373135da4dec66c946b504bb455af4aec732816/src/file_mapped.rs)
has the same SHA256 recorded above, and the open-PR list is empty. It still
unmaps before fallible remapping while retaining the private typed buffer.
There is no newer ownership API in that source to adopt. This read-only audit
does not fix genuine generic resize/partial-shrink or destructor failures;
they remain open implementation criteria. The downloaded index, source and
paginated commit/PR evidence are retained under experiments/issue-138.
