# Optional staged engine

The existing `LinkStorage` / `NamedTypesDecorator` path remains the cache and
compatibility path. It does not gain isolation merely by acquiring a file lock.
The optional Rust `engine::Engine`, C# `Engine.LinksEngine<T>`, and JS/TS
`WasmEngine` keep a session's writes private until commit. Sessions implement
the existing raw links and names interfaces. Choose the policies at construction.
JS runs the Rust implementation through WASM; it does not duplicate the
transaction algorithm.

## Choose a configuration

| Scenario | Construction | Acknowledgement and cost |
| --- | --- | --- |
| Disposable cache | Existing bare in-memory links store | No journal, retained snapshots, transaction validation, replica work or disk flush is added. |
| Isolated volatile state | Staged engine + `MemoryLog` / `MemoryCommitLog` | Atomic publication within one engine, rollback, configurable isolation. Process exit loses it. |
| Early point-write conflict detection | Select pessimistic admission | Reserve individual written addresses until session end; competing writes fail immediately. [Configuration and costs](ENGINE-PESSIMISTIC.md). |
| Durable single writer | Staged engine + exclusive file log, require durable flush | One authoritative, checksummed complete revision is appended and flushed before publication. Storage must honor flush; one writer owns the log. |
| Bounded time travel | Set Rust `HistoryPolicy::Last(n)`, C# `HistoryLimit = n`, JS `history: n` | Retain up to n revisions including current. Active sessions and previously returned snapshots pin their versions independently. |
| Complete time travel | Rust `HistoryPolicy::All`, C# `HistoryLimit = null`, JS `history: 'all'` | Every replayed/committed revision remains addressable in memory. |
| Independent branch | Rust `Engine::fork`, C# `LinksEngine.Fork`, JS `WasmEngine.fork` | Seed and flush a separate journal; edit privately and merge committed changes atomically into the parent. |
| Asynchronous copies | Wrap a log with replicated log, async policy | Commit waits for primary only. Call synchronize under the engine's log mutex to catch up followers. A primary loss can lose uncopied commits. |
| Synchronous copies | Replicated log, acknowledgements k including primary | Commit waits for primary and enough successful follower flushes. Failure can leave a committed primary or partial set of copies; outcome discovery is mandatory. |
| Serializable independent point edits | Select `SerializableValidation::ReadSet` / `SerializableValidation.ReadSet` / `serializableValidation: 'read-set'` | Track observed addresses and validate concurrent changes at commit; scans still check the whole revision. [Recipe and costs](ENGINE-VALIDATION.md). |
| Localized journal edits | Optional `DeltaLog` / `DeltaCommitLog` around the selected log | Store a checksummed changed span between periodic full checkpoints; engine cloning and replay remain O(N). See [format, composition and examples](ENGINE-DELTA.md). |

`supports_durable_flush` / `SupportsDurableFlush` / `supportsDurableFlush` is
an adapter contract. Memory logs declare false. An unknown adapter defaults to
false in Rust; explicit durability requirements reject it. A durable external
adapter must actually flush its storage before returning. Replicated durability
requires durable primary and, conservatively, every eligible follower when
more than one acknowledgement is requested. A copy count alone says nothing
about independent failure domains, fencing, power-loss protection, or elections.

## Concurrency and isolation

Optimistic admission permits multiple sessions. Validation and publication use
one engine mutex, so two commits cannot validate the same state concurrently.
Exclusive admission fails immediately when another session is active; rollback,
drop/dispose, and completed attempts release admission. A session belongs to
one caller; share engine handles between threads instead of sharing a session.

Optional [pessimistic point writes](ENGINE-PESSIMISTIC.md) reserve written addresses
and reject competing writes before staging. Independent addresses remain concurrent;
isolation and serializable validation remain separately selected. Cluster engines
reject this local-only strategy.

| Isolation | Reads | Conflict checks |
| --- | --- | --- |
| Read committed | Each operation sees current committed state plus its own writes | A written address changed after begin aborts. Reads can change and scans can gain rows. |
| Snapshot | Begin-time immutable state plus own writes | Same write/write check. Disjoint write skew is allowed. |
| Serializable, revision validation (default) | Begin-time immutable state plus own writes | Any intervening database revision aborts the session, including scans and read-only commits. This conservative check prevents predicate anomalies but can abort unrelated work. |

Optional serializable read-set validation accepts disjoint point reads/writes while
checking their observed addresses, including missing rows. Scans and name searches
still check the whole revision. It requires serializable isolation; default
revision validation is unchanged. See [configuration and tradeoffs](ENGINE-VALIDATION.md).

There are no dirty reads or dirty writes between sessions of the same engine.
These properties do not apply to independent writers or direct mutation of an
adapter behind the engine. Retry conflicts with a **new** session. A failed
commit attempt closes its session and discards its staged writes. Allocations
can leave gaps after rollback. An optional references, unique pairs or unique
names constraint validates the whole proposed state at commit. Raw mutation
never silently merges duplicate pairs or cascades deletes: select an existing
usages/uniqueness decorator if that policy is wanted. Legacy CLI backends retain
their established merge and cascading behavior.

## Commit, recovery and errors

A write commit encodes the complete next revision, appends its record, flushes
the log, then publishes the immutable state. The log is authoritative; there
is no separately acknowledged data checkpoint. This avoids mixed mapped data
and journal visibility. Full revision records are deliberately simple, not a
space-efficient WAL implementation. The optional [delta journal](ENGINE-DELTA.md)
compresses physical records without changing this logical commit boundary.

An append or flush error is an **indeterminate commit**: data may have reached
the log even if the caller got an error. The engine then rejects further
writes and engine snapshots. Close/reopen using the same authoritative log to
discover the outcome. Do not blindly retry an uncertain business operation;
use an application idempotency key or inspect recovered state first. JS reports
`LinksError.outcome === 'unknown'`; validation conflicts are `failed`.
Replication does not change these rules. Followers can contain an unacknowledged
commit and must not be promoted automatically.

The exclusive file adapters discard an incomplete final line and reject complete
corrupt records. Recovery checks checksum, UTF-8 names, address bounds, unique
addresses, contiguous revisions, nondecreasing allocator and selected constraints.
Checksums detect accidental corruption, not malicious tampering. Establish the
database directory and protect its metadata before depending on power-loss
recovery. Filesystem/device failure outside the adapter's flush contract is not
proven by ordinary process crash tests.

The legacy transaction decorators have a different architecture: their commit
marker is flushed before acknowledging commit, data is flushed before an applied
marker, and replay errors propagate. They still mutate the underlying store
while a transaction is open; use the staged engine for session isolation.
Sized and chunked retention discard completed rollback records without replaying
their writes. Chunk archives contain committed changes only; pending records
remain in the live log. Retention limits the in-memory list, while the durable
sidecar still keeps recovery markers. Optional diagnostic I/O failures preserve
the operation's outcome; storage and journal errors retain their error semantics.

Legacy Rust/C# transaction wrappers retain observed partial writes when a storage
operation fails. Automatic operations undo those observations; explicit
transactions keep them for caller rollback. This requires the adapter's change
callbacks. Rust panic unwinding and unobserved mutations are outside this contract.

Explicit-address allocation through the legacy Rust transactions/version APIs
is a logged write, including any intermediate placeholders retained by the
backend. Rollback and checkout undo those rows; automatic allocation flushes
its journal and data like other writes. Existing/zero addresses are no-ops.
Built-in map adapters observe the surviving row directly. The default observation
adapter for other backends compares complete before/after row maps, costing
O(N log N) time and O(N) space for this operation; a backend can provide its own
allocation callbacks. Bare `ensure_link_created` keeps its existing cost.

## Branches and merge recovery

A fork copies the parent's current committed rows into revision 1 of an empty,
independent journal. Choose its concurrency, isolation, constraints and retention
separately. Unfinished sessions on either engine remain private. Branch addresses
are local: independently allocating the same address with different values
creates a merge conflict. Equal complete rows converge; no address remapping is
performed. Use disjoint address ranges or externally assigned addresses when
independent branches must allocate without collisions.

Merge compares each changed complete row with its saved base and the current
parent. Disjoint changes combine; identical changes converge; divergent edits,
delete/update conflicts and name changes to the same row conflict. The parent
validates its constraints and flushes one complete revision before publishing any
merged rows. Exclusive admission and existing session conflict checks still
apply. Repeating an unchanged successful merge adds no revision. A three-way
merge validates its resulting state, but does not retroactively serialize the
independent transactions that produced both histories.

After a successful merge, persist `merge_base_record()` / `MergeBaseRecord` /
`mergeBaseRecord()` with application metadata. To reopen, open the same logical
parent and call `resume_branch` / `ResumeBranch` / `resumeBranch` with the branch
journal and descriptor. The descriptor contains the complete committed branch
base, so in-memory history eviction does not invalidate it. Recovery verifies
that it matches an actual branch journal revision and validates the complete
journal. The caller is responsible for associating it with the right parent;
the checksum is not an authenticated lineage identifier.

An older saved descriptor allows replay after a crash between parent commit and
descriptor persistence: already identical rows are accepted, while divergent
overlap conflicts conservatively. An append/flush error during merge has an
unknown outcome and poisons the parent; reopen it before outcome discovery or
retry. Failure to seed a branch leaves the parent untouched, but the branch log
may contain the seed and must be inspected/reopened rather than blindly reused.
Fork and merge currently copy full states and use the same O(N) record format.

## Distributed ownership and horizontal scale

Replicated logs have a fixed, externally fenced primary and fixed followers.
A follower must be an exact prefix of the primary; divergent or ahead followers
are rejected. Catch-up appends only missing records, then flushes. Scheduling is
explicit, so a bare store pays no background replication cost. Adapter resources
remain owned by the caller; close sessions and engines before releasing them.

A shared-storage deployment must enforce one writer through the file adapter's
advisory lock or an external fencing mechanism that every writer respects.
Local advisory locks are not a distributed lease and must not be assumed to
work on an arbitrary network filesystem. A shared-nothing deployment gives
independent logs and failure domains to followers and each independent shard.
Partition tenants/keys into separate engines to scale aggregate independent
work horizontally. Addresses are local to a shard; use `(shard, address)` in
application references and keep an atomic transaction within one shard.

Safe automatic primary election, changing membership, consensus, cross-shard
atomic transactions, and a routing/network service are not supplied by these
log decorators. They require protocols beyond acknowledgement counting. Existing
`RemoteLinks` TCP stores carry individual links operations; they do not create a
transaction boundary across connections. Do not infer any of those guarantees
from a replicated in-memory test.

## Costs and comparison

The staged implementation clones the current rows for each write commit and
writes a complete logical revision: O(N + W) memory/work plus O(N) journal bytes for N
rows and W staged writes. A scan currently materializes its view. History adds
up to O(HN) retained row memory. The allocator is shared. Optimistic read-committed
and snapshot sessions, plus serializable read-set sessions, track per-address changes only while another session can
be stale, retaining deleted-address tombstones to detect delete/recreate races.
The final session validates before reclaiming that map. Opening an engine,
exclusive admission and optimistic whole-revision serializable validation do not build
per-row generations. A continuously overlapping set of sessions can still retain
many changed addresses; end sessions promptly. Pessimistic admission additionally
tracks reserved write addresses and generations under all isolation levels; its
[separate cost description](ENGINE-PESSIMISTIC.md) explains ownership and cleanup.
Recovery reads the full log; retention bounds in-memory history, not log size.
Optional single-span delta encoding reduces physical bytes for localized changes
and adds an O(N) comparison; it does not compact history or remove full-state work.
Replication catch-up compares prefixes and copies missing complete revisions.
There is no background compaction or group commit. These limits matter for a
large or frequently written database.

| Competitor configuration | Closest available configuration | Material difference |
| --- | --- | --- |
| SQLite WAL with synchronous commit | Exclusive file log + staged engine, serializable | Full-state records instead of page WAL, conservative validation, no SQLite locking/checkpoint implementation or performance equivalence. |
| PostgreSQL READ COMMITTED / REPEATABLE READ | Read committed / snapshot | Similar visibility choices; no SQL planner, row locks or PostgreSQL vacuum. |
| PostgreSQL SERIALIZABLE | Serializable revision/read-set validation | Whole-revision scans or point dependencies replace SSI dependency tracking; unrelated predicates can still conflict. |
| FoundationDB conflict sets | Serializable read-set validation | Local address sets and whole-revision scans; no range index, distributed resolver, snapshot-read bypass or FoundationDB protocol. |
| RocksDB optimistic transactions | Optimistic admission + snapshot/serializable | No LSM tree, key-range locks, parallel validation, compaction or RocksDB performance parity. |
| RocksDB pessimistic transactions | Pessimistic point writes + selected isolation | Immediate busy errors, engine-local ownership and snapshot validation; no waiting, deadlock detector, range locks, LSM tree or performance equivalence. |
| InnoDB synchronous redo/group commit | Durable staged journal with explicit flush | No group commit, page WAL, buffer pool or doublewrite repair. Optional delta encoding reduces bytes, not the full-state CPU cost. |
| MongoDB durable write concern | Synchronous durable replica acknowledgements + outcome discovery | No MongoDB election/oplog protocol, tagged concerns or causal replica reads. A lost acknowledgement can follow a committed write. |
| Cassandra ordinary quorum writes | Fixed-primary synchronous replica log | No leaderless writes, timestamp reconciliation, hints or anti-entropy. Counting replicas does not provide these guarantees. |
| Cassandra Accord transactions | Selected-shard atomic catalog publication | One catalog serialization point, without Accord's dependency/epoch protocol or WAN latency properties. |
| Redis asynchronous replication | Async replicated log | Explicit caller scheduling and complete revisions; no Redis network/replication service. |
| Redis WAIT / WAITAOF style acknowledgement | Synchronous copies with durable adapters | Acknowledgements count flushes according to adapter contract, not consensus or automatic failover. |
| etcd consensus transactions | ClusterEngine + EtcdJournalStore | Linearizable selected-shard snapshots and atomic CAS; etcd supplies consensus. Full journals in one group limit data/write scale. |
| TiKV distributed transactions | Selected-shard atomic groups + partitioned journal stores | Independent data groups and atomic catalog publication; no TiKV lock/timestamp protocol, LSM storage or Region rebalancing. |
| Iceberg optimistic metadata publication | PartitionedJournalStore | Immutable journal objects plus atomic pointer CAS; different data/wire formats, full journal transfers, no manifest tree or garbage collector. |

See [the pinned source study](case-studies/issue-138/RESEARCH.md),
[acceptance inventory](case-studies/issue-138/REQUIREMENTS.md), and
[portable revision vectors](protocol/engine-revision-vectors.json). Tests and
coverage evidence must be read at their measured scope; neither a passing suite
nor a configuration analogy proves every ACID failure mode.

## Run the examples

```sh
cargo run --manifest-path rust/Cargo.toml --example engine_strategies
cargo run --manifest-path rust/Cargo.toml --example engine_strategies -- /existing-directory/links.journal
dotnet run --project examples/engine-csharp -- /existing-directory/links.journal
wasm-pack build --target web --out-dir ../../js/pkg rust/wasm
node js/client/examples/engine.mjs /existing-directory/links.journal
```

Add `--read-set` to any of these engine examples to select serializable point
validation and demonstrate two independent edits committing. It composes with
`--delta` and the optional journal path.

Use a separate journal per simultaneously running writer. To verify portable
recovery, stop one example before opening its journal with another language.
Every implementation uses the same `engine1` records; u32 consumers reject
addresses outside their range. C# and Rust support wider unsigned addresses.

The JS filesystem adapter is imported separately from
`@link-foundation/link-cli-client/engine-file-log`; the universal browser entry
point has no Node imports. It uses an atomic `journal.writer` directory to fence
writers. After an abrupt process exit, first verify/fence the old owner and then
remove that directory explicitly. It deliberately does not guess whether a
remote owner is dead or automatically break a stale lock. Native file adapters
use the existing advisory file-lock implementation, released by the OS at exit.
These lock formats differ: a JS writer directory does not fence a native writer's
advisory lock. Never open the same journal concurrently through different
adapters; mixed-language deployments need one externally enforced writer fence.
Replicas must be independent storage instances and failure domains. C# and JS
reject repeated object identities, but different wrappers can still alias the
same underlying storage; the adapter owner must prevent that aliasing.

JS/WASM custom adapters implement `read(): string` (a JSON array of record
strings), `append(record): void`, `flush(): void`, and a boolean
`supportsDurableFlush`. Append and flush must complete synchronously and return
undefined. A Promise cannot provide this boundary and is rejected. Browser
IndexedDB and other asynchronous storage require a server-side engine or a
separate asynchronous transaction interface; an exported in-memory journal is
not a durable commit. Caller-owned adapters outlive all sessions that use them.

## Coordinated network groups

[ClusterEngine](ENGINE-CLUSTERS.md) composes the same links API with an atomic
journal store. The optional etcd adapter supplies linearizable snapshots, atomic
publication across selected shards, and consensus failover. It bounds network
payloads and preserves unknown outcomes. Full journals in one coordinator group
limit scale. `PartitionedJournalStore` moves immutable journals to independently
routed data groups and retains atomic pointer publication in the catalog. See the
deployment and costs guide for topology, object retention and throughput limits.

C# named, typed and simple file constructors release their owned data handle
if names construction fails. A failed constructor over a caller-supplied store
leaves that store usable. Partially initialized names mappings are released on
both raw-facade and names-content errors; no GC/finalizer wait is required.

The legacy transaction decorators retain every observed undo record before
appending a logical operation's journal batch. An append failure makes an
explicit transaction rollback-only: roll it back before writing again. Automatic
operations attempt rollback immediately. If cleanup fails, the error remains
visible and the transaction stays pending; retry `Rollback()` on the C# decorator
or `rollback()` in Rust after repairing the storage failure. Rust mapped stores
forward upstream cascade callbacks, including uniqueness merges and dependent
deletes. These in-process undo guarantees do not close the legacy crash window
between in-place mutation and its journal append; use the staged engine when
unpublished mutations must stay outside the authoritative store.

Beginning rollback also makes the legacy transaction rollback-only before any
undo or persistence step. An error during undo, data flush, rollback-marker append
or journal flush retains that abort decision. Repair the failure and retry
rollback; commit and new writes remain rejected until cleanup succeeds.

Rust mapped creation also forwards allocation, update and uniqueness callbacks.
Resolving a duplicate can return an existing address after deleting a temporary
row; rollback must preserve the existing row and its dependents. A rejected
creation can leave an allocated temporary row, so its observations are retained
even on error. The default `LinksStorage::create_link_observed` is for atomic
single-row creation; adapters with partial writes or cascades must override it.

Legacy signed journal sequence numbers never wrap. Already-exhausted logs reject
writes before mutation. If an observed cascade needs more numbers than remain,
all its undo state stays in pending memory, outside numbered history and the
journal; automatic operations attempt rollback and explicit transactions require
rollback. A rejected callback batch does not consume numbers; preparatory
nested writes may already have consumed valid numbers and are undone with the
operation. As with other legacy observed writes, this does not close the crash
window before the batch is journaled.
