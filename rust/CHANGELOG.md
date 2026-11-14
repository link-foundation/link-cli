# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- changelog-insert-here -->

## [3.1.0] - 2026-11-14

### Added

- `link_cli::protocol::ipc` carries the LiNo text and binary protocols over every IPC mechanism of IPC transports v1 (`docs/protocol/ipc-transports-v1.md`): TCP and UDP loopback, Unix domain sockets, Windows named pipes, POSIX FIFOs, anonymous pipes (`stdio` and `exec:`), shared memory, memory-mapped files, files on disk, OS signals, POSIX and System V message queues, mailslots, `WM_COPYDATA` and ALPC. An endpoint string selects the mechanism, `LinksServer` and `LinksClient` take any endpoint, `register` adds custom kinds, and every kind interoperates with the C# `clink` and the JavaScript client (#140).
- `clink --serve` repeats to serve one store on several endpoints at once, `--forward` relays every connection of the `--serve` endpoints to another endpoint without opening a database (so transports stack), `--connect` takes any endpoint and `--trace` (or `CLINK_IPC_TRACE=1`) logs every transport event (#140).

### Changed

- `libc` and `errno` are regular Unix dependencies again (not only of the `native-pages` feature), because the POSIX IPC transports of `protocol::ipc` use them in every build (#140).

### Fixed

- `link_cli::protocol::ipc` builds for `wasm32` without warnings.
- Keep the dependent WASM lockfile in the same verified release commit as the native manifest and lockfile, preventing `--locked` CI failures after automatic releases.

### Fixed

- Shared-memory, memory-mapped-file and file-ring IPC connections no longer drop the last bytes a peer writes right before it ends its direction: the reader reloads the ring positions after seeing the writer end and only reports the end of the stream once the ring is still empty (#140).

### Fixed

- Datagram IPC sessions (`udp`, `mq`, `sysvmq`, `mailslot`, `copydata`, `alpc`) acknowledge an in-order DATA or EOF frame before the reader can see it, and a half-close whose end the peer never acknowledged because it already hung up now succeeds like TCP's `shutdown(SHUT_WR)`. Before, a reader that hung up right after reading the end (removing a System V queue discards a pending acknowledgement, a full POSIX queue drops it) made the writer's half-close fail with "the peer no longer reads" although every byte was delivered (#140).
- Failing to disable Nagle's algorithm on a TCP IPC stream is traced instead of being silently ignored (#140).

Update links-notation to 0.31.4 and refresh locked dependencies.

## [3.0.1] - 2026-11-13

Update rusqlite to 0.42.0, ureq to 3.5.2, errno to 0.3.15 and rustls-webpki to 0.103.17 so the Dependencies check passes; cargo-audit stays held back at 0.22.2 until taiki-e/install-action ships a verified 0.23.0 manifest (issue #176).

Remove `package.homepage`, which only repeated `package.repository` and now trips Cargo's `redundant_homepage` lint under `-Dwarnings`.

In the WebAssembly wrapper, make `console_error_panic_hook` a wasm32-only dependency (it is used only there) and drop the unused `web-sys`, which Rust 1.100.0's `cargo::unused_dependencies` lint rejects.

## [3.0.0] - 2026-11-11

Reuse the standard protocol with selected query address types, retaining the
u32 facade. Add original-query-text envelopes and a standard JS/TS BigInt client
over actual WASM causal workspaces, with bounded request parsing and explicit
preparation/publication ownership. Preserve upstream LiNo comment boundaries.

Flush data and the transaction journal before checkpoint/rollback markers;
reject corrupt complete transaction records and propagate recovery errors.
Transition replay helpers now return Result instead of hiding storage failures.

Keep optional storage, query, server, transaction, trigger and version
diagnostics best effort.
Reclaim rolled-back retention records without replaying aborted writes, exclude
them from committed archives, and leave pending transactions unreclaimed.

Journal public explicit-address allocations and all retained placeholders so
rollback, recovery and branch checkout include them. Return I/O errors through
fallible named-query bridges and release failed automatic create operations.

Record storage callbacks even when the operation subsequently fails, so automatic
partial writes roll back and explicit transactions retain their undo information.
Preserve the original absent state across repeated create/update callbacks.

Forward mapped-store cascade observations and restore original contents of slots
allocated while undoing a higher address. Retain the entire observed undo batch
before journal I/O, reject commits and writes after an append failure, and roll
back failed automatic appends while retaining pending state if cleanup fails.

Fence commit and further writes as soon as rollback begins, retaining the abort
decision across undo, data-flush, rollback-marker and journal-flush errors.

Observe mapped creation callbacks rather than assuming the returned address is
new. Preserve existing duplicate rows during rollback/recovery and undo temporary
allocations when a validator rejects creation after allocation.

Reject exhausted legacy transaction sequence space before mutation and reserve
capacity for complete observed batches. Retain an unnumbered batch only as
pending undo when a cascade exceeds capacity; never serialize wrapped numbers.

Initialize legacy version history from retained journal records rather than
manufacturing versions for every integer across sparse sequence gaps.

Persist branch and parent names with an unambiguous versioned marker, retaining
legacy reads. Recover tag names containing equals signs without truncation.

Retain a known committed transaction until its data checkpoint and journal
markers finish. Commit, rollback and save retry that checkpoint without a new
commit decision; subsequent writes wait. Preserve the version owner until
branch publication succeeds, including named writes, and reject public recovery
while a transaction owns live state.

Use portable transaction timestamps in browser/Node WASM while retaining native
clock semantics. Run ordinary legacy transaction and history contracts in WASM.

Apply transaction admission checks to named writes, including the version
bridge, while failed committed checkpoints or rollback cleanup still own it.

Recover the complete committed suffix after an older applied-marker gap and
flush final redo/undo before advancing applied markers. Preserve later committed
updates, deletion and recreation across interrupted recovery and retries.

Retain uncertain commit attempts before marker append/flush, fence new writes
and resolve the decision through commit/rollback/save without undoing a possibly
durable commit. Avoid duplicate decision markers on retry and finish version
publication after cleanup resolves the inner decision.

Exclude aborted and recovered pending records from initial version history and
its branch head. Reject existing sidecars that attribute a known uncommitted
record, and reject version construction/recovery over a live inner transaction.

Retain an implicit version owner for automatic writes through failed commit and
metadata publication. Retry committed work through commit/rollback/save, preserve
ordinary pre-commit undo, and fence new mutations until the original branch is
published. Keep existing/zero-address allocation and existing lookup as no-ops.

Persist a version-publication intent before committing the journal, flush exact
branch attribution before clearing it, and recover interrupted publication after
abrupt process exit. Keep admission through metadata/flush errors and reject
malformed or contradictory intents without rewriting sidecars. Permit custom
metadata adapters through the common links API while retaining the default type.

Resolve every changed undo and redo transition before checkout or branch
switching. Reject missing retained history without changing rows or metadata,
while preserving no-op and available-suffix moves over an unavailable shared
prefix. Index the retained journal once instead of rescanning it per transition.

Fence public unlogged transition apply/revert while a transaction or its failed
commit/rollback cleanup still owns the data. Preserve retry of that owner and
ordinary unlogged version replay after ownership is released.

Repair partial file-journal writes before reusing the same writer for read,
append or flush. Retain the repair requirement across repeated I/O failures,
preserve complete records after uncertain writes and use bounded tail scanning.

Persist resolved checkout/branch-switch replay intents before changing rows.
Retain exclusive replay ownership through metadata and flush failures, finish
exact positions on reopen and reject contradictory movement metadata. Preserve
resolved records across retention and sequence floors after payload eviction.

Avoid reusing recovered commit/abort transaction IDs after a Rust process restart
with a repeated wall clock. Check both explicit and automatic owners against the
existing decision indexes, preserving wire compatibility and default-off tracing.

Claim chunk archive filenames exclusively and retry occupied candidates after
clock/counter reuse, preserving other journals' archives and symlink targets.
Keep non-collision filesystem failures visible and committed cleanup retryable.

Validate recovery journals before replacing live history or replaying rows.
Reject conflicting sequence records, contradictory decisions and invalid owners
or sequences while preserving idempotent parsed duplicates and retained floors.
Retain admission across failed recovery and reload, retrying an incomplete data
refresh before following writes or replay can proceed.

Construct branch paths iteratively without stack growth per ancestor, scanning
transition attribution once. Preserve nested fork cutoffs, per-branch heads and
root-to-leaf replay order, including empty histories and subsequent edits.

Record ordinary name changes and row-associated names in complete portable
journal records. Restore unique-name owners and deleted/cascaded names during
rollback; publish name-only versions and preserve names through recovery,
movement intents and chunk archives. Retain the original Copy row transition
and v1 codec; expose complete record snapshots and replay alongside them.

Select optional pre-effect logical row/name undo images for ordinary transactions.
Retain preparation and cleanup ownership, validate mandatory metadata before
mutation and reconcile unreported effects before committed history/publication.
Preserve the default observer cost path and existing transition codecs; recover
unknown allocations and redo later committed checkpoints after image restoration.

Add an optional staged links engine with optimistic/exclusive admission,
configurable isolation and immutable history, commit-time constraints, portable
checksummed revisions, and asynchronous/synchronous fixed-primary log adapters.
Expose sessions through the existing links API and the WASM binding.

Support independent journal branches and atomic three-way merge, with saved
merge bases for reopen/retry after uncertain I/O outcomes.

Compose atomic multi-shard transactions with replaceable journal stores and an
optional etcd HTTP adapter, with bounded I/O, linearizable snapshots, generation
validation, unknown outcomes and actual three-member failover integration.

Route immutable journals to independent data stores while atomically publishing
small catalog pointers; preserve cross-shard validation and unknown-outcome
discovery across independent consensus groups.

Keep optional engine and replica diagnostics best effort when stderr fails,
so a diagnostic write cannot panic after durable publication or poison the
engine's commit mutex.

Select optional pessimistic point-write ownership with immediate busy errors,
independent isolation/read-set validation, stale/ABA rejection, branch protection
and cleanup on every session outcome. Keep engine-local locks out of cluster
workspaces and preserve existing optimistic/exclusive defaults.

Commit caller-selected groups of independent sessions with separate revisions
and one shared durability flush. Preserve input-order validation, known aborts
and indeterminate prefix recovery without publishing uncertain state. Expose
matching Rust-backed WASM/JS ownership and outcome contracts. Avoid flushing
the primary twice during synchronous replica acknowledgement.

Select structurally shared immutable ordered engine state independently of
isolation, history and journal settings. Preserve flat-map defaults and portable
revision records; share unchanged entries across live and recovered revisions,
scans, group preparation and branches. Keep the public BTreeMap snapshot view
through lazy materialization and provide borrowed iteration without that copy.

Select canonical changed-row revision encoding independently of state structure
and other engine policies. Preserve complete-snapshot defaults, mixed journal
recovery, saved branch bases, shared group flush and physical delta composition.
Apply selected change records against immutable predecessor roots and validate
base, allocator, row/name operations and checksums before publication.

Select an optional bundled SQLite physical page-WAL journal. Preserve staged
reads, atomic group flushes and uncertain outcomes; fence stale owners, validate
schema/UTF-8/sequences, enforce WAL/FULL and expose bounded checkpoints. Keep
default and bare-WASM builds free of the selected native SQLite component.

Select optional persistent constraint indexes independently of map, encoding and
other engine policies. Allocate only selected pair/name/reference indexes, allow
atomic swaps, count incoming references and publish roots only after flush.
Preserve scan defaults, historical replay validation and uncertain group outcomes.

Select a bounded immutable journal object cache for partitioned stores. Keep
catalog reads and publication checks fresh, return owned records and retain no
cache by default. Reuse the maintained LRU component with standard hashing.

Reject cycles and missing nodes while decoding persisted Unicode sequences.
Preserve repeated shared subtrees, UTF-16 code-unit extraction and valid deep
graphs. Add a checked sequence walker; retain the existing Vec API with bounded
failure for malformed graphs. Keep name and metadata errors fallible.

Format deep link structures with explicit source-spine frames and one output
buffer. Preserve reference order, names, cycle handling and generic-store
errors while avoiding recursive stack exhaustion and repeated subtree copying.

Preserve exact names in the legacy text store, including whitespace, quote
edges, control characters and names without rows. Keep raw legacy backslashes
literal and use versioned UTF-8 name metadata for controls and absent addresses.
Retain live rows, names, indexes and allocation when a complete reload fails.

Select a SQL-free native doublets page journal independently of logical engine
and replication strategies. Validate independent versioned binary vectors,
canonical page and record chains, immutable publication and uncertain flush
fencing. Provide an optional native exclusive file device with durable flush,
parent directory synchronization on POSIX, bounded metadata reads and explicit
close error fencing. Keep native I/O dependencies out of default and WASM builds.
Expose the portable page codec to WASM host devices and a public native example.

Add independently selected two-copy native page repair, with immutable format
mode, full-prefix validation before repair and uncertain-outcome ownership fences.

Accept equals signs in CLI tag names using the first resolvable complete suffix.
Preserve aliases to existing delimiter-containing tags and invalid-point errors.

Add independently selected immutable sorted-run engine state in native Rust and
Rust-backed WASM/JS/TS. Share run arrays across revisions, merge with newest-write
precedence and retain existing logical journals and links/session interfaces.

Return allocation and journal errors through named query planning and persistent
transformation composition instead of entering infallible transaction/version
bridges. Add a default fallible pair lookup/allocation to the named links trait.

Propagate allocation errors from explicit and nested text imports and binary
archives, including row reallocation after a selected trigger removes it.

Export portable retained-history checkpoints and optionally publish them through
coordinated generation CAS without rewriting local append-only journals.
Compose the initial checkpoint with optional physical delta encoding and either
replication order, preserving exact replay and uncertain-operation fencing.

Add an optional ordered-journal replacement capability and portable two-bank
page journals. Synchronize the complete inactive image before its epoch-bound
root decision; fence uncertain I/O and recover complete old or new records.
Preserve ordinary page formats, caller-owned devices and single/mirrored modes.

Publish retained journal checkpoints through optional replacement logs while
holding the engine commit mutex. Preserve live sessions, history, branch bases
and committed allocator positions; fence uncertain outcomes until reopening.
Compose physical delta compression and expose compaction through WASM bindings.

Add an optional native checkpoint journal factory owning the raw root and both
single or mirrored banks. Acquire all writer fences before recovery, attempt
every cleanup in reverse order, preserve all constructor/cleanup errors and
retain uncertain native fences without flushing pending work on close.

Select mirrored checkpoint roots independently of bank mirroring, preserving
existing single-root APIs and costs. Validate both root copies before choosing,
validate the committed bank before repair, synchronize both decisions before
acknowledgment and reject incompatible persisted modes before mutation. Native
factories retain all raw owners through acquisition, recovery and reverse cleanup.

Add an optional namespace-maintenance capability with complete bounded,
revision-consistent etcd scans and single-send generation-conditional deletion.
Keep ordinary atomic stores and existing partitioned retention unchanged.

Select explicit epoch-fenced immutable journal reclamation with bounded complete
root discovery, exact-generation deletion and coherent owned-reader refresh.
Preserve append-only partitioned stores and default zero-cache costs. Provide
portable wire vectors and native/WASM retained-history checkpoint handoffs.

Select immutable physical sorted journal tables independently with `sorted-tables`.
Retain exact tombstone generations, bounded complete validation, physical record
addresses and page hashes for indexed point reads. Preserve default core and
SQLite optionality, and verify native file interchange and actual WASM controls.

Add optional `physical-lsm` WAL semantic replay and validated checkpoint encoding,
with independent shared vectors and actual-WASM controls. This component performs
no I/O; physical table selection, publication and native ownership composition
remain separate required work.

Add the optional native `FileLsmJournalStore` facade with bounded read-only
discovery before repair, independent root/bank/table mirrors, exclusive directory
ownership and cached uncertain cleanup. Preserve the physical store and WAL
formats across Rust, C#, Node, Bun and Deno; add complete-store handoffs, actual
cluster consumers and finite native failure/process-exit controls.

Select bounded immutable journal chunks independently of caching and engine
policies. Reuse unchanged complete chunks while preserving whole-journal
defaults, atomic catalog publication, owned reads, collision checks and
cross-language native file interchange. Reject malformed manifests and complete
update admission failures before data access; retain explicit staging/replay
costs and the separate epoch-reclamation format.

Compose optional bounded immutable journal chunks with epoch-fenced reclamation.
Protect all live references, stage each publication in its captured epoch and
refresh whole reader groups after retirement. Preserve blob2 defaults and
owned snapshots, and verify native file and consensus handoffs across runtimes.

Retain checked mapped-view flush ownership through Rust storage decorators and
explicit generic adoption. On Windows, synchronize the current mapped view
before its owned file buffers. Reject failed remaps and retired handles, and
release retained file handles when the mapping closes. This barrier does not
supply physical torn-page repair or hardware power-loss certification.

Prepare replacement views before retiring plain-data Rust mappings. Rejected
file growth, capacity overflow and failed initialization retain the old pointer,
contents and checked flush handle. Initialize fresh padding and failed-extension
tails exactly. Windows extends through a temporary mapping object while its
old view remains live. Generic destructor values retain their existing path;
failed shrink and physical torn-page repair remain separate limitations.

Preserve persisted bytes and the current mapping when zero items are removed.
Reject excessive shrink requests before invalidating the live flush handle.

Preserve the owned mapping and checked flush barrier when destructor-bearing growth rejects arithmetic/layout overflow or a zero-growth callback unwinds.

Initialize destructor-bearing mapped values from the original file length rather than treating minimum-page padding as persisted application data.

Add independently selected causal version encoding, retained-sibling merge and checked counter construction with shared native/WASM/JS/C# vectors. These semantic operations perform no storage I/O or replication.

Add selected bounded local causal ownership over coherent atomic journals. Persist author counters, mutations and retry receipts together; retain historical contradictions and concurrent deletion evidence. Include portable shared vectors and SQL-free native directory interchange/process-exit examples. Merkle transport, projection and safe reclamation remain separate unfinished components.

Add optional causal-merkle snapshots, canonical root-bound proof frames and bounded differing-range pulls with owned continuations. Preserve concurrent destination evidence and deletion markers; include native filesystem wire exchange and process-exit examples without SQLite. Keep projection, membership reclamation and automatic balancing as separate unfinished work.

Add optional causal-batches over a coherent atomic journal store. Publish staged
owner records and bounded retry receipts together, retain original results after
lost replies and later edits, and reject stale bases or child-token collisions
without partial publication. Preserve the existing causal owner wire and bare
dependency boundary; include native SQL-free directory interchange examples.

Add separately selected owned causal row/name projection with explicit sibling
rejection or maximum-dot selection. Retain deletion markers and losing siblings,
validate complete supplied evidence before pruning, reject cross-row dot reuse,
and bound complete input independently of visible rows.

Reuse the standard query processor with a selected address type while retaining
the u32 facade. Add a checked full-u64 causal named/query adapter with a wider
wildcard marker, finite grammar/result/binding admission and lossless name output.

Reserve exact query allocations through the shared execution path on private
rows. Preserve owned identities across reference replanning and explicit ensures,
without staging allocation probes or publishing before explicit session commit.

- Upgrade doublets to 0.9.0, preserving split-store query indexes when forward
  references become live and filtering concrete target counts correctly.

- Validate legacy recycling queries on a detached copy of the actual allocator,
  including named allocations and duplicate merges. Reject missing references
  before writing data, names or journals, and keep transaction/history behavior.

Select permanent fresh-address blocks through a linearizable atomic journal key.
Retain exact author/token/count receipts after unknown replies and later grants,
reject collisions and exhaustion without publication, and never recycle abandoned
addresses. Bound replay and admission; preserve bare-store defaults. Include
shared Rust/C#/JS vectors, actual WASM contracts and physical/etcd examples.

Added optional explicit author takeover fencing over atomic journal providers.
Permanent owner-token receipts resolve lost replies without reviving replaced
owners. Guarded data publication compares the current fence in the same CAS;
ordinary manually managed providers retain their existing path.
Admit the canonical ASCII wire format before parsing so character budgets
cannot admit oversized UTF-8 records.

- Preserve checked query admission and allocation reservations through
  permission and undo sessions, retaining generic addresses and original-query
  envelopes with subscription change observations.
- Upgrade to doublets 0.21.0, links-notation 0.31.3 and current released
  protocol/storage dependencies.

### Fixed

- A mapped `grow` or `shrink` of values with destructors that hits a file quota
  or mapping limit now returns the error with its view still readable. It used
  to crash on the next read.
- `shrink` now aborts if a tail destructor panics. It used to drop those values
  a second time.

### Changed

- Update doublets to 0.21.0 and links-notation to 0.31.3.

### Fixed

- Names holding both quote kinds, backticks or a leading `#` now read back from
  LiNo output, because the notation library delimits them.

### Added

- Nested savepoints on engine sessions and cluster shards with `savepoint`,
  `rollback_to_savepoint` and `release_savepoint`. Rolling back restores staged
  rows only; reads stay validated, point-write ownership stays and allocated
  addresses stay retired. The WebAssembly session exposes them as `savepoint`,
  `rollbackTo` and `releaseSavepoint`.

### Added

- `clink --serve`, with or without `--graphql`, now stops cleanly on Ctrl-C and on termination signals: it finishes the current request, saves the store and exits with status 0. `GraphqlServer::shutdown_handle` stops a GraphQL server from another thread, as `LinksServer::shutdown_handle` already did.

### Added

- Optional `EngineOptions::lock_wait`: a pessimistic write waits a bounded time for
  another session's address and refuses a wait that would close a cycle. The JS
  client offers the same through `lockWaitMs`, on top of the new
  `StagedSession.writeOwner` and `takeWriteBlocker`.

### Added

- `Engine::backup_to` copies a consistent cut to an empty log while commits continue,
  flushes it and verifies it by reading it back. It can target any retained revision
  for a point-in-time copy. `Engine::export_checkpoint_at` returns the same records.
  The WebAssembly `StagedEngine` gains `backup` and an optional revision on
  `exportCheckpoint`.

### Fixed

- Rollback, undo-image restore and recovery replay no longer delete a row that shares its source and target with another row. They now restore the recorded row through the new `LinksStorage::restore_link`, which skips duplicate resolution on direct stores. A layered `NamedLinksStorage` view keeps its own checks, so a duplicate below such a view still merges.

### Fixed

- A GraphQL subject can now write a value on a link it has just created. `insert_links_one` with a nested `string`, `number` or `object` used to answer `Forbidden` under a permission policy, because the check on the owner of the value did not know which links the subject had created.

### Added

- `Engine::backup_incremental_to` appends to an earlier backup only the revisions it lacks, each as the rows that changed, and JavaScript gets `WasmEngine.backupIncremental`. It refuses a destination whose last revision is no longer retained or that is not a copy of this engine, and leaves it unchanged.

### Added

- `engine::restore_backup` copies an archived journal to an empty log up to a chosen revision, so an incremental chain reopens at any revision it holds and a copy can end just before damage, and JavaScript gets `WasmEngine.restoreBackup`. A refused restore leaves the destination unchanged.

### Added

- `Engine::compact_journal_after` compacts once a selected number of commits have landed since the last compaction, and the optional `BackgroundCompaction` calls it periodically on its own thread and reports the first failure. The WebAssembly engine exposes `compactJournalAfter`.

### Added

- `engine::TimedLog` and `engine::revision_at`: an optional log decorator that records when each revision was written, so `restore_backup` can restore to a time.

### Fixed

- An update naming only one part of a link, such as `((1: 1 1)) ((1: 2 *))`, keeps the stored value of the other part instead of writing the wildcard marker into the link.

### Added

- `CacheLinks` and `CacheOptions`: an optional decorator that keeps any store as a cache, with links that expire (`expire`, `persist`, `time_to_live`, `purge_expired`, a default time to live) and a capacity kept by evicting the least recently used link nothing else refers to.

### Added

- `engine::recovery`: `check_journal` reports the longest prefix of a journal that `Engine::open` would replay and the first damaged record; `recover_journal` picks the longest valid prefix among copies (the damaged primary, replicas, mirrors, a restored backup) and refuses copies that diverge; `repair_journal_file` rewrites a damaged file journal under the writer lock, first keeping the original bytes in `<path>.damaged-<n>`; `read_journal_file_lossy` reads a journal with invalid UTF-8 so the checksum reports it.

### Fixed

- An engine journal record whose checksum differs from the written one only in letter case (`B` for `b`, a single flipped bit) is now refused as damage instead of being replayed; only the exact lowercase checksum the encoder writes is accepted.

### Fixed

- `ReplicatedLog` refuses to open a primary when a replica holds the primary's whole log and more, which means the primary lost acknowledged commits (a truncated file, an old backup). Writing on top would fork history and drop those commits for good; the error names the replica to repair the primary from. Unreadable or divergent replicas are still ignored.

### Added

- `EngineTransaction::lock_for_update(id)` reads a link and reserves its address until the session ends, like SQL `SELECT ... FOR UPDATE`, in a pessimistic engine. Locking every row a decision reads refuses write skew, which write locks alone let commit. A missing address is reserved too, a changed link is refused with a conflict, and the call waits under `lock_wait` like a write. The WASM `StagedSession.lockForUpdate` exposes it to JavaScript. See `docs/ENGINE-PESSIMISTIC.md`.

### Changed

- Update `rustls-pki-types` to 1.16.0 and split the Rust files approaching the 1000-line limit, with no change in behaviour. CI now restores Cargo caches on every branch but saves them only from main, so a Windows test job no longer times out saving a cache, and false negatives in the link, secret, CodeQL and file-size checks are fixed (issue #172).

## [2.1.2] - 2026-11-06

Update doublets and links-notation, and split large permission, triplet and protocol modules without changing their public interfaces. Harden CI failure, security and release checks.

## [2.1.1] - 2026-11-05

Update links-notation to 0.30.1 while auditing the associative architecture roadmap.

Preserve quoted names in triplet imports and substitution queries, and export names without turning numeric names, wildcards or variables into query syntax.

## [2.1.0] - 2026-11-05

Added `clink --serve <address> --graphql`: a Hasura-compatible GraphQL server implementing the Deep Standard, so `DeepClient` from `@deep-foundation/deeplinks` and DeepCase connect to link-cli. See `docs/GRAPHQL.md`.

## [2.0.0] - 2026-11-05

A reference to an unreadable link needs its own right (#161). Without it an unreadable address answers exactly as a missing one, in the view, in operations and in queries, and the name of a link the subject may not refer to is an unknown name that cannot be taken over. A subject may refer to a link it may read, to itself, to what a rule `(r: AllowReference (g: Subject Object))` covers (`Object` and the links of type `Object`), and to the `Object` of its own `AllowRead` and `AllowWrite` rules. `AllowReference` does not allow reading; a reference a link already holds may stay; choosing the address of a new link is a reference; the write check runs first. Breaking: a subject that referred to an unreadable link needs a rule now, and a rule `Anything` to `Anything` brings the previous behaviour back. `PermissionMarkers` has a new field `allow_reference`, `PermissionAccess` a new variant `Reference`, `AccessPolicy` a method `can_reference` that defaults to `false`, and `Triplets` a method `reference_is_triplet`.

## [1.5.0] - 2026-11-05

Serve triplets over the Links Notation text and binary protocol (#152). `clink --serve --triplets` (`ServerOptions::triplets`) answers `(count: (i T f t))`, `(each: (i T f t))`, `(create: (T f t))`, `(update: (i T f t))`, `(delete: i)` and three-part substitution queries with `(i: T f t)` triplets, keeps names and `(auth: token)`, and leaves a server without the flag as it was. Subscriptions push triplet changes and permissions judge them through the triplets view of `PermissionsDecorator`. A binary client whose packet asks for `--arity 2..3` gets each triplet as one link of three references, `(i: (T f t))`, and every parser reads both forms. `RemoteLinks` implements `Triplets` and adds `subscribe_triplets` and `receive_triplet_notification`, `triplets_operations` holds the requests and replies, and `docs/protocol/triplets.txt` is the conversation both ports replay.

## [1.4.0] - 2026-11-05

Add the `Triplets` trait and `TripletsOverDoublets` (#151): triplets `(index: type from to)` stored in any `NamedTypeLinks` with an associative API, the twin of the C# `TripletsOverDoublets` with the same tests. A node `(i: T 0 0)` is the doublet `(i: T i)` and a link `(i: T f t)` is `(i: T ((Pair f) t))`, with the points `Pair` and `Nothing` found or created by name, so the type stays the source and `TypesDecorator` reads triplets as values of their types. `create_triplet`, `update_triplet` (the address stays), `delete_triplet` (cascading) and `each_triplet` / `count_triplets` by index, type, from and to leave no unused wrapper or pair behind. `process_triplets_query` runs substitution queries with three parts and `triplets_to_lino` / `import_triplets_lino` export and import `(index: type from to)` lines. The encoding and its costs are in `docs/TRIPLETS.md`; `cargo run --example triplets` walks through it.

## [1.3.0] - 2026-11-03

Add an optional permission layer with permissions stored in links (#149), the port of the C# one with the same tests, which replay those of `@link-foundation/ui-links`. A rule is a pair `(g: Subject Object)` marked by `(r: AllowRead g)` or `(r: AllowWrite g)`; `Anything` matches everything and write implies read. `PermissionPolicy` reads the rules from the data (`in_data`) or from a store of their own (`with_rules`), and `PermissionsDecorator` is the store as a subject sees it: reads skip what it may not read, and a create, update or delete it may not make fails with `Forbidden` (`Forbidden: update 14`) and changes nothing. A link the subject created is its own while its source is 0. `UndoableLinksDecorator` makes a request atomic, and `NamedTypeLinks` gains `reference_exists`, so references are checked in the store below the view.

`clink` gains `--permissions self|<rules.links>`, `--subject`, `--tokens`, `--grant-token`, `--revoke-token` and `--token`. `LinksSessions` (`with_permissions`, `with_subject`, `with_authenticator`) gives every connection of `LinksServer::serve_sessions` a session; `(auth: token)` replies `(subject: i)` through any authenticator callback or `LinksTokenTable`, a token table stored as links named after the SHA-256 of each token. Without `--permissions` nothing changes.

Add layers that compose with it and with each other, none depending on another. `UnionLinks` shows many links networks as one (a client's links joined with a server's, a cache with its source): the first member that has an address shadows the rest, a new link is created in the first member, and an update, a delete or a name goes to the member that has the link. `RemapLinks` shows a network under other addresses through an `AddressMap`: `OffsetAddressMap` moves it out of the way of another one, and `AddressTable` gives addresses in the order they are seen. `LinksExternalIds` binds the ids of other systems (UUIDs, ObjectIds, URLs) to links, in the data or in a store of its own, in the layout the C# and JavaScript ports share. `NamedTypeLinks` gains `claims`, so the undo of a request recreates a link in the member it came from, and `NamedLinksStorage` lets the transactions and version control layers sit over any of these views: a `PermissionsDecorator` over a union restricts every member at once, and one under it a single member.

Update `doublets` to 0.19.0 and `sha2` to 0.11.1, with the lock files.

## [1.2.0] - 2026-11-03

The LiNo protocol server now supports subscriptions (#150).

- `(subscribe: (index source target))` replies `(subscription: N)`.
- Later changes that other connections make to matching links are pushed as `(notify: N)` followed by their `(before) (after)` lines. Pushes and replies arrive in the order the store changed.
- `(unsubscribe: N)` replies nothing, and closing a connection drops its subscriptions.
- `LinksServer::with_readable` hides links from subscribers, so a link that becomes unreadable arrives as a deletion.
- `ServerOptions::max_pending_notifications` disconnects subscribers that fall too far behind.
- `LinksClient` sets pushes aside for `receive_notification`.
- `RemoteLinks` gains `subscribe`, `unsubscribe` and `receive_notification`.

Both ports replay the shared conversation in `docs/protocol/subscriptions.txt`.

Updated `doublets` to 0.18.0.

## [1.1.0] - 2026-11-02

Add the `Types` trait and `TypesDecorator` (#148): a type hierarchy `Type -> SubType -> … -> Value` with multiple inheritance kept in plain links, the port of the C# `TypesDecorator` with the same tests. The type of a link is its source, the root is `(Type: Type Type)`, a sub-type is `(S: T S)` and extra parents are pairs `(p: S T2)` marked by `(m: Extends p)`. `type_of`, `is_type`, `parents_of`, `ancestors_of`, `is_a` and `is_of` terminate on cycles, and `define`, `extend` and `instance` write. `NamedTypeLinks` gains `query_links`, which finds links by a pattern.

Update `doublets` to 0.17.0, `links-notation` to 0.30.0 and the WebAssembly `wasm-bindgen` family to their latest releases, with the lock files. With doublets 0.17 a cascading delete resets every usage to `(0 0)` before deleting it, as C# does; the net change reported for each link is unchanged.

## [1.0.9] - 2026-10-24

Expanded Rust query processor parity with the C# implementation, including unwrapped query pairs, structural and wildcard deletes, variable-driven updates, named composite rename behavior, direct Rust basis dependencies on doublets, links-notation, and lino-arguments without a direct clap dependency, and a doublet-backed Rust port of the C# Unicode string storage path with explicit Data.Doublets.Sequences-style converter abstractions.

Added `--out`/`--lino-output` database export support that writes the complete links database as LiNo with named references when available.

Added Rust `NamedTypes` and `NamedTypesDecorator` parity for link storage with external-reference names, name cleanup on delete, and unique name reassignment behavior.

Added strict validation for missing numeric and named link references, plus `--auto-create-missing-references` to create missing references as self-referential point links.

Added Rust pinned type decorator parity and composed pinned type access into the named types decorator.

Routed Rust query processing and CLI string ID aliases through the named types decorator so aliases resolve from the separate names database, matching C# full string ID behavior.

Added `--export` as an alias for `--out` database export.

Added `--in`/`--lino-input`/`--import` database import support for reading LiNo files into the links database with named references enabled by default.

Fixed issue 20 query parity by preserving unbound substitution parts from existing matched links and rejecting unsupported explicit creation addresses.

Added issue 62 regression coverage for reversible explicit updates, finite pinned types, unsupported placeholders, and stale named-link mappings.

Moved Rust release automation into `rust/scripts/`, added crates.io publication
checks, and relocated the WebAssembly wrapper crate under `rust/wasm/`.

Documented the dual `[lib]` + `[[bin]]` nature of the `link-cli` crate and
added crates.io metadata (`description`, `documentation`, `homepage`,
`keywords`, `categories`) so the public library surface is discoverable
from crates.io and docs.rs.

Added optional transactions and version-control layers (issue #94).
`transactions::TransactionsDecorator` records each Create/Update/Delete
as a reversible transition in a sidecar doublets store and exposes
`begin_transaction()` / `commit()` / `rollback()` plus three retention
policies (`infinite`, `sized:<n>`, `chunked:<n>:<dir>`) and two commit
modes (`sync`, `async`). `version_control::VersionControlDecorator`
adds branching, tagging, and time-travel checkout over the recorded
log. The `clink` binary surfaces both layers through `--transactions`,
`--transactions-file`, `--commit-mode`, `--retention`, `--log`,
`--vc`, `--vc-file`, `--branch`, `--branch-from`, `--checkout`,
`--tag`, `--list-branches`, and `--list-tags`. When no flag is passed,
behaviour is byte-identical to the existing CLI — no sidecar is
written and no extra cost is paid.

Hardened the Rust pipeline (issue #96). Every job now declares a
timeout and a least-privilege token, release jobs run in a
non-cancellable writer concurrency group, the advertised
`changelog-pr` release mode is implemented, and the lint job fails on
`Cargo.lock` drift. `anyhow` and `memmap2` were refreshed in both
lockfiles to clear the outstanding RUSTSEC advisories.
`cargo clippy` now runs with `-D warnings` and also covers the `rust/wasm`
workspace, which had never been formatted or linted, and pull requests
re-run the fast checks on a simulated merge with the tip of `main` so a
semantic merge conflict fails the pull request instead of `main`. The
pattern-matching helpers moved from `query_processor.rs` into
`query_processor/matching.rs` to clear the file-size warning; no public API
changed.

Refreshed every Rust dependency to its latest release (issue #98):
`doublets` 0.3.0 -> 0.4.0, `links-notation` 0.13.0 -> 0.16.1,
`thiserror` 1.0 -> 2.0.20 and `anyhow` -> 1.0.104 in the CLI workspace,
plus `wasm-bindgen` 0.2.127, `serde` 1.0.229, `serde_json` 1.0.151,
`web-sys` 0.3.104 and `wasm-bindgen-test` 0.3.77 in the WASM workspace.
The `doublets` bump matters beyond hygiene: an external crate that
already depends on `doublets` 0.4 can now link against `link-cli`
without pulling two semver-incompatible copies of the same store into
one binary.

Fixed opening a `transactions::FileTransitionLog` whose tail was left
half-written by a crash on Windows (issue #98). The torn tail is trimmed
with `File::set_len`, but Windows grants an append-only handle
`FILE_APPEND_DATA` without `FILE_WRITE_DATA`, so the call failed with
`ERROR_ACCESS_DENIED` and the log could not be reopened at all. The
repair now runs through a dedicated read/write handle, and the
append-only handle is opened afterwards.

Also replaced a `chunks_exact(2)` loop in the balanced variant converter
with `as_chunks::<2>()`, which the Rust 1.98 `clippy::chunks_exact_to_as_chunks`
lint rejects under `-D warnings`.

Made the Rust library reusable as a doublets-backed transactional store
(issue #98).

`storage::LinksStorage<T>` is the trait the transactions layer is now
written against, so any doublets-compatible store can be composed under
it. `storage::DoubletsStorage` implements it over a real
`doublets::unit::Store`: `DoubletsStorage::open` creates a file-mapped
store whose links are mutated **in place** (the inode never changes, so
other processes that already mapped the file keep seeing the same data),
and `DoubletsStorage::wrap` adopts a store the embedding application
already owns. The CLI's own text-file `LinkStorage` implements the same
trait, so nothing about `clink` changed.

`GenericTransactionsDecorator<T, S, L>` is generic over the doublets
address type, the wrapped store and the transitions log, so a consumer
with a `unit::Store<usize, _>` is a first-class user rather than being
pinned to `u32`; `TransactionsDecorator` remains the `u32` +
`NamedTypesDecorator` specialisation `clink` uses. The transition wire
format writes addresses in decimal and is therefore identical across
address types, and an address that does not fit the target type is
reported as `LinkError::AddressOutOfRange` instead of being truncated.
`transactions::FileTransitionLog` adds a plain append-only,
`fsync`-per-append log for consumers that do not want a second links
database; a torn tail left by a crash is discarded when it is reopened.

`storage::lock` adds advisory file locking over a `<database>.lock`
sidecar (shared for readers, exclusive for writers, blocking `acquire`
and non-blocking `try_acquire`), surfaced as
`DoubletsStorage::open_shared` / `open_exclusive` /
`try_open_exclusive`, and `LinksStorage::has_external_changes` answers
"has anyone else written since I last looked?" from a cheap
`StorageRevision` fingerprint. Durability is now stated explicitly:
writes to a file-mapped store survive a process crash without any
`save()`, while surviving a machine crash requires the `fsync` that
`LinksStorage::flush` performs; recovery replays committed-but-unapplied
transitions and rolls back uncommitted ones, and is covered by tests
that crash a file-backed store mid-rebuild.

The public storage and transactions APIs take `AsRef<Path>` rather than
`&str` and return the typed `LinkError` rather than `anyhow::Error`.

Building the crate now requires Rust 1.89, the release that stabilised
`std::fs::File::lock`.

Updated the `doublets` basis crate to 0.5.0 (both the `link-cli` and `clink-wasm` lockfiles), which brings the upstream `doublets::decorators` layer into reach for the Rust storage stack.

Reused the `doublets` 0.5.0 decorator layer in the Rust storage stack: `DoubletsStorage::map_store` composes any upstream (or custom) decorator onto an open database while keeping its path, advisory lock and change-detection fingerprint, and `DoubletsStorage::with_automatic_uniqueness_and_usages_resolution` applies the same stack C# gets from `ILinksExtensions.DecorateWithAutomaticUniquenessAndUsagesResolution`. The `doublets` crate and its `decorators` module are re-exported from `link_cli` so downstream crates can build custom stacks without a version-mismatched direct dependency.

Routed the CLI's `LinkStorage` through the upstream `doublets` uniqueness and cascade resolvers, so a delete now removes the links that referenced the deleted one and an update that would duplicate an existing link merges into it, matching the C# CLI. The transactions log records one transition per link a write actually touched — including the ones a cascade touched — so rollback and version-control branch switching no longer lose the cascaded changes, and the query processor restores links that a resolved write deleted as a side effect, mirroring `RestoreUnexpectedLinkDeletions` in the C# processor.

Fixed Rust name lookups losing links whose external reference collided with a `doublets` service constant. `LinkStorage` now reports the hybrid `LinksConstants`, which reserve the upper half of the address range for external references, and its inherent `get_or_create` no longer resolves to `Doublets::search` — which treats `any` as a wildcard — through the `Doublets` impl for `&mut LinkStorage`. Reserved pinned type names such as `Type` or `UnicodeSymbol` also resolve deterministically now that name holders are ordered by address instead of by hash map iteration order.

Added a Rust `PersistentTransformationDecorator` that ports the C# persistent transformation triggers, including the `Once`/`Always` trigger schema, the `<database>.triggers.links` sidecar store and the embedded store. Triggers written by either implementation are readable by the other.

Exposed the persistent transformation triggers on the Rust CLI: `--always`, `--once` and `--never` store or remove a trigger, `--triggers`/`--triggers-file` point at the trigger store and `--embed-triggers` keeps the triggers in the main database, matching the C# CLI.

Fixed address allocation in the Rust storage so that it matches the C# store: a freed address is reused before the store grows, the most recently freed one first, deleting the last link shrinks the store, and ensuring an address gives back the addresses passed over on the way to it. The free list is persisted so the order survives between CLI invocations.

Fixed `--changes` reporting in the Rust CLI: an auto-created reference is now reported as an update of the placeholder it started from, a delete reports the whole cascade of removed usages, and the reported changes are emitted in a reproducible order instead of one derived from the hash seed of the process.

Made every module of the `link_cli` library public, along with the query patterns, the resolved links, the link reference validator and the transition wire-format constants, so an alternative CLI or an embedding application can reuse and replace the same pieces `clink` is built from instead of reimplementing them.

Fixed unbound substitution variables and `*` in a substitution: they are now resolved at the write boundary the way C# resolves `Constants.Any`, so a created half becomes null, a lookup treats the half as a wildcard, and an update keeps the half already stored, instead of writing the literal `4294967295`.

Added `PersistentFileMapped::open_existing`, a safe constructor that adopts
the full element capacity represented by an existing file instead of starting
with a logical capacity of zero. This lets embedding applications reopen a
persisted `doublets::unit::Store` without an `unsafe` `grow_assumed` call.

Added LiNo substitution operations over TCP (issue #105): a dot-stuffed UTF-8 `TextLinoProtocol` and an interchangeable `BinaryLinoProtocol` whose packets are sections of links sharing one reference width (8, 16, 32 or 64 bits) and one arity range. Doublets with one uniform width are the default; external (Hybrid) references, any arity range such as `2..3` or `1..`, and packed per-section widths are options, and packed output is never larger than uniform output. The format is pinned by shared golden vectors in `docs/protocol/binary-links-notation-vectors.txt`. `LinksServer`/`LinksClient` and the new `clink --serve`/`--connect` options (`--protocol`, `--external-references`, `--arity`, `--packed-widths`) run create, read, update and delete queries over either protocol.

Upgraded `links-notation` to 0.22.0. In earlier versions, parse time grew exponentially with nesting depth, so one short message could stall a server.

Fixed the `--changes` report of creations:

- Every link a query creates is reported, named point links and leaves created on the way included. Each one is reported once, as `() ((name: name name))`, not as an empty link filled in later.
- Creating a link under an existing name redefines that link in place, and a new name for an existing doublet names it instead of copying it.
- Reference validation predicts the addresses new links get by asking the store. Stores reuse the address freed last first, so the lowest free address is not always next. A reference to a freed address the query does not refill is now reported missing instead of being dropped.

Added `RemoteLinks`, which implements `doublets::Links`, `doublets::Doublets` and `NamedTypeLinks` for links that live behind a `LinksServer`. Code written against those traits, the `QueryProcessor` included, switches from a local file to a server by swapping one value. `LinksServer` now also answers `LinksOperation` documents, one per interface call: `(count: …)`, `(each: …)`, `(create: …)`, `(update: …)`, `(delete: …)`, `(get-name: …)`, `(set-name: …)`, `(get-by-name: …)` and `(remove-name: …)`. These are the documents the C# `RemoteLinks` sends, and both test suites replay the same recorded conversation, `docs/protocol/links-operations.txt`. A write replies with the net change of every link it touched: the steps of a cascading delete differ between the C# and the Rust stores, the net changes do not.

Added `--export-binary <PATH>` (aliases `--binary-output`, `--binary-out`) and `--import-binary <PATH>` (aliases `--binary-input`, `--binary-in`), which write and read the whole store, names included, as a store archive in binary links notation: a links packet that keeps every address and hole, followed by a names packet. The library exposes the same as `protocol::export_store`, `protocol::import_store` and their `_file` variants. A binary archive is imported before the `--in` LiNo file and exported wherever `--out` is written. The archive bytes match the C# port.

Updated `thiserror` to 2.0.21 and refreshed `Cargo.lock`. A new `Dependencies` workflow fails while any dependency or lock file entry is behind its latest release, and Dependabot opens the update pull requests.

Fixed the WebAssembly build, which broke when `NamedTypeLinks` started requiring `update_observed` and `delete_observed`. The browser workbench now runs on the new `LinkStorage::in_memory`, the CLI's store without a file, instead of its own simplified copy. In the browser, deleting a link now also deletes its usages, and updating a link into an existing pair merges the two, exactly as in the CLI.

`protocol::packet::address_tier` now returns a `ProtocolResult` and fails as unencodable for an address beyond the internal range instead of silently returning width 8. A server that is stopping now answers a request it already read with `(error: "server is shutting down")` instead of hanging up, and its trace notes when a client hangs up. Malformed-operation errors print the offending link in LiNo instead of Rust `Debug` syntax, and the trailing-bytes error no longer depends on the byte count. Tests now cover every line of the protocol code: each malformed-packet test asserts the exact error, which fixed cases that passed for the wrong reason, and new tests cover misshapen `name`/`link` replies, archive file errors and a stopped server.

The binary links notation now comes from `links_notation::binary` (links-notation 0.23.0) instead of a copy in `link_cli::protocol` (issue #104). `link_cli::protocol` re-exports it, so `protocol::packet`, `LinksPacket`, `ArityRange`, `BinaryLinoOptions`, `DecodeLimits`, `encode_document`, `decode_document`, `format_document` and `parse_document` keep their paths. Breaking changes:

- `DecodeLimits::max_text_bytes` is gone: the text limit belongs to the transport, so `TextLinoProtocol { limits }` becomes `TextLinoProtocol { max_text_bytes }` (64 MiB by default). The new `ProtocolLimits { binary, max_text_bytes }` replaces `DecodeLimits` in `ServerOptions::limits`, `read_any_document` and `MessageFormat::protocol`.
- The upstream defaults apply: the default `max_depth` drops from 1024 to 64, the same depth the text parser accepts, and the new `max_string_bytes` limit (64 MiB) bounds the strings a packet expands to.
- `BinaryLinoProtocol` checks its limits when it encodes too, so it fails with `ProtocolError::Unencodable` instead of writing a packet its peer would reject.
- The re-exported codec (`encode_document`, `decode_document`, `parse_document`, `LinksPacket::from_bytes`, `LinksPacket::read_from` and the rest of `protocol::packet`) returns `BinaryError` instead of `ProtocolError`. Every protocol, server, client and archive function still returns `ProtocolError`, and `From<BinaryError> for ProtocolError` keeps the kind and message, so `?` keeps working.
- The upstream decoder validates more: a packet's link count and references are checked against the limits before any link is decoded.

Updated `links-notation` to 0.23.0. The `Dependencies` workflow now checks every manifest git tracks, and a dependency can be held back only by a comment on its manifest line that links an open issue.

### Changed
- Publish to crates.io through GitHub Actions trusted publishing (OIDC), falling back to the `CARGO_REGISTRY_TOKEN`/`CARGO_TOKEN` secret when the trusted publishing exchange is unavailable.

Mark the internal `clink-wasm` browser wrapper as `publish = false` so registry scans no longer treat it as an unpublished crates.io package. Its wasm-pack app build and GitHub Pages deployment remain available.

Update Rust dependencies and lockfiles to satisfy the latest-dependency CI gate, including doublets 0.8.0, links-notation 0.26.0, and lino-arguments 0.5.0. Verify the canonical lossless reference literals used by the newer Links Notation formatter while retaining coverage for legacy multiline text framing.

Add optional `IndexedLinksDecorator` lookup indexes, composed through
`LinkStorage::with_indexed_lookups`, `NamedTypesDecorator::with_indexed_lookups`
and the Rust CLI's `--indexed-lookups` flag. Named LiNo imports and exports use
persistent exact-pair/source/target indexes instead of repeated full-store
scans when enabled. Keep the original scan algorithms as the default for
comparison. Preserve Unicode names, duplicate resolution, cascading deletion,
address reuse and persisted readback. Add bounded scaling and parity tests.

Synchronize protocol and dependency documentation with the versions inherited
from main. Add compatibility tests for legacy quoted input and lossless CRLF
transport with links-notation 0.26. Binary version 1 encoding is unchanged.

Add `Clink.request(text)` to the WebAssembly wrapper: it executes a
LinksOperation or substitution document exactly like `clink --serve` and
returns the reply text, so `@link-foundation/link-cli-client` drives the
in-process store and remote servers through one contract (issue #108).

`LinkStorage::set_name` takes a name away from the link that had it before,
as the file-backed names store and the C# port do. Previously the old link
kept reporting the name, and removing that stale name unnamed the new link;
the WebAssembly store showed this through `get-name` (issue #108).

Saving a database now writes a new file next to it and moves it over the old one instead of truncating the file and rewriting it in place. Stopping `clink --serve` (Ctrl+C, a crash) during a write used to leave an empty or partial database and lose every link, the acknowledged ones included; it now leaves the last complete save. Nothing is synced to disk, so a power loss can still lose recent writes.

Update links-notation to 0.27.1 and refresh the native and WebAssembly lockfiles
to satisfy the repository's dependency freshness check.

Validate every LiNo import definition, including nested links, before changing links or names so invalid input leaves library stores unchanged.

Preserve quoted and encoded query references as names throughout validation,
creation, matching and mutation, including numeric names such as "0" and "1".
Bare numbers remain link addresses. Applies to the CLI, library and WASM wrapper.
Keep quoted names intact in stored triggers, and quote numeric names in LiNo
output so exported names round-trip through queries.

Fix the release pipeline so a Rust release always commits its version bump.
`rust/CHANGELOG.md` did not exist, so the release script's `git add` failed
silently, nothing was committed and every push to main re-published the same
changelog fragments as a new patch version (1.0.6, 1.0.7 and 1.0.8). The
script now creates `CHANGELOG.md`, consumes the fragments, syncs `Cargo.lock`,
verifies the release write, fails on any staging error, retries only lost push
races and tags the release only after its commit reached the remote.

crates.io trusted publishing now runs only when the
`CRATES_IO_TRUSTED_PUBLISHING` repository variable is `true`. Attempting it
unconditionally left a "No Trusted Publishing config found" error annotation on
every successful release that published with the token secret.

The crates.io, Docker Hub and GitHub release lookups that decide whether and
which version to release now retry network failures, 429 and 5xx with backoff
and fail with an error once their attempts run out. A failed lookup used to
count as "not published", so a single dropped connection re-released a
published version or bumped from a stale base.

Fix Auto Release compilation with warnings denied by importing the test-only
PathBuf in the test module. Compile every executable release helper in its
non-test release configuration on pull requests before it can reach main.
