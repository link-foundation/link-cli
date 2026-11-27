# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [5.4.0] - 2026-11-27

Added `--in-max-depth <N>` (1 to 128, default 64) to set the nesting limit of
LiNo imports through `--in`, `--import` and `--lino-input`, and
`LinoDatabaseInput.ImportText`/`ReadFromFile` overloads that take `maxDepth`.
LiNo import now accepts nested pairs as link references, as the Rust and
JavaScript implementations do. The 64-level default is now documented (#200).

## [5.3.1] - 2026-11-26

`clink --transactions` writes no longer slow down as history grows (issue #195). The CLI journal is now an append-only `<db>.transitions.log` file (`FileTransitionLog`): a write appends its transitions and markers instead of rewriting the whole transitions store. A bounded `--retention` policy also compacts the journal on disk, so the file stays bounded. An existing `<db>.transitions.links` journal is imported once. CI runs a benchmark that fails when write N+100 is much slower than write N on a constant-size store.

Also updates `links-notation` to 0.32.0.

## [5.3.0] - 2026-11-25

Concurrent `clink` runs on one store no longer interleave their changes (#194).
A run now locks its stores for its whole read-modify-write: writers
exclusively, read-only runs of an existing store shared (readers creating a new
store at once crashed each other). On Windows, which gives the mapped store to
one process at a time, readers lock exclusively too. A run that cannot get the lock within
`--lock-timeout <ms>` (default 30000) exits with a "store is locked" error that
names the owner. `--lock flock` is the default. It takes an OS lock on
`<db>.lock` (`flock` on Unix, `LockFileEx` on Windows), which the system
releases when the holder exits. `--lock pid` uses a `<db>.lock.d` directory
with an owner record (pid, host, PID namespace, start time), which a waiter
breaks once that owner process is gone. `--lock none` and `--no-lock` are for
callers that serialize access themselves. `--lock-status` prints who holds the
locks of a store. The Rust and JavaScript `clink` take the same locks, so mixed
runs exclude each other too. The library gains `StoreLock`,
`StoreLockOptions`, `StoreLockOwner`, `StoreFileLock` and `StorePidLock`.

Update `links-notation` to 0.32.0.

## [5.2.1] - 2026-11-23

Update the LiNo dependency to 0.31.8 using the renamed `links-notation` NuGet package.

## [5.2.0] - 2026-11-21

Add nested session and cluster-shard savepoints with `Savepoint`, `RollbackTo`
and `ReleaseSavepoint`. Rolling back restores staged rows only; reads stay
validated, point-write ownership stays and allocated addresses stay retired.

Select permanent fresh-address ranges through one linearizable atomic journal
key. Retain exact author/token/count receipts across lost replies and following
grants; refuse token collisions, exhausted domains and finite admission before
publication. Preserve caller-owned providers and existing manual reservations.

Select immutable sorted runs independently of concurrency, isolation, validation,
retention and journal encoding. Share immutable run arrays between revisions,
preserve tombstones through merges and retain the existing links/session API.

Add selected causal version encoding, retained-sibling merge and checked counter
construction with shared Rust, actual-WASM and JS vectors. These semantic
operations perform no storage I/O, durable author fencing or replication.

Select bounded local causal batches over one coherent journal store. Publish
the complete owner journal and separately bound retry receipts with one two-key
compare-exchange. Reject stale bases, child-token collisions and invalid later
edits before publication; discover exact retries after lost replies and later
writes. Preserve standalone owner compatibility and caller-owned store lifetime.

Select owned causal row/name projection with explicit conflict rejection or
maximum-dot selection. Preserve complete retained siblings including deletions,
validate complete input before pruning, and bound version count and wire bytes.

Stage ordinary causal links and names through the existing private engine,
with a fresh caller-reserved address range and selected constraints. Freeze
one complete final difference before explicit-token batch publication; preserve
exact retries after unknown replies and following writes. Expose the standard
ulong links/name surface and owned prepared requests for process recovery.

Preserve the current mapped view and owned file handle when capacity growth is
rejected. Construct a replacement before publishing its pointer, retaining
flush, retry, following edits and exact used-length disposal after an I/O error.

Reject complete corrupt journal entries and report replay failures. Add explicit
data/journal flush callbacks and checkpoint only after data reaches storage.

Add an optional staged links engine with configurable concurrency, isolation,
immutable revision retention, constraints and replaceable file/replicated logs.

Support independent journal branches and atomic three-way merge, with saved
merge bases for reopen/retry after uncertain I/O outcomes.

Compose atomic multi-shard transactions with replaceable journal stores and an
optional etcd HTTP adapter, with bounded I/O, linearizable snapshots, generation
validation, unknown outcomes and actual three-member failover integration.

Route immutable journals to independent data stores while atomically publishing
small catalog pointers; preserve cross-shard validation and unknown-outcome
discovery across independent consensus groups.

Add optional portable delta journal decorators with bounded checkpoint chains,
exact logical replay, corruption rejection and uncertain-write fencing.
Avoid unnecessary recovered-row conflict metadata and skip per-row generation
tracking for exclusive/whole-revision serializable strategies. Reclaim generations
after the final session validates, preserving optimistic ABA/branch conflicts.

Select serializable point-read validation independently of journal/history settings.
Permit disjoint point edits while checking missing rows, ABA, branch changes and
read/write cycles; scans retain conservative revision validation.

Preserve storage outcomes when optional store, query, server, transaction,
trigger or version console diagnostics fail. Retention no longer replays
rolled-back writes or archives them as committed changes; background retention
waits for pending transactions.

Retain observed partial writes when storage or user callbacks throw, undo
automatic operations and preserve explicit rollback state. Keep the original
absent state across repeated allocation/update callbacks.

Release owned data handles and partially initialized names mappings when a
constructor fails; leave borrowed data usable after failed construction.

Retain the complete observed undo batch before journal writes. Reject commit and
further writes after an append failure, roll back automatic failures, and expose
Rollback() on the decorator to retry cleanup when an implicit rollback fails.

Make the transaction rollback-only before undo starts, preserving the abort
decision when undo or rollback persistence fails until cleanup is retried.

Release CLI-owned mapped data, journal, version and trigger stores before the
entry point returns, including failure while constructing a later layer.

Reject exhausted legacy transaction sequence space before mutation and retain
complete pending undo when a cascade exceeds remaining sequence capacity.

Wait for active asynchronous checkpoints before transaction shutdown returns,
serialize concurrent shutdown callers and reject callback self-joins. Preserve
borrowed-store ownership and journal recovery for cancelled/failed checkpoints.

Select optional pessimistic point-write ownership with immediate busy errors,
independent isolation/read-set validation, stale/ABA rejection, branch protection
and cleanup on every session outcome. Keep engine-local locks out of cluster
workspaces and preserve existing optimistic/exclusive defaults.

Dispose complete facade graphs with borrowers before their dependencies, joining
asynchronous checkpoints before mapped stores close. Share nested/cyclic traversal
state, preserve detached references, and leave dependencies available when borrower
cleanup fails and must be retried.

Keep version-control admission and owner cleanup retry after failed transaction
rollback. Release admission when the inner transaction reaches a terminal state,
including a failure reported after rollback completes.

Finish committed version publication after checkpoint or metadata errors through
transaction/owner disposal and rollback. Keep write and metadata admission fenced
until publication succeeds, and avoid duplicate transition markers on retry.

Publish the final signed version sequence without wrapping. Initialize version
history from retained journal records, avoiding phantom versions across gaps.

Persist branch and parent names with an unambiguous versioned marker, retaining
legacy reads. Recover tag names containing equals signs without truncation.

Fence named writes while committed version publication is pending. Reject
public journal or version recovery while an open transaction owns live state.

Retain known committed synchronous checkpoints through failed persistence.
Retry them through Commit, CommitAsync, Rollback or transaction disposal before
releasing foreground admission or publishing version metadata. Fence row and
named writes during incomplete commit/rollback cleanup; preserve queued async
policy and borrowed-store shutdown semantics.

Recover the ordered committed suffix after an applied-marker gap. Persist the
complete redo and pending undo before closing that gap, so interrupted recovery
cannot overwrite later committed updates, deletions or recreated rows.

Checkpoint current inline effects when sized/chunked retention encounters an
older failed async checkpoint; preserve newer updates, deletion and recreation.

Retain uncertain commit attempts before marker append/flush, fence new writes
and resolve the decision through cleanup without undoing a possibly durable
commit. Avoid duplicate decision markers on retry and retain version publication
ownership when cleanup resolves an uncertain inner commit.

Exclude aborted and recovered pending records from initial version history and
its branch head. Reject existing sidecars that attribute a known uncommitted
record, and reject version construction/recovery over a live inner transaction.

Retain an implicit version owner for automatic writes through failed commit and
metadata publication. Retry committed work through cleanup, preserve ordinary
pre-commit undo (including failed undo retry), and fence all new mutations until
the original branch publication finishes. Preserve no-op and async policies.

Wait asynchronously for background checkpoint work instead of occupying a
ThreadPool thread for every idle transaction owner. Apply admitted batches in
order without another pool dispatch, while preserving cancellation, actual
worker joining and callback self-join rejection during shutdown.

Persist a version-publication intent before journal commit and durably publish
the exact branch/range before clearing it. Recover committed or aborted intents
after abrupt process exit, retain admission through every metadata/flush failure,
and reject malformed or contradictory intents without rewriting their sidecar.
Expose the metadata flush callback and durable-publication capability.

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
Retain exclusive replay ownership through row, metadata and flush failures;
finish exact positions on reopen, fence queued asynchronous checkpoints and
preserve retry after failed startup construction. Reject contradictory movement
metadata and retain sequence allocation floors after journal payload eviction.

Claim chunk archive paths with exclusive creation, preserving existing archives
and symlink targets. Keep rejected optional archives in the live journal for a
later retention attempt, using a fresh random candidate.

Validate recovery journals before replacing live history or replaying rows.
Reject conflicting sequence records, contradictory decisions and invalid owners
or sequences while preserving idempotent parsed duplicates and retained floors.
Retain admission after failed recovery, including queued asynchronous checkpoints
and retention, until retry completes recovery before following writes or replay.

Construct branch paths iteratively without stack growth per ancestor, scanning
transition attribution once. Preserve nested fork cutoffs, per-branch heads and
root-to-leaf replay order, including empty histories and subsequent edits.

Commit caller-selected groups of independent sessions with separate revisions
and one shared durability flush. Preserve input-order validation, known aborts
and indeterminate prefix recovery without publishing uncertain state. Avoid
flushing the primary twice during synchronous replica acknowledgement.

Record ordinary name changes and row-associated names in complete portable
journal records. Restore unique-name owners and deleted/cascaded names during
rollback; publish name-only versions and preserve names through recovery,
movement intents and chunk archives. Retain the original row transition and
v1 codec, add complete record snapshots/replay, preserve shared-name adapters
and distinguish an existing name owner at address zero from an absent name.

Select optional pre-effect logical row/name undo images for ordinary transactions.
Retain preparation and cleanup ownership, validate mandatory metadata before
mutation and reconcile unreported effects before committed history/publication.
Preserve the default observer policy and original constructor signatures; recover
unknown allocations and redo later committed checkpoints after image restoration.

Select structurally shared immutable ordered engine state independently of
isolation, history and journal settings. Preserve flat-map defaults and portable
revision records; share unchanged nodes across live and recovered revisions,
scans, group preparation and branches without copying complete row maps.

Select canonical changed-row revision encoding independently of state structure
and other engine policies. Preserve complete-snapshot defaults, mixed journal
recovery, saved branch bases, shared group flush and physical delta composition.
Apply selected change records against immutable predecessor roots and validate
base, allocator, row/name operations and checksums before publication.

Select an optional provider-neutral SQLite physical page-WAL journal. Preserve
staged reads, atomic group flushes and uncertain outcomes; fence stale owners,
validate schema/UTF-8/sequences, enforce WAL/FULL and expose bounded checkpoints.
Keep the core free of a mandatory SQLite NuGet provider and bound explicit
provider command retries.

Select optional persistent constraint indexes independently of map, encoding and
other engine policies. Allocate only selected pair/name/reference indexes, allow
atomic swaps, count incoming references and publish roots only after flush.
Preserve scan defaults, historical replay validation and uncertain group outcomes.

Select a bounded immutable journal object cache for partitioned stores. Keep
catalog reads and publication checks fresh, return owned arrays and retain no
cache by default. Coordinate concurrent fills and payload accounting locally.

Reject cyclic Unicode sequence graphs through a checked iterative walker while
retaining the maintained package converter, repeated shared subtrees and existing
UTF-16 behavior. Keep traversal state local after failed decoding and reject
malformed persisted names on repeated reopening.

Format deep link structures iteratively with one StringBuilder. Preserve
source-only expansion, cycle references and deepest-first name lookups while
avoiding call-stack exhaustion and repeated whole-subtree copying.

Treat the .NET 10 listener socket-clear race as a clean accept-loop exit only
after server shutdown starts. Preserve unexpected null-reference failures
outside shutdown and join accepted connection workers before returning.

Select a SQL-free native doublets page journal independently of logical engine
and replication strategies. Validate canonical versioned pages and decisions,
stage group writes, fence uncertain owners and reject committed corruption.
Provide an exclusive native file device with bounded reads, durable flush,
POSIX parent synchronization and explicit close-error ownership fencing.

Select two-copy native page repair separately from revision, validation and
replication policies. Synchronize both copies before acknowledgement and validate
the entire expected prefix before repairing a damaged copy. Preserve the default
single-copy format and costs, and retain uncertain native ownership fences.

Accept equals signs in native command tag names while preserving aliases to
complete existing tags, unknown-point diagnostics and following database edits.

Select optional portable retained-history checkpoints and coordinated checkpoint
publication through the existing generation CAS. Preserve append-only defaults,
committed allocators, selected history and uncertain-outcome recovery.
Compose initial checkpoints with physical delta journals and either replica
decorator order while preserving exact replay and uncertain-operation fencing.

Provide optional complete-journal replacement through two fixed page banks and
an epoch-bound synchronized root. Keep devices caller-owned, reject pending
physical records and fence uncertain operations until actual reopening.

Compact retained committed journals under the engine commit lock through an
optional replacement log. Preserve sessions, pins, history and allocator gaps;
compose physical delta journals and fence unknown publication until reopening.

Own the native checkpoint root and both single/mirrored banks through a selected
factory. Acquire all fences before recovery, close every raw owner root last,
preserve constructor and cleanup errors, and retain uncertain fences without
flushing pending work during disposal.

Select checkpoint root mirroring independently of bank copies. Retain the
original constructor signature and single-root defaults. Validate root copies
and their committed bank before repair, synchronize both decisions before
acknowledgment and reject persisted mode conversion without changing existing
bytes. Acquire and clean up every selected native root and bank owner.

Add optional namespace maintenance with complete bounded revision-consistent
etcd scans and generation-conditional single-key deletion. Preserve ordinary
atomic-store costs and leave immutable-object reclamation independently selected.

Select explicit epoch-fenced immutable journal reclamation with complete bounded
root discovery, exact-generation deletion, coherent owned-reader refresh and
zero-default optional caching. Keep append-only modes and expose native/WASM
retained-history checkpoint handoffs and deterministic concurrency fault cases.

Provide explicitly selected immutable physical sorted journal tables over native
doublets pages, with exact tombstone generations, bounded full validation and
indexed physical point reads. Verify portable vectors and native cross-language
file interchange; keep WAL/manifest/compaction composition separate.

Add optional native-lsm1 semantic WAL replay and revalidated checkpoint encoding
with shared independent vectors, canonical u64 descriptors and finite admission.
Keep physical table validation, publication and native ownership composition as
separate required work; this codec performs no storage I/O.

Compose an explicitly selected bounded physical journal store from an atomic
replaceable WAL and exclusive immutable-table namespace. Preserve group CAS,
tombstone generations, bounded buffering, foreground compaction and maintenance
consumers; fence uncertain publication and defer table repairs until complete
selected-state validation. Native directory ownership remains separately selected
and requires its own read-only discovery before constructing a repairing WAL.

Add borrowed read-only checkpoint inspection in Rust, C# and JavaScript. Return
owned selected-prefix records with finite page admission, preserving suffixes and
staged mirror repairs until higher-level semantics validate; allocate no identity,
initialize no bank and leave ordinary constructor recovery behavior intact.

Provide a separately imported JavaScript native LSM facade with exclusive
bounded directory discovery, read-only validation before recovery effects,
independent file mirrors, and explicit uncertain-handle cleanup fencing.

Provide the C# native LSM directory facade with matching bounded read-only
discovery, independent file mirrors, regular-file validation and cached
uncertain-handle cleanup. Add finite file-operation failures, abrupt-exit
recovery and actual direct, partitioned and reclaiming cluster examples.

Select bounded immutable journal chunks independently of caching and engine
policies. Reuse unchanged complete prefixes while retaining whole-journal
defaults, atomic catalog generation checks, owned reads and fresh full-content
staging comparisons. Validate every manifest before fetching data and every
update before staging. Preserve native Rust/C#/JS file interchange; keep the
epoch-reclamation format separate and reject unsupported roots before deletion.

Select bounded journal chunks independently on the epoch-fenced reclaiming store.
Protect every live reference during collection, restage complete objects after a
fence, and refresh the complete reader group if a chunk retires. Preserve blob2
writing by default and interoperable native file/consensus consumers.

Add selected local causal ownership over atomic journals, durable retry receipts,
retained historical identities and bounded replay, with SQL-free native LSM
interchange and explicit process-exit recovery examples.

Add selected immutable causal Merkle snapshots, canonical root-bound proof frames
and bounded differing-range pulls with opaque continuations. Preserve concurrent
destination evidence and validate physical cross-runtime/process-exit exchange.

Share the established query matching and mutation algorithms across selected
unsigned generic addresses while preserving the original uint facade, options,
patterns and implicit query strings. Keep quoted numeric names distinct from
full-width stored addresses. Preserve concrete zero bindings during swaps,
equality and joins. Expose owned read-only callback parts without invoking the
upstream default-constant reflection helpers that predate UInt128. The checked
causal adapter, finite query admission and monotonic allocation preparation
remain separate required work under CSHARP-QUERY-PARITY-PLAN.md.

Prepare monotonic engine queries over detached rows and names using the shared
processor. Reserve exact FIFO allocation identities under the owner gate,
without creating/deleting probe rows or invoking real observers. Keep failed
preview admission free of cursor changes, retain already reserved gaps after
later reference rejection or rollback, and consume only after write ownership
is acquired. Allocate reservation state lazily. Add compatible optional named
store capabilities and direct explicit ensure, preserving legacy recycling
prediction and explicit custom creator delegates. Guard full-width ensure
regressions with a finite allocation cap instead of walking the u64 domain.

Expose the ordinary selected-address processor through CausalSession.QueryLinks.
Use UInt128 controls with checked u64 stored parts and the caller's independent
fresh range, including retained-identity resurrection. Freeze every retained
adapter operation at preparation/disposal. Admit original bytes, parser depth,
nodes, streamed result/binding expansion and decoded named change replies on
owned preview state before real edits or callbacks. Share the existing matching,
mutation, reservation and explicit-token batch implementations; retain the uint
facade and opt-in limits for existing stores. Keep anonymous creation callbacks
compatible with the pinned upstream helper while avoiding its UInt128 list
initializer. Verify shared query vectors, bounded refusals and exact lost-reply
retries; physical query interchange remains required work.

Select unsigned addresses in the existing operation/restriction, server and
remote-store implementations, retaining public uint facades. Add an explicit
original-query envelope and QueryLossless without mutation fallback retries.
Admit decoded documents before recursive formatting/tracing and clamp checked
text/binary decoding before canonicalization. Convert parser/admission failures
to protocol errors; count the final decoded text byte. Keep callback rows owned
and read-only, preserve private staging and preparation freezes, and verify
shared query vectors across all codecs. Standard operation address checks
precede edits; finite limits retain broad-read refusals.

Validate legacy owned mapped queries on detached data/names allocator images,
including explicit/named allocations and duplicate merges. Keep refused queries
free of provider and journal writes, close retained preview interfaces safely,
and forward through existing transaction/version write fences. Preserve caller
and derived-store allocator fallbacks instead of replacing their behavior.

Add separately selected author takeover fencing with permanent claim receipts
and atomic fence comparisons alongside ordinary data publications. Retrying a
superseded owner token cannot reclaim ownership; inner providers stay caller-owned.
Admit canonical ASCII ownership records before JSON parsing so the declared
byte budget also bounds malformed UTF-8 records.

Preserve checked query admission and allocation reservation capabilities through
permission and undo sessions. Keep original-query envelopes observable by
subscriptions.

Update Platform.Data.Doublets to 1.0.0 and Link.Foundation.Links.Notation to 0.31.1.

Add optional `EngineOptions.LockWait`: a pessimistic write waits a bounded time for
another session's address and refuses a wait that would close a cycle.

Add `LinksEngine.Backup`, which copies a consistent cut to an empty commit log while
commits continue, flushes it and verifies it by reading it back. It can target any
retained revision for a point-in-time copy. `ExportCheckpoint(revision)` returns the
same records.

A GraphQL subject can write a value on a link it has just created. `insert_links_one` with a nested
value used to answer `Forbidden` under a permission policy, because the check on the value's owner
built a new permissions view that did not know what the subject had created.

`LinksEngine.BackupIncremental` appends to an earlier backup only the revisions it lacks, each as
the rows that changed. It refuses a destination whose last revision is no longer retained or that is
not a copy of this engine, and leaves it unchanged.

`LinksEngine.RestoreBackup` copies an archived journal to an empty log up to a chosen revision, so
an incremental chain reopens at any revision it holds and a copy can end just before damage. A
refused restore leaves the destination unchanged.

A permissions view now answers a write to an absent link with `Link not found`, the same answer a
hidden link gets, instead of passing it to the memory store, whose unchecked read could end the process. A write at
the null link gets that answer too, instead of skipping the permission checks.

`LinksEngine.CompactJournalAfter` compacts once a selected number of commits have landed, and the
optional `BackgroundCompaction<T>` calls it periodically on its own thread and reports the first failure.

Fixed: stopping the GraphQL server can no longer crash the process. A stop arriving between the accept loop's check and its accept raised an unhandled error on a background thread.

Added `TimedCommitLog` and `TimedCommitLog.RevisionAt`: an optional log decorator that records when each revision was
written, so `LinksEngine.RestoreBackup` can restore to a time.

Fixed: a stopped `--serve` or GraphQL server no longer leaves its interrupt handler registered in the process.

An update naming only one part of a link, such as `((1: 1 1)) ((1: 2 *))`, keeps the stored value of the other part instead of merging the link into another one.

A served database now stops gracefully on `SIGTERM` as well as on Ctrl+C, as the Rust server already did, instead of being killed with status 143.

Added: `CacheLinksDecorator` keeps a store as a cache: links can expire (`Expire`, `Persist`, `TimeToLive`, a default time to live and `PurgeExpired`), and `MaxLinks` keeps the store to a size by evicting the links used least recently that nothing else uses, refusing the write when every link is in use. See `docs/ENGINE-CACHE.md`.

Added: `EngineJournal<T>` checks and repairs a damaged engine journal. `Check` reports the longest prefix that `LinksEngine` would replay and the first damaged record; `Recover` picks the longest valid prefix among the journal and its replicas, mirrors or backups and refuses copies that disagree; `RepairFile` keeps the original bytes in `<path>.damaged-<n>`, writes the repaired journal beside it, flushes it and renames it over the path, and refuses to run while a writer holds the journal lock. See `docs/ENGINE-RECOVERY.md`.

Fixed: an engine journal record whose checksum differs from the written one only in letter case (`B` for `b`, a single flipped bit) is now refused as damage instead of being replayed; only the exact lowercase checksum the encoder writes is accepted.

Fixed: `ReplicatedCommitLog.Read` refuses a primary when a replica holds the primary's whole log and more, which means the primary lost acknowledged commits (a truncated file, an old backup); `LinksEngine` no longer opens and writes on top of it, which would fork history and drop those commits for good. The error names the replica to repair the primary from. Unreadable or divergent replicas are still ignored. The JS `ReplicatedCommitLog.read()` and the Rust `ReplicatedLog` behave the same way.

Added: `EngineSession<T>.LockForUpdate(id)` reads a link and reserves its address until the session ends, like SQL `SELECT ... FOR UPDATE`, in a pessimistic engine. Locking every row a decision reads refuses write skew, which write locks alone let commit; a missing address is reserved too, a changed link is refused with a conflict, and the call waits under `LockWait` like a write. The JS `EngineClient.lockForUpdate(index)` and the Rust `EngineTransaction::lock_for_update` behave the same way. See `docs/ENGINE-PESSIMISTIC.md`.

C# CI jobs now start only after the JavaScript stage (lint, `npm run test:js`, parity check) passes for the same commit. The C# suite loads the shared golden fixtures in `parity/golden-fixtures.json` and CI checks its receipt (issue #111).

The release job now pushes its version commit with the release deploy key, because `main` requires the three stage checks. When that push is declined, the error says how to restore the key.

The `publish-current` dispatch on `main` passes the same JavaScript and .NET checks as an automatic release and can recover an unpublished NuGet package at the current version.

A System V IPC peer that reads the end of a stream and removes both queues before its acknowledgement is read no longer fails the end with "the peer hung up": a send that finds the peer gone while sending the end ends the stream, as a hang-up does.

Update Platform.Data.Doublets to 0.18.10 and Link.Foundation.Links.Notation to 0.31.0; harden CI checks, hosted runner recovery and release authentication diagnostics.

Update System.CommandLine to 3.0.0, Microsoft.Data.Sqlite to 11.0.0 and Microsoft.NET.Test.Sdk to 18.11.1; split the C# files approaching the 1000-line limit and fix false positives and negatives in CI checks (issue #172).

Update Microsoft.NET.Test.Sdk to 18.12.0 (issue #176).

Keep examples and experiments out of NuGet, verify the package output, recover both unpublished packages before versioning again, and update the coverage collector.

Carry the LiNo text and binary protocols over every IPC mechanism of
IPC transports v1 (`docs/protocol/ipc-transports-v1.md`) (#140): TCP and UDP
loopback, Unix domain sockets, Windows named pipes, POSIX FIFOs, anonymous pipes
(`stdio` and `exec:`), shared memory, memory-mapped files, files on disk, OS
signals, POSIX and System V message queues, mailslots, `WM_COPYDATA` and ALPC.
`Foundation.Data.Doublets.Cli.Ipc` selects the mechanism by an endpoint string
(`IpcTransports.Connect`, `Listen`, `ListenAll`, `Relay`, `Register`),
`LinksServer.ListenAll` and `LinksClient.ConnectEndpoint` take any endpoint, and
the transports interoperate with the Rust `clink` and the JavaScript client.
`clink --serve` repeats to serve one store on several endpoints, `--forward`
relays every connection to another endpoint without opening a database so
transports stack, `--connect` takes any endpoint and `--trace` (or
`CLINK_IPC_TRACE=1`) logs every transport event. The tests use
Microsoft.NET.Test.Sdk 18.12.0.

Preserve quoted names in triplet imports and substitution queries, and export names without turning numeric names, wildcards or variables into query syntax.

## [5.1.0] - 2026-11-05

Serve Hasura-compatible GraphQL for Deep with `clink --serve <address> --graphql`, with `--jwt-secret`, `--graphql-admin-secret` and `--deep-package`; add `TripletsQueryProcessor.MatchQuery` (#153).

## [5.0.0] - 2026-11-05

A reference to an unreadable link needs its own right (#161). Without it an unreadable address answers exactly as a missing one, in the view, in operations and in queries, and the name of a link the subject may not refer to is an unknown name that cannot be taken over. A subject may refer to a link it may read, to itself, to what a rule `(r: AllowReference (g: Subject Object))` covers (`Object` and the links of type `Object`), and to the `Object` of its own `AllowRead` and `AllowWrite` rules. `AllowReference` does not allow reading; a reference a link already holds may stay; choosing the address of a new link is a reference; the write check runs first. Breaking: a subject that referred to an unreadable link needs a rule now, and a rule `Anything` to `Anything` brings the previous behaviour back. `PermissionMarkers<T>` has a fourth parameter `AllowReference`, `PermissionAccess` a new value `Reference`, `IPermissionPolicy<T>` a method `CanReference` that defaults to `false`, and `ITriplets<T>` a method `ReferenceIsTriplet`. `AdvancedMixedQueryProcessor.Options.References` and the `references` parameter of `LinksServer.ExecuteRequest` are gone, as a query checks its references through the store it runs on; a store may implement the new `IReferencedLinks<T>` to answer them.

## [4.5.0] - 2026-11-05

Serve triplets over the Links Notation text and binary protocol (#152). `clink --serve --triplets` (`LinksServerOptions.Triplets`) answers `(count: (i T f t))`, `(each: (i T f t))`, `(create: (T f t))`, `(update: (i T f t))`, `(delete: i)` and three-part substitution queries with `(i: T f t)` triplets, keeps names and `(auth: token)`, and leaves a server without the flag as it was. Subscriptions push triplet changes and permissions judge them through the new `TripletsPermissionsDecorator`. A binary client whose packet asks for `--arity 2..3` gets each triplet as one link of three references, `(i: (T f t))`, and every parser reads both forms. `RemoteLinks` implements `ITriplets<uint>` and adds `SubscribeTriplets` and `ReceiveTripletNotification`, `TripletsOperation` holds the requests and replies, and `docs/protocol/triplets.txt` is the conversation both ports replay.

## [4.4.0] - 2026-11-05

Add `TripletsOverDoublets<TLinkAddress>` and `ITriplets<TLinkAddress>` (#151): triplets `(index: type from to)` stored in any doublets store with an associative API. A node `(i: T 0 0)` is the doublet `(i: T i)` and a link `(i: T f t)` is `(i: T ((Pair f) t))`, with the points `Pair` and `Nothing` found or created by name, so the type stays the source and `TypesDecorator` reads triplets as values of their types. `CreateTriplet`, `UpdateTriplet` (the address stays), `DeleteTriplet` (cascading) and `EachTriplet` / `CountTriplets` by index, type, from and to leave no unused wrapper or pair behind. `TripletsQueryProcessor.ProcessQuery` runs substitution queries with three parts, such as `((($r: knows alice bob)) (($r: knows alice carol)))`, and `TripletsLino` imports and exports `(index: type from to)` lines. The encoding and its costs are in `docs/TRIPLETS.md`.

Also updates `Platform.Data.Doublets` to 0.18.8.

## [4.3.0] - 2026-11-03

Add an optional permission layer with permissions stored in links (#149), the port of `createPolicy` and `withPermissions` of `@link-foundation/ui-links` with its tests. A rule is a pair `(g: Subject Object)` marked by `(r: AllowRead g)` or `(r: AllowWrite g)`; `Anything` matches everything and write implies read. `PermissionPolicy<TLinkAddress>` reads the rules from the data or from a store of their own, and `PermissionsDecorator<TLinkAddress>` is the store as a subject sees it: reads skip what it may not read, and a create, update or delete it may not make throws `ForbiddenException` (`Forbidden: update 14`) and changes nothing. A link the subject created is its own while its source is 0. `UndoableLinksDecorator<TLinkAddress>` makes a request atomic, and the query processor checks references in the store below the view (`Options.References`).

`clink` gains `--permissions self|<rules.links>`, `--subject`, `--tokens`, `--grant-token`, `--revoke-token` and `--token`. A server with `LinksServerOptions.Permissions` gives every connection a session (`LinksSessions`); `(auth: token)` replies `(subject: i)` through `LinksServerOptions.Authenticate`, any callback, or `LinksTokenTable`, a token table stored as links named after the SHA-256 of each token. Without `--permissions` nothing changes.

The dependency `Platform.Data.Doublets` is updated to 0.18.7.

Add layers that compose with it and with each other, none depending on another. `UnionLinksDecorator<TLinkAddress>` shows many links networks as one (a client's links joined with a server's, a cache with its source): the first member that has an address shadows the rest, a new link is created in the first member, and an update, a delete or a name goes to the member that has the link. `RemapLinksDecorator<TLinkAddress>` shows a network under other addresses through an `IAddressMap<TLinkAddress>`: `OffsetAddressMap<TLinkAddress>` moves it out of the way of another one, and `AddressTable<TLinkAddress>` gives addresses in the order they are seen. `LinksExternalIds` binds the ids of other systems (UUIDs, ObjectIds, URLs) to links, in the data or in a store of its own, in the layout the Rust and JavaScript ports share. `IComposedLinks<TLinkAddress>` (`Claims`, `EnsureCreated`) lets the undo of a request recreate a link in the member it came from, so a `PermissionsDecorator<TLinkAddress>` over a union restricts every member at once and one under it a single member.

## [4.2.0] - 2026-11-03

Added subscriptions to the LiNo protocol server (#150). `(subscribe: (index source target))` answers
`(subscription: N)`, and every later change another connection makes to a link matching the pattern is pushed to the
subscriber as `(notify: N)` followed by its `(before) (after)` changes; `(unsubscribe: N)` answers nothing, and closing
the connection drops its subscriptions. `LinksServerOptions.Readable` hides links from subscribers (a link that becomes
unreadable arrives as a deletion) and `LinksServerOptions.MaxPendingNotifications` disconnects a subscriber that stops
reading. `LinksClient` sets pushes aside for `ReceiveNotification`, and `RemoteLinks` gains `Subscribe`, `Unsubscribe`
and `ReceiveNotification`.

Updated Platform.Data.Doublets to 0.18.7.

## [4.1.0] - 2026-11-02

Add `TypesDecorator<TLinkAddress>` and `ITypes<TLinkAddress>` (#148): a type hierarchy `Type -> SubType -> … -> Value` with multiple inheritance kept in plain links. The type of a link is its source, the root is `(Type: Type Type)`, a sub-type is `(S: T S)` and extra parents are pairs `(p: S T2)` marked by `(m: Extends p)`. `TypeOf`, `IsType`, `ParentsOf`, `AncestorsOf`, `IsA` and `IsOf` terminate on cycles, and `Define`, `Extend` and `Instance` write. The decorator is not sealed, and pinned types are sub-types of its root.

Update Platform.Data.Doublets to 0.18.6 and System.CommandLine to 2.0.14. Counting `(* point)` now visits a self-referencing point once. Since 0.18.6 a merging update also ends by reporting the link it merged into, unchanged; `LinksUniquenessAndUsagesRepointingResolver` does not pass that notice on, so `clink --changes` and the protocol server still report only the links a write changed, as the Rust port does (creating a duplicate replies with no change).

## [4.0.6] - 2026-10-24

Harden the C# release automation (issue #134): the release commit is rebased
onto the latest main before any file is touched, a push that loses the race to
a concurrent release is rebased and retried while a repository-rule rejection
fails fast with an explicit error, the tag is created only after the commit
reached main and exactly that tag is pushed, git runs without a shell so
release descriptions are passed verbatim, and CHANGELOG.md no longer gains an
extra blank line on every release.

The NuGet and GitHub release lookups that decide whether to release now retry
network failures, 429 and 5xx with backoff and report the network cause, so a
single dropped connection no longer fails the release.

A changeset whose bump type cannot be parsed now fails the release with an
"Invalid changeset" error naming the file. It used to be skipped with a
warning and then deleted, so its bump and description were silently lost.

`LinksServer` no longer crashes the process when it shuts down right after
accepting a connection. The worker used to get its stream while `Shutdown`
disposed the client, and `TcpClient` throws `ArgumentNullException` when that
happens midway; the stream is now taken in the accept loop, under the lock
`Shutdown` holds.

## [4.0.5] - 2026-10-24

Preserve quoted query references as names, including numeric names such as "0"
and "1", throughout reference validation, creation, matching and mutation. Bare
numbers remain link addresses. Encoded reference literals also resolve as names.
Stored triggers preserve this distinction when normalized and replayed, and
numeric names exported as LiNo can be replayed through queries.

## [4.0.4] - 2026-10-24

Parse and validate the whole LiNo input before changing links or names, so malformed imports leave the database unchanged through both the CLI aliases and the library APIs.

## [4.0.3] - 2026-10-24

Preserve literal backslashes and multiline quoted names during LiNo import. Quote
backslash-containing names on export, and use the existing UTF-8 hexadecimal
reference literals for control characters and mixed quotes so user text survives
import/export/import into a fresh database.

## [4.0.2] - 2026-10-15

Synchronize protocol documentation with links-notation 0.26 inherited from main.
Verify legacy quoted input and lossless CRLF transport. Binary version 1
encoding is unchanged.

## [4.0.1] - 2026-10-15

Update Links Notation, System.CommandLine, and test dependencies to satisfy the latest-dependency CI gate. Verify canonical lossless reference literals while retaining coverage for legacy multiline text framing.

Quote numeric names in LiNo text exports and preserve quoted references as names on import, preventing numeric names from being confused with link addresses.

Handle a socket disconnected before its connection worker starts so server shutdown cannot crash the process.

## [4.0.0] - 2026-10-07

The binary links notation codec now comes from `Link.Foundation.Links.Notation.Binary` (links-notation 0.23.0) instead of a local copy. `ArityRange`, `LinksPacket`, `Section`, `PacketReference`, `DecodeLimits`, `LinoMapping` and `LinoFormat` are removed from `Foundation.Data.Doublets.Cli.Protocol`; import them from `Link.Foundation.Links.Notation.Binary`. `LinoStreamReader` is replaced by `PacketReader`.

- `DecodeLimits.MaxTextBytes` moves to `TextLinoProtocol.MaxTextBytes`. The new `ProtocolLimits` record (`Binary`, `MaxTextBytes`, `Default`, `Unlimited`) is now the type taken by `LinksServerOptions.Limits`, `MessageFormat.Protocol` and `LinoProtocols.ReadAnyDocument`.
- `BinaryLinoProtocol` now enforces `Limits` when it encodes too. The upstream defaults apply, so the default `MaxDepth` is 64 and there is a new `MaxStringBytes` limit.
- The protocols still raise only `LinoProtocolException`. A codec `BinaryNotationException` is converted with the same kind and detail and is kept as `InnerException`, and `LinoProtocolException.From` exposes this conversion.

## [3.2.0] - 2026-10-05

Fixed the `--changes` report of creations:

- Every link a query creates is reported, named point links and leaves created on the way included. Each one is reported once, as `() ((name: name name))`, not as an empty link filled in later.
- Creating a link under an existing name redefines that link in place, and a new name for an existing doublet names it instead of copying it.
- Reference validation predicts the addresses new links get by asking the store. Stores reuse the address freed last first, so the lowest free address is not always next. A reference to a freed address the query does not refill is now reported missing instead of being dropped.
- `EnsureCreated` no longer throws when the store hands out a freed address above the target before reaching it.

A query that refers to a missing link or does not parse is reported in one `Error: Query error: ...` or `Error: Parse error: ...` line on stderr, as the Rust clink reports it, instead of an unhandled exception with its stack trace. `--trace` still prints the stack trace.

Updated `System.CommandLine` to 2.0.12, and the test dependencies `Microsoft.NET.Test.Sdk` to 18.10.1 and `coverlet.collector` to 10.1.0. A new `Dependencies` workflow fails while any dependency is behind its latest release, and Dependabot opens the update pull requests.

Added LiNo substitution operations over TCP (issue #105). There are two protocols, and either can replace the other:

- `TextLinoProtocol`: dot-stuffed UTF-8 messages.
- `BinaryLinoProtocol`: each packet is a list of sections of links that share one reference width (8, 16, 32 or 64 bits) and one arity range. The default is doublets with one uniform width. Options add external (Hybrid) references, any arity range such as `2..3` or `1..` (`ArityRange`), and packed per-section widths; packed output is never larger than uniform output. Shared golden vectors in `docs/protocol/binary-links-notation-vectors.txt` pin the format.

`LinksServer` and `LinksClient` run create, read, update and delete queries over either protocol. The new `clink --serve` and `--connect` options do the same from the command line, together with `--protocol`, `--external-references`, `--arity` and `--packed-widths`. The wire format is byte-for-byte the same as the Rust port's, so C# and Rust servers and clients work with each other.

Upgraded Link.Foundation.Links.Notation to 0.22.0. In earlier versions, parse time grew exponentially with nesting depth, so one short message could stall a server.

An update into an existing doublet merges into it and now re-points every link that used the merged-away address at the surviving one, as the Rust port does. Before, the `MergeUsages` of Platform.Data.Doublets 0.18.1 ([Data.Doublets#515](https://github.com/linksplatform/Data.Doublets/issues/515)) blanked the half of the usage instead, so `() ((1 2) (2 1))` followed by `((1: 1 2)) ((1: 2 1))` left `(2: 2 0)` rather than `(2: 2 2)`. Stores are composed with the new `DecorateWithAutomaticUniquenessAndUsagesRepointing()`, whose top layer, `LinksUniquenessAndUsagesRepointingResolver`, replaces `LinksCascadeUniquenessAndUsagesResolver`.

`LinksPacket.AddressTier` now throws an unencodable `LinoProtocolException` for an address beyond the internal range instead of silently returning width 8, and the unused `LinoFormat.IsReference` was removed. A server that is stopping now answers a request it already read with `(error: 'server is shutting down')` instead of hanging up, and its trace notes when a client hangs up. `LinksPacket.WidthFromCode` is total over two-bit codes, and the accept loop no longer keeps a separate branch for a client accepted while stopping (its worker closes it). Tests now cover every line of the protocol code, and each malformed-packet test asserts the exact error, which fixed three cases that passed for the wrong reason.

Added `RemoteLinks`, an `INamedTypesLinks<uint>` whose links live behind a `LinksServer`. Code written against the links interface, the query processor included, switches from a local file to a server by swapping one value. `LinksServer` now also answers `LinksOperation` documents, one per interface call: `(count: …)`, `(each: …)`, `(create: …)`, `(update: …)`, `(delete: …)`, `(get-name: …)`, `(set-name: …)`, `(get-by-name: …)` and `(remove-name: …)`. These are the documents the Rust `RemoteLinks` sends, so either port's client works with either port's server. Both test suites replay the same recorded conversation, `docs/protocol/links-operations.txt`, to prove it. A write replies with the net change of every link it touched.

Fixed a crash of a `LinksServer` shut down right after it accepted a connection: the connection thread set an option on the socket the shutdown had already disposed, and the `NullReferenceException` ended the process.

Added `--export-binary <PATH>` (aliases `--binary-output`, `--binary-out`) and `--import-binary <PATH>` (aliases `--binary-input`, `--binary-in`), which write and read the whole store, names included, as a store archive in binary links notation: a links packet that keeps every address and hole, followed by a names packet. The library exposes the same as `StoreArchive.Export`, `StoreArchive.Import`, `StoreArchive.ExportToFile` and `StoreArchive.ImportFromFile`. A binary archive is imported before the `--in` LiNo file and exported wherever `--out` is written. The archive bytes match the Rust port.

## [3.1.0] - 2026-08-29

Opened the library up for extension: every decorator (`NamedTypesDecorator`, `NamedLinksDecorator`, `SimpleLinksDecorator`, `PinnedTypesDecorator`, `TransactionsDecorator`, `VersionControlDecorator`, `PersistentTransformationDecorator`) is now unsealed with overridable members, disposable ones follow the `protected virtual void Dispose(bool)` pattern so a subclass can release resources of its own, and `PersistentTransformationDecorator.PersistentTransformationQuery` and `InternalNamePrefix` are public. A custom CLI can now subclass any layer of the stack instead of forking it.

## [3.0.0] - 2026-08-29

Refreshed every C# dependency to its latest release and retargeted the
packages to `net10.0` (issue #98). `Link.Foundation.Links.Notation`
moves 0.13.0 -> 0.16.1 so the C# and Rust implementations parse LiNo
with the same version of the same grammar; that release only ships a
`net10.0` assembly, so `Directory.Build.props` and all three projects
now target `net10.0` and CI provisions the .NET 10 SDK. `System.CommandLine`
moves 2.0.7 -> 2.0.11, and the test project picks up
`Microsoft.NET.Test.Sdk` 18.9.0, `xunit.runner.visualstudio` 4.0.0 and
`coverlet.collector` 10.0.1.

This is a breaking change for consumers still on `net8.0`: upgrade to
the .NET 10 SDK/runtime before taking this release.

Made the C# transactions layer reusable from outside the CLI, matching
what the Rust library now offers (issue #98).

`TransactionsDecorator` is generic over the doublets address type:
`TransactionsDecorator<TLinkAddress>` works over any
`INamedTypesLinks<TLinkAddress>` whose address is an
`IUnsignedNumber<TLinkAddress>`, so a consumer with a `ulong`-addressed
store is a first-class user rather than being pinned to `uint`. The
non-generic `TransactionsDecorator` remains as a `uint` specialisation,
so existing code that constructs it keeps compiling. The transitions
wire format is unchanged and address-type independent — addresses are
written in decimal under the invariant culture, so a log written by a
`uint`-addressed store reads back unchanged in a `ulong`-addressed one,
and an address that does not fit the target type is rejected instead of
being silently truncated.

New `LinksFileLock` and `StorageRevision` cover multi-process access:
advisory locking of a database's `.lock` sidecar (shared for readers,
exclusive for writers, with a blocking `Acquire` and a non-blocking
`TryAcquire`) and a cheap "has anyone else written since I last looked?"
fingerprint. The lock file path and the shared/exclusive semantics match
the Rust `storage::lock` module, so the two implementations can guard the
same database.

Source-breaking: `Transition`, `ITransaction` and `ITransactionsLinks`
are now generic. Existing `uint` code should use `Transition<uint>`,
`ITransaction<uint>` and `ITransactionsLinks<uint>`.

## [2.6.0] - 2026-08-18

Added optional transactions and version-control layers (issue #94). The
new `TransactionsDecorator` records each Create/Update/Delete as a
reversible transition in a sidecar doublets store and exposes
`BeginTransaction()` / `Commit()` / `Rollback()` plus three retention
policies (`infinite`, `sized:<n>`, `chunked:<n>:<dir>`) and two commit
modes (`sync`, `async`). The new `VersionControlDecorator` adds
branching, tagging, and time-travel checkout over that log. The CLI
surfaces both layers through `--transactions`, `--transactions-file`,
`--commit-mode`, `--retention`, `--log`, `--vc`, `--vc-file`,
`--branch`, `--branch-from`, `--checkout`, `--tag`, `--list-branches`,
and `--list-tags`. When no flag is passed, behaviour is byte-identical
to the existing CLI — no sidecar is written and no extra cost is paid.

Hardened the C# build and the pipelines around it (issue #96).
`Directory.Build.props` now turns warnings into errors and enables the
.NET analyzers, and `TransactionsDecorator` / `VersionControlDecorator`
implement `IDisposable` so the memory-mapped databases they own are
released deterministically — the leak that made the Windows test job
fail while the pipeline still reported success. The C# workflow no
longer masks those Windows failures with `continue-on-error`, verifies
formatting and file sizes, and finally implements the `changeset-pr`
release mode it had been advertising without handling.
Pull requests also re-run the build and tests on a simulated merge with
the tip of `main`, and the coverage upload moved to
`codecov/codecov-action@v7` to stop the Node.js 20 deprecation warning.

## [2.5.0] - 2026-05-15

Split the C# distribution into two NuGet packages so external .NET
projects can consume the public library without pulling in the
`dotnet tool` packaging:

- `clink` — unchanged dotnet tool, now built from a CLI csproj that only
  contains `Program.cs` and `System.CommandLine` wiring.
- `Foundation.Data.Doublets.Cli` — new library package that ships the
  parser, query processors (basic / advanced / mixed), `ChangesSimplifier`,
  named/pinned type decorators, persistent transformation trigger
  decorator, LiNo I/O adapters, the `UnicodeStringStorage` extension, and
  every other reusable building block. Generated XML doc comments are
  packed alongside the assembly and rendered into a DocFX site published
  to GitHub Pages.

## [2.4.0] - 2026-05-12

Added `--export` as an alias for `--out` database export.

Added `--in`/`--lino-input`/`--import` database import support for reading LiNo files into the links database with named references enabled by default.

Added `--out`/`--lino-output` database export support that writes the complete links database as LiNo with named references when available.

Added a universal `NamedTypesDecorator` that implements both links operations and named type lookups, with automatic cleanup and uniqueness checks for external-reference names.

Added binary links-backed persistent transformation triggers with `--always`, `--once`, `--never`, `--triggers-file`, and `--embed-triggers`.

Added `IPinnedTypes` and `PinnedTypesDecorator`, and composed pinned type support into `NamedTypesDecorator`.

Fixed self-link substitution with outgoing links by preserving unbound substitution parts from the matched link and rejecting unsupported link addresses during explicit creation.

Fixed explicit indexed numeric updates so auto-created numeric references do not steal the substitution pair, and added issue 62 regression coverage.

Moved C# release automation into `csharp/scripts/` and packaged the C# README
with the NuGet tool.

Added full string ID alias support for advanced LiNo queries through the named types decorator.

Updated the C# LiNo parser dependency to the current `Link.Foundation.Links.Notation` package and refreshed supported NuGet package versions.

Added strict validation for missing numeric and named link references, plus `--auto-create-missing-references` to create missing references as self-referential point links.
