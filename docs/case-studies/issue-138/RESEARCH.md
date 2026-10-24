# Strategy research (2026-10-24)

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
| [Apache Iceberg optimistic concurrency](https://iceberg.apache.org/spec/#optimistic-concurrency) | [format specification](https://apache.googlesource.com/iceberg/+/HEAD/format/spec.md), optimistic concurrency and filesystem operations | Immutable data plus atomic metadata publication separates data storage from the serialization point. The partitioned journal decorator stages objects in independent groups, then compares all selected catalog generations before publishing pointers. Data files and table formats are not copied. |

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
| `etcd-io/raft` `c51f010c851edd0dada5af41bafbf68e54617767` | `raft.go:772–789`, `maybeCommit`, advances a leader's log using its term and the tracker quorum's committed index. | Fixed-primary replication lacks leader terms, membership and fencing. It cannot claim equivalent consensus or automatic failover. |
| `redis/redis` `c039dd784ca6139314880631de162f65dda25d45` | `src/replication.c:4877–4952`, distinguishes processed offsets from AOF-fsynced offsets and waits for acknowledgements of the client's write offset. | An adapter must distinguish copies from durable copies. Replication errors require outcome discovery because the primary may have committed. |
| `tikv/tikv` `47037121d3e60d81ed77c04de3a14b5205c1d2b6` | `src/storage/txn/commands/prewrite.rs:40–80`, declares primary/secondary locks, timestamps, TTLs and separate 1PC/2PC paths. | Independent shard commits cannot be presented as an atomic multi-shard transaction. Coordination and lock recovery require additional protocols. |
