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
| Durable single writer | Staged engine + exclusive file log, require durable flush | One authoritative, checksummed logical revision is appended and flushed before publication. Storage must honor flush; one writer owns the log. |
| Physical page WAL | Staged engine + optional SQLite log, require durable flush | SQLite supplies WAL/FULL and checkpoints; one engine owns its connection and cache. [Providers, maintenance, costs and examples](ENGINE-SQLITE.md). |
| Bounded time travel | Set Rust `HistoryPolicy::Last(n)`, C# `HistoryLimit = n`, JS `history: n` | Retain up to n revisions including current. Active sessions and previously returned snapshots pin their versions independently. |
| Complete time travel | Rust `HistoryPolicy::All`, C# `HistoryLimit = null`, JS `history: 'all'` | Every replayed/committed revision remains addressable in memory. |
| Independent branch | Rust `Engine::fork`, C# `LinksEngine.Fork`, JS `WasmEngine.fork` | Seed and flush a separate journal; edit privately and merge committed changes atomically into the parent. |
| Asynchronous copies | Wrap a log with replicated log, async policy | Commit waits for primary only. Call synchronize under the engine's log mutex to catch up followers. A primary loss can lose uncopied commits. |
| Synchronous copies | Replicated log, acknowledgements k including primary | Commit waits for primary and enough successful follower flushes. Failure can leave a committed primary or partial set of copies; outcome discovery is mandatory. |
| Serializable independent point edits | Select `SerializableValidation::ReadSet` / `SerializableValidation.ReadSet` / `serializableValidation: 'read-set'` | Track observed addresses and validate concurrent changes at commit; scans still check the whole revision. [Recipe and costs](ENGINE-VALIDATION.md). |
| Localized journal edits | Optional `DeltaLog` / `DeltaCommitLog` around the selected log | Compress consecutive complete snapshots with a checksummed changed span; logical change records pass through. See [format, composition and examples](ENGINE-DELTA.md). |
| Retain small-edit immutable states | Select shared state independently of the other options | Path-copy changed entries instead of cloning a complete row map. See [costs and examples](ENGINE-SHARED-STATE.md). |
| Serialize small edits | Select change records independently | Encode changed rows plus base/allocator metadata. Combine with shared state to avoid full candidate copies; see [costs and protocol](ENGINE-CHANGE-RECORDS.md). |
| Independent commits sharing a flush | Opt-in `commit_group` / `CommitGroup` / `commitGroup` | Preserve each accepted session's revision and share one durability boundary. [Ownership, failure outcomes and costs](ENGINE-GROUP-COMMIT.md). |

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

A write commit encodes the next logical revision, appends its record, flushes
the log, then publishes the immutable state. The log is authoritative; there
is no separately acknowledged data checkpoint. This avoids mixed mapped data
and journal visibility. Full revision records are deliberately simple, not a
space-efficient WAL implementation. The optional [delta journal](ENGINE-DELTA.md)
compresses consecutive full snapshots. Optional [change records](ENGINE-CHANGE-RECORDS.md)
serialize changed rows. Both preserve this logical commit boundary.
The optional [SQLite journal](ENGINE-SQLITE.md) stores those records inside
SQLite page-WAL transactions and exposes physical checkpoint maintenance.

An append or flush error is an **indeterminate commit**: data may have reached
the log even if the caller got an error. The engine then rejects further
writes and engine snapshots. Close/reopen using the same authoritative log to
discover the outcome. Do not blindly retry an uncertain business operation;
use an application idempotency key or inspect recovered state first. JS reports
`LinksError.outcome === 'unknown'`; validation conflicts are `failed`.
Replication does not change these rules. Followers can contain an unacknowledged
commit and must not be promoted automatically.

The exclusive file adapters discard an incomplete final line on open and before
read, append or flush after a writer I/O error. Failed repair keeps further
operations fenced until repair succeeds. Complete records remain intact, even
when an error followed their write; this does not authorize retrying an uncertain
business operation. A repair scans backwards with an 8 KiB buffer. Successful
operations only check a flag, with no rescan. C# disables its managed write
buffer so failed buffered bytes cannot reappear during repair; append therefore
performs a file write immediately. These costs belong to the selected file log.
Complete corrupt records are rejected. Recovery checks checksum, UTF-8 names,
address bounds, unique addresses, contiguous revisions, nondecreasing allocator
and selected constraints.
Checksums detect accidental corruption, not malicious tampering. Establish the
database directory and protect its metadata before depending on power-loss
recovery. Filesystem/device failure outside the adapter's flush contract is not
proven by ordinary process crash tests.

The legacy transaction decorators have a different architecture: their commit
marker is flushed before acknowledging commit, data is flushed before an applied
marker, and replay errors propagate. They still mutate the underlying store
while a transaction is open; use the staged engine for session isolation.
Their complete journal records undo and version ordinary named writes, including
unique-name moves and names removed by deletion. Name-only versions, archive
replay and movement recovery use the complete payload; row-only snapshots retain
their original API. See [transaction names](TRANSACTION-NAMES.md) for codecs,
reader compatibility, named-adapter requirements and the cost of sidecar scans
and pre-effect journal flushes. Staged sessions include names in atomic revisions.
Sized and chunked retention discard completed rollback records without replaying
their writes. Chunk archives contain committed changes only; pending records
remain in the live log. Live C# retention checkpoints current inline effects
when an older committed record lacks an applied marker, preserving newer edits.
Retention limits the in-memory list, while the durable
sidecar still keeps recovery markers. Optional diagnostic I/O failures preserve
the operation's outcome; storage and journal errors retain their error semantics.

Legacy Rust transaction IDs exclude every recovered commit and abort decision,
even if the wall clock repeats after a process restart. Explicit and automatic
owners use the same allocation check. It uses the existing decision hash sets:
expected O(1) per candidate, with O(K) extra candidates when a restarted counter
meets K historical IDs at the same timestamp. The counter advances as collisions
are skipped; no database scan, extra retained index or random dependency is added.
These are journal-local identities. This check does not repair older journals
already containing reused IDs or conflicting decisions.

Native chunk archives claim a previously absent path atomically. Rust advances
its candidate counter and retries occupied names, including existing symlinks;
K collisions cost O(K) additional opens without scanning the directory. Other
filesystem errors retain committed cleanup for retry. C# uses random GUID names
and exclusive creation too; a rejected optional archive leaves its records in
the live log, with a new candidate on a later retention attempt. Existing entries
and symlink targets remain untouched. Both adapters keep the live journal as
recovery authority. Archive contents are not atomically published after a partial
write, and these checks do not establish archive power-loss durability. Native
filesystem archives are unavailable in the linked WASM backend.

Legacy Rust/C# transaction wrappers retain observed partial writes when a storage
operation fails. Automatic operations undo those observations; explicit
transactions keep them for caller rollback. This requires the adapter's change
callbacks. Rust panic unwinding and unobserved mutations are outside this contract.

Public `ApplyTransition`/`RevertTransition` helpers (Rust `apply_transition`/
`revert_transition`) also reject a live owner before touching data. Finish the
original transaction or its failed commit/rollback cleanup before unlogged
replay. These helpers remain available to higher-level version decorators once
ownership is released and do not append another transition. The admission check
costs O(1), with no journal scan or allocation on success.

After a legacy journal commit is known durable, a failed synchronous checkpoint
keeps the transaction owner and blocks later row and named writes. In Rust,
retry `commit`, `rollback` or `save`; in C#, retry the handle's `Commit`,
`CommitAsync`, `Rollback` or `Dispose`. The C# decorator's `Rollback()` also
retries an automatic operation's checkpoint. Cleanup finishes the durable
commit decision; it cannot undo it. C# `IsCommitted` reports that decision even
when checkpoint persistence still needs retry. Its normal asynchronous policy
continues to release admission once the durable decision is queued.
C# transaction owners wait asynchronously for background work, so idle owners
do not reserve ThreadPool threads. One consumer checkpoints queued batches in
order without another pool dispatch. Shutdown cancels queued work and joins
in-flight work before borrowed stores may close; cancelled durable decisions
remain recoverable on reopening. The queue and awaitable worker are allocated
only when this transaction decorator is selected.
An error during the commit-marker append or its first flush can also leave a
durable decision. The owner retains that attempt and fences further writes;
the same cleanup entry points resolve it as a commit instead of undoing it.
`IsCommitted == false` after a failed C# commit therefore does not establish an
abort. Uncertain retries scan the journal to avoid duplicate decision markers;
normal commits and known-checkpoint retries do not pay that O(journal length)
read cost. A failed retry read retains ownership too. C# retries finish
synchronously even through `CommitAsync`.
Explicit and automatic version owners retain admission until their checkpoint
and branch publication succeed, including named writes. Automatic row writes
create an implicit version owner before mutation. Pre-commit write/callback/log
errors undo through that owner; a failed undo keeps it for cleanup retry. After
a commit attempt, cleanup resolves that attempt and finishes publication instead
of undoing a possibly durable decision. Rust `commit`, `rollback` and `save`,
and C# owner/graph disposal, retry this retained work. Existing/no-op allocation
and lookup do not create an owner or decision; C# asynchronous commits retain
their selected queue policy. This adds the same version-owner bookkeeping as an
explicit operation, without selecting a stronger inner checkpoint policy.
The version layer also persists a publication intent before the inner decision.
Startup resolves it after journal redo/undo, restoring the exact committed branch
head or clearing an aborted attempt. Committed attribution is flushed before the
intent is removed and flushed. Every failure retains admission until cleanup
finishes. This adds one temporary metadata link and three ordered metadata
flushes per version commit; bare transaction/cache paths pay no such cost.
C# exposes `SupportsDurablePublication` and accepts an explicit `flushBranches`
callback. Rust's default metadata adapter flushes its files; custom adapters
must honor `NamedTypeLinks::save`. In-memory WASM tests verify logical behavior,
while native finite child tests verify actual process exit without destructors.
Neither establishes device power-loss behavior. See the
[intent protocol](protocol/version-metadata.md#pending-transaction-publication).
Creating the legacy version layer over an existing transaction journal attributes
only committed records. Rolled-back and recovered pending sequences are not
versions and do not advance the initial branch head. Construct/recover the layer
after an existing transaction finishes. Startup also rejects a sidecar that
attributes a known uncommitted record, including older incorrect attribution;
it leaves those files intact rather than guessing a repair of already altered
data. This validation scans the retained journal and stores its uncommitted
sequence numbers during metadata recovery. Checkout and branch switching resolve
all changed undo and redo records before mutating rows. If bounded retention has
removed one, they reject the move and preserve rows, branch selection and heads.
An unavailable shared prefix needs no replay, so moves over an available suffix
still work. Resolution snapshots the retained log once and indexes it by sequence:
O(N + V) time and O(N + V) temporary memory for N retained records and V changed
records. Retention does not promise that every stored version remains replayable.
The durable movement intent described below retains ownership during failed replay.
Legacy branch path construction walks ancestors iteratively and scans transition
attribution once, rather than recursing and rescanning for every parent. With D
ancestors, A attributed transitions and P selected records, native Rust takes
expected O(D + A + P) time; C# additionally sorts selected per-branch buckets,
with O(P log P) worst-case sorting. Both use O(D + P) temporary memory and
constant call-stack space. These bounds treat key hashing/comparison as unit
cost; long branch names add their hashing/comparison cost. Each branch's own head and all descendant fork
cutoffs still apply; records remain grouped root to leaf, then ascending within
each branch. These costs apply only to the selected legacy version wrapper;
there is no persistent extra index or work on bare cache paths. The separate
retained-journal diff-resolution cost above still applies. Finite stack tests
use a metadata-only adapter to isolate traversal; real-name semantic tests
verify actual native reopenings. WASM controls verify logical behavior without
claiming a native stack overflow or filesystem durability there.

Recovery replays the complete committed suffix after its first missing applied
marker, including later records that already have markers. It flushes the final
redo and pending undo before filling that gap, keeping an anchor for another
restart if recovery fails before the final data flush.
Both Rust and C# reject public recovery while an open transaction owns live
state; finish its cleanup first. Recovery first builds temporary indexes and
validates every recognized record before replacing live history or replaying
rows. It rejects conflicting records for one sequence, opposite decisions for
one owner, zero owners and nonpositive sequences. Parsed identical records and
identical decisions are idempotent; legacy lexical forms, unrelated names, gaps
and applied checkpoints whose payloads were retained away remain accepted.
Journal preparation costs expected O(N + U log U) time; its temporary indexes
use O(U + M) memory in addition to the adapter's journal read, for N records,
U unique transitions and M unique markers. Hash indexes
replace repeated ordered-vector insertion. These costs apply only to the
selected transactions wrapper; standalone codecs and bare stores are unchanged.

A failed journal read, validation, replay or persistence step retains recovery
ownership. Writes, new transactions and unlogged/version replay remain fenced
until recovery succeeds; C# queued checkpoints and retention also wait. Retry
Recover, or decorator rollback; Rust save/commit are also cleanup entry points.
A failed Rust reload additionally retains its underlying refresh obligation,
including when the external journal already marks every transition applied.
Cleanup retries that refresh before admitting writes. Reads can still observe
the current mutable view during failed recovery; this legacy API supplies no
snapshot isolation. Raw inner-store access remains the caller's responsibility.
Default-off tracing reports failed recovery while preserving its original error.
Legacy in-memory transactions and history also
run in browser/Node WASM, using the JS wall clock for timestamps. File-backed
adapters and chunk archives still require a supported filesystem.

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
validates its constraints and flushes one selected logical revision before publishing any
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
Fork and merge use the receiving engine's selected flat/shared map and the same
O(N) record format. Mixed representation forks convert their initial rows.

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

The default flat representation clones current rows for each write commit and
writes a complete logical revision. Candidate creation costs O(N); applying W
writes costs O(W log N), followed by the selected revision encoding. Flat scans
materialize their view and history can retain O(HN) map entries. Optional
[shared immutable state](ENGINE-SHARED-STATE.md) replaces those map copies with
an O(1) root and O(W log N) changed paths, preserving earlier snapshots and
sharing unchanged entries during recovery. Optional [change records](ENGINE-CHANGE-RECORDS.md)
serialize changed rows. Selected constraints and scans still visit the full state. The allocator is shared. Optimistic read-committed
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
and adds an O(N) comparison for full records; changes pass through without that
comparison. Neither compacts history. Replication catch-up compares prefixes
and copies missing logical records.
Optional [caller-selected group commit](ENGINE-GROUP-COMMIT.md) shares one flush
across independent sessions while buffering their selected logical records.
There is no automatic grouping scheduler or background compaction. These limits
matter for a large or frequently written database.

| Competitor configuration | Closest available configuration | Material difference |
| --- | --- | --- |
| SQLite WAL with synchronous commit | Optional SQLite log + staged engine, selected isolation | Reuses SQLite page WAL/FULL and checkpointing; logical links revisions still replay the retained journal. No SQL planner or independent-engine MVCC/performance equivalence. |
| PostgreSQL READ COMMITTED / REPEATABLE READ | Read committed / snapshot | Similar visibility choices; no SQL planner, row locks or PostgreSQL vacuum. |
| PostgreSQL SERIALIZABLE | Serializable revision/read-set validation | Whole-revision scans or point dependencies replace SSI dependency tracking; unrelated predicates can still conflict. |
| FoundationDB conflict sets | Serializable read-set validation | Local address sets and whole-revision scans; no range index, distributed resolver, snapshot-read bypass or FoundationDB protocol. |
| RocksDB optimistic transactions | Optimistic admission + snapshot/serializable | No LSM tree, key-range locks, parallel validation, compaction or RocksDB performance parity. |
| RocksDB pessimistic transactions | Pessimistic point writes + selected isolation | Immediate busy errors, engine-local ownership and snapshot validation; no waiting, deadlock detector, range locks, LSM tree or performance equivalence. |
| InnoDB synchronous redo/group commit | Durable staged journal + opt-in independent-session group commit | Shared flush with separate revisions; optional SQLite supplies its pager/WAL. No InnoDB buffer pool, automatic grouping scheduler or doublewrite repair. Shared state and change encoding independently reduce localized copy/serialization costs. |
| MongoDB durable write concern | Synchronous durable replica acknowledgements + outcome discovery | No MongoDB election/oplog protocol, tagged concerns or causal replica reads. A lost acknowledgement can follow a committed write. |
| Cassandra ordinary quorum writes | Fixed-primary synchronous replica log | No leaderless writes, timestamp reconciliation, hints or anti-entropy. Counting replicas does not provide these guarantees. |
| Cassandra Accord transactions | Selected-shard atomic catalog publication | One catalog serialization point, without Accord's dependency/epoch protocol or WAN latency properties. |
| Redis asynchronous replication | Async replicated log | Explicit caller scheduling and complete journal prefixes; no Redis network/replication service. |
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
`--delta`, `--shared`, `--changes`, `--pessimistic` and the optional journal path.

Use a separate journal per simultaneously running writer. To verify portable
recovery, stop one example before opening its journal with another language.
Every implementation accepts the same `engine1` and selected `engine2` records; u32 consumers reject
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
