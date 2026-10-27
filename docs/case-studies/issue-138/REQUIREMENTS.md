# Acceptance inventory

Generic Rust initialization (R1/R6/R7/R8) now uses the original file length
and successful-growth boundary for destructor-bearing values. Three finite
regressions fail against immutable 9bb985f and current pre-fix 40d850f:
fresh/cross-page growth skips requested values, and a raw callback reports page
padding as initialized application data. All byte patterns in those controls
are valid. Both feature configurations now preserve the persisted prefix,
following growth, checked flush and exact destructor counts. Complete checks
finish 2026-10-27T12:19:05Z with 1,197 native tests, six doctests, 723 actual-WASM
tests and 221 repository checks passing. Complete native coverage reports
17,168/18,051 lines and 3,294/3,782 branches; complete linked WASM reports
13,144/15,760 and 2,663/3,171. All production denominators remain included.
RUST-GENERIC-INITIALIZATION-PLAN.md and mapped-generic-initialization{,-before}.txt
preserve commands and limits. Genuine generic I/O/partial-shrink failure,
callback/destructor panic and arbitrary persisted representation adoption remain
unfinished. Fresh CI and every wider unmet criterion remain required.

All nine workflows pass on published 3cd89e9, including native Rust on
Windows/Linux/macOS, full coverage, native LSM consumers, merge, lint and
packaging, and 2,462 C# tests per platform. growth-admission-ci-after.txt
preserves exact head, timestamps, run URLs and actual platform result lines.
This includes generic growth admission and retaining all three complete C#
reports. Later logical initialization/causal changes require fresh CI.
The earlier intermittent Windows timeout's host cause remains unestablished,
and every wider gap remains open.

C# coverage retention (R8) now publishes the existing complete Windows and
macOS reports as distinct artifacts alongside the established Linux artifact.
The immutable successful dec6cb7 run has only the Linux report; the artifact
verifier independently rejects that missing evidence. All 221 repository
checks and strict workflow checks pass locally. Fresh exact-head 3cd89e9 C#
CI passes 2,462 tests on every platform; the verifier downloads all three real
reports and verifies their sizes and SHA256 digests against GitHub. Complete
lines/branches are Linux 7,270/7,471 and 5,643/6,105; Windows 7,256/7,471 and
5,623/6,105; macOS 7,265/7,471 and 5,638/6,105. This changes evidence retention
without claiming higher coverage. Complete project 100% remains unfinished.
See CSHARP-PLATFORM-COVERAGE-PLAN.md and csharp-platform-coverage{,-before}.txt.

Generic Rust growth admission (R1/R6/R7/R8) now rejects arithmetic/layout
overflow before invalidating the live descriptor and handles zero growth
directly against its existing prefix. Three finite regressions first fail
against immutable 9bb985f; ordinary zero growth already passes. Both feature
configurations now preserve pointer, prefix, physical length, checked flush,
following growth and exact destructor counts. Complete sequential validation
finishes 2026-10-27T11:50:17Z with 1,194 native tests, six doctests, 723
actual-WASM tests and 221 repository checks passing. All production coverage
denominators remain included; mapped-growth-admission{,-before}.txt retains
full metrics and commands. Genuine generic resize I/O failures, nonzero
callback panic, failed partial shrink and broader representation safety remain
separate open audits. Every wider unmet criterion remains unfinished, fresh
platform CI is required, and PR139 stays draft and unmerged.

C# version-exit setup progress (R1/R3/R7/R8) now reports returned creation
before its update starts, retaining the public helper's callback and duplicate
semantics. An immutable baseline fails the boundary test and passes the
independent behavior control. All 31 focused controls, 2,462 tests in each
full Debug/Release suite and 221 repository checks pass at 11:22:31Z. Existing
tracing is enabled only in the child; idle/phase/capture/reaping bounds remain
unchanged. version-setup{,-before}.txt preserves evidence. All nine workflows
pass for prior dec6cb7, including 2,460 C# tests per platform before this
visibility correction; the original timeout is intermittent and its exact
host cause remains unestablished. mapped-grow-ci-after.txt preserves SHA and
timestamps. Later empty-shrink/setup corrections require fresh exact-head CI.
Every wider unmet criterion remains unfinished; PR139 stays draft/unmerged.

Zero-removal and rejected-shrink boundaries (R1/R6/R7/R8) now preserve Rust's
live mapped owner. Four finite regressions fail against immutable 2478d59;
complete shrink and destructor controls already pass. Returning for zero
removal preserves persisted items before adoption, physical length, pointer
and checked flush admission. Excessive removal retains the existing panic
before invalidating the descriptor. Both feature configurations pass all six
controls; full validation finishes 2026-10-27T11:01:26Z with 1,190 native tests,
six doctests, 723 actual-WASM tests and 221 repository checks passing. See
RUST-EMPTY-MAPPING-PLAN.md and mapped-empty{,-before}.txt for actual coverage,
commands and limits. Genuine failed partial shrink, destructor-bearing growth,
physical repair, full native/C#/linked-WASM 100% and every other wider unmet
criterion remain unfinished. Fresh platform CI is required; PR139 stays draft
and unmerged.

No required sub-issues were returned by GitHub's sub-issue API. Initial issue
comments and all three PR comment/review endpoints were empty.

PR feedback added after that initial inventory requests recovery of an interrupted
session and reporting the interruption to Codex/OpenAI. The preserved ancestry
commit has been recovered; validation continues under DEEP-ANCESTRY-PLAN.md.
The reporting request is fulfilled by the linked public report in
[SESSION-RECOVERY.md](SESSION-RECOVERY.md). That request does not change or
complete the original R1–R12 acceptance criteria. A second recovery/reporting
request and the explicit requirement for optional SQLite with native doublets
implementations are recorded in SESSION-RECOVERY.md. Native physical storage
gaps remain unfinished even when an optional SQLite configuration passes.

| ID | Issue requirement | Solution and verification plan | Status |
| --- | --- | --- | --- |
| R1 | Fully test ACID readiness | Failure injection at journal append/flush, data checkpoint, rollback and recovery; anomaly schedules and bounded concurrent histories; report exact persistence assumptions. | In progress: durability, corrupt recovery, Windows mapped flush, short filesystem reads, and cross-language lexical regressions reproduced and corrected. C# whole-graph disposal joins borrowers before closing mapped dependencies and preserves stores after cleanup failure. Real process-exit recovery probes pass; power-loss and distributed fault coverage remain incomplete. |
| R2 | Configurable concurrency strategies | Exclusive writer admission, optimistic validation and optional fail-fast pessimistic point writes; read committed, snapshot isolation and whole-revision or point-read serializable validation. Same transaction/storage API. | Implemented in Rust, C#, and Rust-backed JS/WASM; visibility, dirty writes, write conflicts, scan/write-skew validation, admission release, bounded concurrent writer tests, optional point-read tracking and independent finite serial-history oracles. Additional competitor strategies remain within R5's wider scope. |
| R3 | Configurable version control | Existing branches/tags plus optional no-history, bounded and complete immutable revision history; pin active snapshots independently of retention. | Retention and independent journal branches with atomic three-way merge implemented in all language surfaces. Tests cover conflicts, repeated/incremental merges, constraints, exclusive admission, unknown flush outcomes, allocator recovery and reopening from a saved merge base after history eviction. The legacy C# version owner retains admission and cleanup retry across failed rollback; 24 phase/entry-point regressions and a terminal retention control verify mapped recovery. Committed version cleanup retries publication after checkpoint or metadata faults without undoing durable data; 48 mapped phase/entry-point cases verify fencing, exact recovery and idempotent markers. Rust generic and version owners now retain known-commit checkpoints and reject subsequent writes/recovery until retry completes; 108 shared fault schedules and eleven real-filesystem version tests verify it. C# fences pending named version writes and both native languages reject recovery over a live owner. C# now retains known synchronous checkpoint ownership through failed persistence; 96 new phase/entry-point cases verify retries and five write fences. Sixteen additional C# and six real-filesystem Rust regressions cover named writes during failed cleanup. All fifty C# version-publication cases now also require checkpoint completion. Noncontiguous recovery now replays the committed suffix in order and retains its oldest gap until redo and pending undo are durable. Thirty-nine mapped C# and six portable Rust contracts cover real async gaps and interrupted recovery; six more C# regressions verify sized/chunked retention checkpoints current effects without stale redo. Full native/C#/actual-WASM suites pass. Uncertain append/first-flush attempts now retain commit ownership, fence writes and resolve through cleanup without duplicate decisions. Ninety-six C# cases, 88 shared Rust fault schedules and eleven filesystem version functions verify reread failures, repeated cleanup, publication retry and following edits. Automatic version writes now retain an implicit publication owner through failed decisions, checkpoints and metadata writes. Thirty-two C# cases and eleven Rust functions first fail; repeated cleanup, all existing write fences, exact native reopening and following commits now pass. Eighteen C# pre-commit controls verify ordinary/failed undo, an async control verifies queued completion, and four Rust controls also execute on actual WASM. Persisted publication intents now recover exact branch attribution across abrupt exit; six real child cases per native language and before/after failure matrices pass focused tests. Full suites pass (672 Rust, 1,440 C# per configuration, 356 actual-WASM); the ten Windows byte-snapshot sharing violations are corrected and all nine workflows pass on 1f742c5. Three new regressions per native language reproduce silent skipping of missing retained history. Both implementations now preflight all changed undo/redo before mutating rows; no-op and available suffix controls, following edits, native reopening and actual-WASM contracts pass. Full suites pass (675 Rust, 1,443 C# per configuration, 359 actual-WASM). Final signed version attribution terminates without overflow, and native initial histories use actual committed records rather than inventing gap or aborted versions; finite before/after tests verify checkout and persistent reopening. Nine additional cases per native language exclude explicit/recovered aborts from initial heads and reachable paths, reject older sidecars that attribute known uncommitted records, and reject construction over an open inner owner. Native reopening and actual-WASM contracts pass; invalid files are preserved without claiming to repair previously altered data. Shared versioned branch-name markers and final-delimiter tag parsing preserve accepted delimiter/Unicode/NUL/newline names, nested ancestry and checkout across reopening; shared malformed/legacy vectors verify compatibility. Existing branch/tag behavior remains available. |
| R4 | Replaceable distributed synchronization decorators | Common log contract, local-only, asynchronous catch-up and synchronous replica acknowledgements; explicit unknown outcomes, prefix validation and retry tests. | Fixed-primary decorators implemented in all languages. Partition/append/flush failures, divergent prefixes and idempotent catch-up tested. The optional atomic journal store and etcd JSON gateway now coordinate named shards; actual three-member leader loss, majority loss and lost-response recovery are tested. Independently selected epoch-fenced reclamation adds conditional deletion and coherent owned-reader refresh; its dedicated plan records verification. etcd owns leadership and membership. |
| R5 | Study competitor source and best strategies | Primary documentation and source survey covering SQLite, PostgreSQL, RocksDB, Redis, etcd and TiKV; costs/guarantee comparison and reusable component evaluation. | In progress; expanded primary-source audit and implemented independent-session group commit distinguish InnoDB flush sharing, MongoDB durable acknowledgement, Cassandra ordinary quorum/repair and current Accord routing; see RESEARCH.md |
| R6 | Same API / costs only for selected guarantees | Keep existing bare-store cache path; add optional control/history/journal/replication composition with explicit complexity and allocation costs. | Existing raw interfaces implemented by staged sessions; optional journals, delta encoding and replica decorators supplied. Bare stores incur no engine overhead. Delta encoding reduces physical bytes for localized edits. Independently selected shared immutable rows retain unchanged paths in candidates, scans and history; independently selected changed-row encoding avoids full serialization for localized edits; independently selected persistent constraint indexes avoid full validation scans for small edits, allocating only for selected constraints; default scan costs remain explicit. An independently selected immutable-object cache avoids repeat data downloads while retaining fresh catalog reads and complete staging comparisons; zero retains no cache. Serializable read-set validation is independently selected and tracks only observed addresses; scans retain revision checks. Conflict metadata is skipped for exclusive/optimistic whole-revision serializable modes, allocated lazily for overlapping sessions that need it, and reclaimed after the last validation. Optional point-write ownership allocates only for selected sessions/addresses, releases on every terminal path and rejects use in uncoordinated cluster workspaces. Independently selected immutable sorted runs share array roots and merge effective edits, with explicit scan-materialization and carry costs. Finer independent validation/history decorator composition remains to study. |
| R7 | Implement all supported languages | Native Rust and C# library surfaces, portable JS/TS surface and declared backend capabilities; shared wire vectors and scenario tests. | Staged engine surfaces implemented. Compiled WASM conformance passes Node, Bun and Deno; shared fixtures are consumed by Rust, C# and actual WASM. Ordinary legacy transactions/history now execute in actual WASM using portable timestamps; shared fault/recovery contracts verify checkpoint ownership. Newly added distributed strategies must maintain parity. |
| R8 | 100% test coverage, edge cases, unit + integration guarantees | Measure coverage by module/language; add deterministic faults, concurrent schedules, real process/filesystem checks; retain coverage artifacts and disclose uncovered branches. A test count is not coverage. | JS production modules measure 100% lines, branches and functions; CI enforces all three thresholds. Actual WASM wrapper source reaches 100% lines, functions and reported branches in 723 actual-WASM tests, including ten registered wrapper unit controls; complete linked LLVM regions/instantiations remain below 100%. Complete linked coverage measures 13,144/15,760 lines and 2,663/3,171 branches and remains below 100%. Native Rust/C# project coverage remains below the required scope; VERIFICATION.md retains complete denominators. |
| R9 | Document each scenario | Runnable cache, durable store and replicated/sharded examples with failure semantics, ownership and persistence settings. | docs/ENGINE-STRATEGIES.md, docs/ENGINE-CLUSTERS.md and Rust/C#/JS examples cover implemented options, including consensus-backed shard groups. Further deployment strategies need their own recipes. |
| R10 | Horizontal cluster, shared and shared-nothing options | TCP/advisory shared stores, independent shard namespaces, replicated logs and consensus-backed atomic shard groups. | Shared coordination and immutable journals on independently routed data groups implemented in all languages. Real leader failover, data-group outage, atomic pointer publication and native/WASM interoperability verified. Optional bounded immutable-object caching avoids repeated data downloads and supports previously verified reads during a data-group outage; catalog availability remains required. Selected epoch-fenced reclamation retires unprotected objects through complete bounded namespace listings and exact-generation deletion. Catalog throughput, whole-journal updates/replay and listing bounds still limit scale; automatic balancing and multi-catalog transactions are not supplied. |
| R11 | Competitor comparisons/configuration parity | Map supported settings and list unsupported guarantees explicitly with primary references. | Comparison table, pinned SQLite/PostgreSQL/RocksDB/Redis/etcd/TiKV/FoundationDB/MySQL/MongoDB/Cassandra/Accord source studies and Iceberg atomic metadata analysis supplied. Consensus, cross-shard coordination and a selected SQLite pager/WAL component are implemented. Native Rust/C#/JS LSM directory discovery and five-runtime whole-store interchange now pass locally; legacy physical repair and other listed algorithm gaps remain unfinished. |
| R12 | Deep case study / collect data / plan + execute one PR | This directory, preserved original issue, plan, requirements, source research and test evidence; PR 139 with positive closing reference. | In progress |

The C# scheduling audit also reproduces starvation with four tiny owners and a
two-worker isolated ThreadPool. An asynchronous channel wait and ordered direct
execution fix it without extending existing timeouts. All 39 focused contracts
and 1,308 Debug/Release tests pass. All nine workflows pass on scheduling SHA 24554b0; see
async-worker-ci.txt. Later code requires fresh exact-head CI.

An exhaustive survey of all current/future successful DBMS is not a finite
verification target. The research inventory is explicit; unsupported algorithms
and unverified guarantees remain unfinished rather than being implied by tests
for a different design.

Public replay admission (R1/R3/R7/R8) now fences pending and failed commit/undo
owners in both native languages. Six before/after contracts per language and
actual-WASM execution preserve rows, journal sequence and cleanup ownership,
then verify following edits and replay outside owners. Full suites pass: 681
Rust, 1,449 C# per configuration and 365 actual-WASM tests. See
REPLAY-OWNER-PLAN.md and replay-owner.txt. All nine workflows pass for prior
replay-owner head 31a50a0, including a successful retry after an external Bun-download HTTP 502. See replay-owner-ci.txt. New code requires fresh CI.

Same-handle file journals (R1/R4/R7/R8) now retain uncertain framing after an
I/O error and repair before writer reuse. Six pre-fix schedules per adapter
reproduce partial ASCII/UTF-8 append followed by read/append/flush. Real bounded
Linux quota children, portable C# filesystem faults and JS short-write mocks
verify retry, complete-record preservation and exact reopening. Full local
suites pass: 690 native Rust, 1,469 C# per configuration, 365 actual WASM and
106 JS tests. All 17 JS modules and three WASM wrappers reach 100% measured
coverage; complete native/linked-WASM/C# coverage remains below 100%. See
INPROCESS-TAIL-PLAN.md and live-tail.txt. All nine workflows pass on journal-tail
head 7826d95; see live-tail-ci.txt. Later code needs fresh exact-head CI.

Legacy checkout and branch switching (R1/R3/R7/R8) now retain an exclusive
replay owner and durable portable intent through row/metadata/flush failures.
Startup validates paths, payloads and decisions before finishing the target;
applied markers preserve sequence allocation after payload eviction. C# keeps
failed-constructor cleanup reachable. Full local suites pass: 733 native Rust,
1,573 C# per configuration, 404 actual WASM and 106 JS. Fault matrices, real
process exits, corrupt-topology preservation, repeated recovery and following
edits are detailed in VERSION-MOVEMENT-PLAN.md and version-movement.txt. All nine
workflows pass on movement head d4cb43c; see version-movement-ci.txt. Full project
coverage and wider requirements stay open; later code needs fresh exact-head CI.

Legacy Rust transaction IDs (R1/R7/R8) now exclude recovered commit and abort
decisions before assigning explicit or automatic ownership. Four bounded native
restart regressions and four actual-WASM cases first fail, then preserve outcomes,
repeated recovery and following commits. Full suites pass: 737 native Rust,
408 actual WASM, 1,573 C# per configuration and 106 JS tests. Existing decision
indexes avoid a new scan or retained index; K collisions cost O(K). See
TRANSACTION-IDENTITY-PLAN.md and transaction-identity.txt. All nine workflows
pass on identity head 6d231fa; see transaction-identity-ci.txt. Archive ownership
and broader requirements remain unfinished; later code needs fresh CI.

Native chunk retention (R1/R4/R7/R8) now claims previously absent archive paths
without truncating existing entries or following symlinks. Four bounded Linux
restart contracts first fail; C# occupied-path contracts first fail separately
without asserting GUID reuse. Repeated collisions, competing writers, cleanup
retry, following commits and repeated reopening pass. Full suites pass: 742
native Rust, 408 actual WASM, 1,580 C# per configuration and 106 JS tests. See
ARCHIVE-OWNERSHIP-PLAN.md and archive-ownership.txt. All nine workflows pass on
archive head 7b03c07; see archive-ownership-ci.txt. Other legacy audits, complete
coverage and broader strategy requirements remain unfinished. Later changes
need fresh exact-head CI.

Legacy recovery (R1/R7/R8) now validates journals before replacing live history or
replaying rows, preserving idempotent parsed duplicates and retained sequence
floors. Read/replay/persistence failures retain admission; Rust cleanup retries
incomplete reloads and C# queued checkpoints/retention wait. Failing before cases,
29 native/actual-WASM focused tests and 146 C# cases verify repeated failure,
cleanup, following commits and reopening. Full suites pass: 765 native, 431
actual WASM, 1,690 C# per configuration and 106 JS. See
RECOVERY-VALIDATION-PLAN.md and recovery-validation.txt. All nine exact-head
workflows pass on 0e95c81; see recovery-validation-ci.txt. Later code requires
fresh CI; other legacy audits, complete coverage and broader requirements remain open.

Deep branch ancestry (R1/R3/R6/R7/R8) now uses iterative path construction in
both native languages. Three bounded 2,048-parent regressions per language
first fail with stack overflow; public semantic controls and two actual native
reopenings preserve fork/head cutoffs and replay order. Each language's shallow
independent oracle checks 2,560 finite paths, including legacy broken-chain
truncation. The actual-WASM depth control passes before and after; no WASM
overflow reproduction is claimed. Full local suites pass: 770 native Rust,
1,695 C# per configuration, 433 actual WASM and 106 JS tests, plus 217 repository
checks. All nine workflows pass on ancestry head aa601e2; see
DEEP-ANCESTRY-PLAN.md, deep-ancestry.txt and deep-ancestry-ci.txt. Complete
coverage and broader strategies remain unfinished; GROUP-COMMIT-PLAN.md
records the independent-session durability work that follows.


Independent-session shared flush (R1/R2/R5/R6/R7/R8/R9/R11) now validates in
input order, preserves separate revisions and known aborts, appends accepted
records and flushes once before publication. Unknown I/O outcomes preserve the
old visible view and require reopening to discover the committed prefix.
Matching native and Rust-backed JS/WASM APIs retain ordinary commit paths and
add no automatic queue or timer. Existing synchronous replication also avoids
its duplicate primary flush while preserving explicit catch-up semantics.
Before/after prototypes and production replication regressions, twelve durable-
image fault schedules, ABA controls, bounded publication gates, real replicated
files and ten abrupt exits per native language verify the declared behavior.
Full local suites pass: 784 native Rust, 1,722 C# per configuration, 446 actual
WASM, 111 JS and 217 repository tests. The JS modules and WASM wrappers remain
at 100% measured coverage; complete-project totals remain below target. See
GROUP-COMMIT-PLAN.md, [the group recipe](../../ENGINE-GROUP-COMMIT.md), and
verification/evidence for exact counts, costs and limits. New code needs fresh
exact-head CI. Page WAL/doublewrite, efficient full-state structures, other
competitor algorithms and the previously listed acceptance gaps remain open.


Ordinary legacy names/history (R1/R3/R6/R7/R8/R9) now use complete canonical
journal records while retaining row-only APIs/codecs. Before/after native and
actual-WASM contracts cover rename/remove, unique-name movement, named deletion,
cascade undo, zero/empty/external names, name-only versions, callback ordering
and failed resulting-name capture. Twenty-four independent-image fault schedules
per native language, corrupt metadata, replay fences, archives, movement failures
and bounded actual exits verify the documented paths. Full suites pass: 801
native, 460 actual WASM, 1,788 C# per configuration, 111 JS and 217 repository
tests. See NAMED-HISTORY-PLAN.md, named-history.txt and TRANSACTION-NAMES.md for
exact coverage, compatibility and costs. Whole-project coverage remains below
100%; row effects before observer journaling, custom unobserved side effects
and the wider strategy/efficiency/cluster criteria remain open at this publication.
All nine workflows pass on named-history head `9d394d5`; see named-history-ci.txt.
The following row-image investigation addresses the logical pre-observer window.


Ordinary pre-effect row undo (R1/R3/R6/R7/R8/R9/R11) now has a selected full
logical-image policy in Rust and C#. Before-fix durable-image tests, actual-WASM
cases and bounded real child exits reproduce persisted allocations, updates,
deletions and cascades before any observation. The default observer path retains
its documented cost and window. Selected owners flush their image before effects
and retain it through uncertain preparation, decision and cleanup. Reconciliation
adds unreported row/name changes to complete history before commit/version intent.
Identical-name calls also prepare/reconcile; genuine no-ops add no version.
The separate default-policy name-eviction regression now preserves the unobserved
previous name owner in the same pending undo batch.

Mandatory canonical v3 metadata fails closed in older readers. Shared codec
vectors, conflicting-image validation, ordered mixed recovery, repeated cleanup,
committed redo, callback names and version replay verify the declared logical
scope. See ROW-WAL-PLAN.md and TRANSACTION-UNDO.md. Full local suites pass: 820 native, 478 actual WASM, 1,901 C# per
configuration, 111 JS and 217 repository tests. See row-wal.txt for exact
coverage. New exact-head CI is pending publication. Full image/per-write scans remain explicit selected
costs; this does not supply physical torn-page repair, efficient page WAL or
complete power-loss verification. Whole-project coverage and the wider
composition, competitor and cluster requirements remain unfinished.

All nine row-undo workflows pass on `f95f59d`, after the newer CI formatter
rejected one else/foreach layout on the preceding commit. The explicit-block
correction preserves behavior and passes formatting plus all platform tests;
see row-wal-ci.txt and row-wal-format-ci-before.txt. Later code needs fresh CI.

Undo-image compatibility controls (R1/R7/R8) now exercise silent row/name replay
failure, unnamed images, sparse wide addresses and truly unreported allocations
and deletions. Existing production behavior passes; no behavior correction was
needed. Full suites pass: 823 native, 481 actual WASM, 1,909 C# per configuration,
111 JS and 217 repository checks. See IMAGE-REPLAY-PLAN.md and image-replay.txt
for exact scope and official coverage. All nine workflows pass on `71cad1c`;
see image-replay-ci.txt. Complete coverage and wider acceptance criteria remain
active; subsequent source changes need fresh exact-head CI.


Optional shared state (R6/R7/R9/R11) now selects persistent ordered rows
independently of admission, isolation, validation, constraints, history and log
encoding. Flat remains the default. Rust reuses rpds's Arc-based map and C#
reuses the existing .NET immutable sorted dictionary; JS/TS/WASM selects the
same native strategy. Focused finite identity controls, independent row/name
oracles, mixed branches, groups, faults and wire vectors pass in native and
actual-WASM execution. Rust's public BTreeMap accessor has an explicit lazy
compatibility cost that internal operations avoid. Full suites pass: 833 native, 489 actual WASM, 1,927 C# per configuration,
111 JS and 217 repository checks. Persistent examples and runtime conformance
pass; shared-state.txt records official coverage. All nine workflows pass on `3960c25`; see shared-state-ci.txt. See
SHARED-STATE-PLAN.md and [costs](../../ENGINE-SHARED-STATE.md).
The following incremental-record batch addresses complete revision encoding.
Finer decorator composition, page WAL/LSM/physical repair, full-project coverage
and the other listed acceptance gaps remain open.


Incremental logical revisions (R6/R7/R8/R9/R11) now independently select
Snapshot(default)/Changes with canonical mixed replay and exact predecessor
validation. Before-fix native/C#/JS failures, independent wire vectors, all map/
encoding combinations, durable-image group faults and forty actual child exits
per native language verify the declared scope. The finite 256-row control reduces
sixteen updates from 4,096 serialized rows / 966,063 bytes to sixteen rows / 4,246
bytes, retaining pins and exact reopening. Full local suites pass: 840 native,
496 actual WASM, 1,946 C# per configuration, 113 JS and 217 repository checks.
All runtime conformance and eight persistent cross-language configurations pass.
See CHANGE-RECORDS-PLAN.md, changes-before.txt, changes.txt and the protocol/recipe.
All nine workflows pass on dd22de1; see changes-ci.txt. Complete coverage, physical
page storage, finer composition and wider competitor/cluster criteria stay open.

The selected SQLite component (R1/R5/R6/R7/R8/R9/R11) adds physical page WAL,
WAL/FULL SQL group commits and explicit checkpoint counters at the existing log
seam. Bundled optional Rust, provider-neutral C# and separately exported JS
adapters preserve the default dependency paths. Independent schema/BLOB vectors,
stale/busy owners, commit faults, bounded storage-full, pinned readers and copied
WAL damage controls pass focused tests. Both native languages exercise forty
bounded child exits across map/logical/physical encodings. Full local suites pass:
852 native, 1,989 C# per configuration, 496 actual WASM, 168 JS and 217 repository
checks. All eight configurations pass Rust -> C# -> Node -> Bun -> Deno persistent
reopening. Four before-failing C# missing-result controls and nine before-failing
existing JS Unicode controls now pass. See SQLITE-JOURNAL-PLAN.md, sqlite-before.txt
and sqlite.txt for official coverage. All nine workflows pass on `3322f36`;
sqlite-ci.txt preserves the exact head and timestamps. This component addresses a selected physical page-WAL
path; legacy torn mapped pages, doublewrite, LSM, real power loss, complete
coverage and the remaining distributed/competitor requirements stay unfinished.


Independently selected constraint indexes (R1/R6/R7/R8/R9/R11/R12) now avoid
full candidate validation scans while retaining only selected immutable
pair/name/reference maps. Default scan mode is preserved. Pre-feature API
contracts fail before implementation; independent finite full-state oracles,
atomic swaps, incoming references, failed/group/branch publication and mixed
replay verify matching semantics. Deterministic root/lookup controls and the
finite allocation example document selected costs. Full local suites pass:
859 native, 2,003 C# per configuration, 502 actual WASM, 168 JS and 217 repository
checks. All sixteen SQLite compositions enforce the same References policy
and reopen through all five providers. The new index component measures 100%
lines/reported branches in native Rust, actual WASM and both C# configurations;
whole-project coverage remains below target. See CONSTRAINT-INDEX-PLAN.md,
constraint-index-before.txt and constraint-index.txt. All nine workflows pass
on `864839b`; constraint-index-ci.txt preserves exact SHA, timestamps and links.
Wider acceptance gaps remain unfinished.


Optional immutable object caching (R6/R7/R8/R9/R10/R11/R12) now reuses checked
payloads independently of other policies. Rust, C# and JS default to zero and
retain fresh catalog reads and full staging comparisons. Before-feature contracts,
independent finite UTF-8/LRU oracles, ownership, concurrent fills, corrupt fetch
repair and uncertain publication pass. Real etcd verifies both budgets, all
public examples, cached data outages and catalog-majority rejection. Full local
suites pass: 866 native, 2,016 C# per configuration, 509 actual WASM, 177 JS and
221 repository checks. New cache components reach 100% measured coverage; whole
project totals remain below target. See OBJECT-CACHE-PLAN.md, object-cache.txt
and the cache recipe for official totals and selected memory costs. All nine
workflows pass on `4729b53`; object-cache-ci.txt preserves exact SHA, timestamps
and links. Complete journal replay/updates,
persistent object retention and the previously listed wider criteria stay open.


Unicode graph integrity (R1/R7/R8/R12) now rejects active-path cycles and missing
children through iterative checked walkers. Bounded automated before failures
reproduce both native languages looping and Rust silently truncating a string.
C# reuses the installed converter through its walker interface; Rust preserves
the public Vec wrapper and adds a Result path. Shared DAGs, Unicode ordering,
existing UTF-16 policies, names, repeated rejection, following reads, deep finite
graphs and persistent reopening pass. Complete local suites pass: 875 native,
2,022 C# per configuration, 516 actual WASM, 177 JS and 221 repository checks.
New walker components reach 100% measured coverage in native Rust, actual WASM
and both C# configurations. All six actual-WASM coverage modules declare the
verified 256 MiB test-memory cap. See UNICODE-GRAPH-PLAN.md, unicode-graph.txt
and the reproduction recipe for official totals and O(depth) metadata costs.
Whole-project coverage, physical repair and the wider criteria remain unfinished.
All nine workflows pass on Unicode head `1a482ac`; later code needs fresh CI.


Deep structure output (R1/R7/R8/R12) now uses explicit source-spine frames and
one output buffer. Bounded before tests fail with native stack overflow in both
Rust entry points and C#. Independent shallow oracles preserve names, cycles,
shared children, reference order, caller paths and error progress; finite depth
2,048 controls and persistent Unicode named stores pass. Complete local suites
pass: 882 native Rust, 520 actual WASM, 2,027 C# per configuration, 177 JS and
221 repository checks. Node/Bun/Deno generated-WASM conformance also passes.
The shared Rust formatter reaches 100% measured component coverage; complete
project totals remain below target. See STRUCTURE-DEPTH-PLAN.md and
structure-depth.txt. Publication requires fresh exact-head CI. The native
physical-storage and broader criteria remain open.


Exact legacy text names and atomic reload (R1/R3/R6/R7/R8/R9/R12) now preserve
complete quoted tails and use versioned UTF-8 comment metadata for controls and
absent-address names. Seven automated before failures reproduce truncation,
quote loss, injected rows, dropped names and destructive failed reload. Eleven
native contracts plus a portable memory-reload control verify independent bytes,
malformed/future metadata, indexes, following edits, address reuse and repeated
Unicode reopening. Full suites pass: 894 native, 521 actual WASM, 2,027 C# per
configuration, 177 JS and 221 repository checks. The native text-format component
measures 100% lines/branches/functions; whole-project totals stay below target.
A separate existing 200-connection test exposed a .NET listener shutdown race;
its shutdown-only correction passes both full C# configurations. See
INLINE-NAMES-PLAN.md, TCP-SHUTDOWN-PLAN.md, inline-names-before.txt and
inline-names.txt. The SQL-free document-label example and runtime conformance
pass. New exact-head CI is pending publication; native page strategies and the
other inventoried requirements remain unfinished. Published head `faf191d`
already passes all nine workflows, including optional-SQL dependency guards;
structure-depth-ci.txt records exact timestamps, SHAs and job links.

Native shadow pages (R1/R5/R6/R7/R8/R9/R11) now have a SQL-free JS/TS provider
with independent byte vectors and finite persistence/corruption/ownership
contracts. Full JS suites pass 197 tests and enforced 100% complete production
coverage, plus strict types, 221 repository tests and Node/Bun/Deno actual-WASM
conformance under 16 independent configurations. Constant new-group write
counters and bounded process exits are recorded in native-pages-js.txt.
All nine workflows pass on `21847f9`, recorded in native-pages-js-ci.txt.
Rust now passes ten portable contracts both natively and in actual WASM, five
native-file contracts, strict Clippy and 16 Rust/Node/Bun/Deno shared-file
configurations. Full suites now pass 910 native, 531 actual WASM and 2,045 C#
tests per configuration. Matching C# files pass all 16 runtime configurations,
ending with 12 revisions and exact prior names. Independent corruption/fault
vectors, close fencing, quotas, following writes and bounded exits pass in each
native provider. Redundant physical repair and fresh CI remain unfinished;
NATIVE-PAGES-PLAN.md tracks those and the wider gaps.


Selected native mirrored repair (R1/R5/R6/R7/R8/R9/R11) now has matching
Rust, C# and JS/TS implementations. Independent mode-1 vectors, 36 damaged-copy
cases, every selected persistence/repair fault before and after its effect,
changed sources, canonical rejection before mutation, owner cleanup and bounded
real process exits pass. All 32 policy configurations pass Rust/Node/Bun/C#/Deno/
Rust file interchange with alternating closed-copy damage, exact repair and
twelve final revisions per file. Full suites pass 922 native, 539 actual WASM,
2,060 C# per configuration and 209 JS tests. See MIRRORED-PAGES-PLAN.md and
mirrored-pages-{js,rust,csharp}.txt. Whole-project coverage remains below target;
legacy physical repair/page undo, LSM, finer composition, complete journal replay/
updates, persistent object retention, balancing, multi-catalog transactions and
the other inventoried criteria remain unfinished. Fresh C# CI exposed formatter
and Windows test-ownership failures; the correction and Rust mirror now pass all
nine workflows on b0612d0, recorded in mirrored-pages-parity-ci.txt. The
next measured command-path audit is tracked in CLI-COVERAGE-PLAN.md.

Native command recovery/version-name contracts (R1/R3/R7/R8/R9/R12) now include
before failures for delimiter-containing tag names, accepted complete-tag aliases,
exact checkout/listing, error preservation and subsequent edits. Thirty-one C#
entry-point contracts verify mapped-store release after every returned command.
Complete runs pass 923 native Rust and 2,091 C# tests per configuration. All
production files remain in the measured denominator: C# reaches 95.67% Debug
and 96.44% Release lines, still below target. Both native public tag-name examples
and finite declared-nesting-limit controls pass. See CLI-COVERAGE-PLAN.md,
VERIFICATION.md and cli-before.txt. Fresh exact-head CI and all wider inventoried
criteria remain unfinished.

Retained C# public query compatibility (R1/R7/R8/R9/R12) now has 33 additional
contracts without production changes. All three processors preserve exact rows,
names, notifications, following edits and native reopening for no-op shapes;
existing rejection/reuse/diagnostic semantics also pass. No suspected leaf-list
bug reproduces. Both complete configurations pass 2,124 tests. Complete measured
lines reach 96.11% Debug / 96.79% Release, still below R8's target. See
QUERY-COMPATIBILITY-PLAN.md and query-compatibility.txt. Command head 63ccbc7
passes all nine workflows, preserved in cli-ci.txt; later changes require fresh
CI. Every wider inventoried gap remains unfinished.

Public Rust argument contracts (R3/R6/R7/R8/R9/R12) now run seven semantic
groups both natively and in actual WASM, preserving complete configurations,
exact values, bounds, rejection recovery and independent decorator selection.
No production defect reproduced. Full suites pass 930 native / 546 WASM tests;
complete measured lines reach 94.29% / 78.24%, below the whole-project target.
See ARGUMENT-CONTRACTS-PLAN.md, VERIFICATION.md and arguments.txt. Query head
580dcb2 passes all nine workflows, recorded in query-compatibility-ci.txt.
Later changes require fresh CI; every wider inventoried gap remains unfinished.

Rejected mapped-file growth (R1/R5/R7/R8/R9/R12) now has a real finite quota
reproduction before the fix. The owned C# mapping retains its current pointer,
view and handle until a replacement is ready; failed initialization releases
new resources. Repeated rejection, exact dirty bytes, flush/retry, following
writes, shrink/regrowth and reopening pass. Both complete configurations pass
2,126 tests; complete measured lines reach 96.16% Debug / 96.84% Release. See
MAPPED-GROWTH-PLAN.md and mapped-growth-{before,}.txt. All nine workflows pass
on prior argument head 164f6f0, recorded in arguments-ci.txt. Fresh platform CI
and all wider inventoried requirements remain unfinished.


Journal value ownership (R1/R3/R7/R8/R12) now has seventeen passing public
controls, including single capture, read-only exposure, complete equality/hash
collections and a finite 65,536-unit .NET name. No production defect reproduced.
Both complete configurations pass 2,143 tests; all production files remain
measured at 7,950/8,260 lines and 4,268/4,638 branches in Debug, 6,104/6,298
lines and 4,223/4,611 branches in Release. See JOURNAL-VALUE-PLAN.md and
journal-values.txt. Whole-project 100% and every broader criterion remain open.
Mapped-growth head 1aafc31 passes C# Linux/macOS/Windows and eight workflows;
Rust Windows fails an existing movement child deadline. See
mapped-growth-ci-before.txt; investigation and fresh exact-head CI remain active.


Independent immutable sorted runs (R5/R6/R7/R8/R9/R11/R12) now share run arrays
in native Rust and C#, selected through the same WASM/JS/TS session API. Finite
map oracles and snapshot/merge controls verify newer-write precedence, tombstones,
resurrection, no-ops, all conversions and independent policies. Full local suites
pass 935 native plus six doctests, 546 actual-WASM, 2,164 C# per configuration,
209 JS on Node 26/24 and 221 repository checks. All 48 native-page compositions
pass 288 cross-language file writers with mirrored repair. See SORTED-RUNS-PLAN.md,
sorted-runs-before.txt and sorted-runs.txt for complete official coverage and
explicit selected costs. Physical SSTables, legacy mapped repair, complete 100%
coverage and every other wider unmet criterion remain unfinished. All nine
exact-head workflows pass at a1f12fec, including all three native platforms;
sorted-runs-ci.txt records heads, timestamps and run URLs. Journal-value e39ecce
and movement-diagnostic 3c22e69 each pass
all nine workflows; their CI evidence files preserve exact heads and timestamps.
The earlier Windows movement timeout's cause remains unknown.

Fallible allocation composition (R1/R6/R7/R8/R9/R12) now preserves errors through
Rust trigger/query and transaction/version layers. Ten real journal-path faults
first panic; two portable regressions first ignore faults or dispatch incorrectly.
Four portable contracts now pass natively and in actual WASM, with eleven
bookkeeping fault positions, successful/no-op trigger controls and parked errors.
Five C# controls preserve exception identity without a production change.
Full suites pass 949 native plus six doctests, 550 WASM, 2,169 C# per configuration,
209 JS and 221 repository checks. Native fixtures require cleanup, following
writes and two exact reopenings. See FALLIBLE-COMPOSITION-PLAN.md and
fallible-composition-before.txt. Import allocation bypasses are addressed by the
following batch; whole-project 100% and every wider unmet criterion stay active. All 48 physical/policy choices pass 288 shared-file writers with mirrored
repair; fallible-composition.txt preserves commands and complete coverage. All
nine workflows pass at c530a6a, including native Linux/macOS/Windows;
fallible-composition-ci.txt retains exact evidence.

Import allocation (R1/R6/R7/R8/R9/R12) now propagates failures through explicit
and nested text imports, binary address reservation and row reallocation after
trigger deletion. Six before-fix native panics and five portable ignored faults
are preserved, alongside passing named-import controls. Full local suites pass
963 native plus six doctests, 556 actual-WASM, 2,172 C# per configuration, 209 JS
and 221 repository checks. Native/C# fixtures verify cleanup, idempotent retries,
following writes and exact reopening; C# is a passing exception control without
production changes. All 48 physical/policy choices pass 288 shared-file writers
with mirrored repair. See IMPORT-ALLOCATION-PLAN.md and import-allocation.txt for
complete coverage and the distinction between error propagation and atomic undo.
All nine workflows pass at 847ed01, including native Linux/macOS/Windows;
import-allocation-ci.txt preserves exact heads, creation timestamps and URLs.
Every wider unmet criterion stays active.

Portable archive contracts (R1/R7/R8/R9/R12) now run ten existing compatibility
cases in actual WASM while preserving six native remote/CLI/filesystem cases.
Four finite stream groups check 31 read-failure boundaries, 30 truncated prefixes,
three trailing bytes, 30 partial-write failures and short successful I/O, with
exact rows/names and following use. A WASM-only file-helper control verifies the
unsupported filesystem capability. No production change is claimed. Complete
Rust suites pass 967 native plus six doctests and 571 actual-WASM tests; complete
official measured lines are 94.37% and 81.45%, below R8's target. See
ARCHIVE-STREAM-PLAN.md and archive-stream.txt. C#/JS production and the preceding
complete validation are retained unchanged. At d98c2ee eight workflows pass,
but four Rust Windows version-exit cases hit the existing ten-second deadline.
archive-stream-ci-before.txt preserves exact timestamps, SHA and error lines.
Phase/stream diagnostics are published at b03bd4e without changing the deadline.
All nine fresh workflows pass, including all three native platforms; the Windows
exit suite passes seven contracts in 6.64 seconds. The original timeout cause
remains unknown. version-exit-diagnostics-ci.txt preserves exact evidence.
Every wider unmet criterion remains active.

Optional logical checkpoints (R1/R3/R5/R6/R7/R8/R9/R10/R11/R12) export retained
committed snapshots without mutating the live journal. The separately selected
cluster interval uses the existing atomic generation CAS and leaves default
append behavior intact. Focused native/C# contracts verify selected constraints,
exact historical allocators and branch descriptors, private pending reservations,
history eviction, immutable objects and before/after unknown outcomes. Complete
local suites pass 975 native plus six doctests, 2,180 C# per configuration,
580 actual-WASM, 211 JS and 221 repository checks. Real etcd passes three modes;
48 existing physical/policy combinations pass 288 writers, and Node/Bun/Deno
produce identical 36-case payloads. JOURNAL-CHECKPOINT-PLAN.md and
journal-checkpoint.txt record exact results and complete coverage. All nine
workflows pass on a329fa7, including native Linux/macOS/Windows matrices; see
journal-checkpoint-ci.txt. Two initial C# Windows fixture cleanup failures pass
on an unchanged-code debug rerun; their underlying cause remains unknown.
Initial-checkpoint support in the physical delta decorator is verified
under CHECKPOINT-DELTA-PLAN.md.
Old object reclamation, local file replacement and every wider unfinished
requirement remain open.

Initial checkpoint/delta composition (R1/R3/R6/R7/R8/R9/R12) now accepts exactly
one initial engine3 in each physical codec. Shared before-fix vectors reproduce
the rejection independently in Rust, C# and JS. Complete fresh suites pass
980 native plus six doctests, 585 actual-WASM, 2,187 C# per configuration, 215 JS
and 221 repository checks. Both replica orders, independent compression intervals,
exact bytes, retained history/allocators/pins and unknown-outcome fences pass.
All three JS runtimes agree on 36 actual-WASM compositions. See
CHECKPOINT-DELTA-PLAN.md, checkpoint-delta-before.txt and checkpoint-delta.txt.
The changed codec reaches 100% measured lines/branches, with native/WASM
functions also at 100%; complete project coverage and every wider criterion
remain unfinished. All nine exact-head workflows pass on 68fa923, including
every native platform; checkpoint-delta-ci.txt records the runs and timestamps.

Persisted initial checkpoints (R1/R3/R6/R7/R8/R9/R12) now pass all 48 native
page/policy configurations through Rust, Node, Bun, C#, Deno and Rust again.
Each of 288 writer handoffs preserves the exact retained revision-40 checkpoint,
allocator gaps 33 through 40, names, references and pins before committing two
following revisions. The final revision is 52 with 44 rows. Mirrored handoffs
include closed-copy data/metadata damage and repair. The ordinary revision-zero
control also passes 48 configurations/288 writers. This extends examples and
normal-CI integration; no production engine/page/codec change is claimed.
CHECKPOINT-PAGES-PLAN.md and checkpoint-paged.txt preserve scope and evidence.
Local atomic journal replacement and every wider unmet criterion remain open.

Optional portable two-bank journals (R1/R3/R5/R6/R7/R8/R9/R10/R11/R12)
now replace a complete ordered sequence by preparing an inactive bank and then
synchronizing an epoch-bound root. Independent shared vectors and missing-feature
controls precede implementation. Finite faults before/after every baseline I/O,
all 128 torn-root lengths, stale bank identities, mirrored-bank reuse, pending
input, exact Unicode bytes and u64 limits pass. Two additional Rust ownership
regressions first fail, then verify that both banks drop before root on ordinary
destruction and every tested constructor failure.

Complete local suites pass 992 native Rust tests plus six doctests, 597 actual
WASM tests, 2,198 C# tests in each configuration, 226 JS tests and 221 repository
checks. Node/Bun/Deno each pass all 11 portable controls and actual-WASM integration;
36 composition and payload reports agree exactly across all three runtimes.
See LOCAL-CHECKPOINT-PLAN.md and local-checkpoint{,-before}.txt. All nine
workflows and both native language platform matrices pass on c66e75f;
local-checkpoint-ci.txt records exact heads, timestamps and URLs. Native owning factories remain unfinished, as do
whole-project 100% coverage and every wider unmet acceptance criterion.

Engine checkpoint publication (R1/R3/R5/R6/R7/R8/R9/R10/R11/R12) now retains
the commit mutex throughout construction and optional atomic replacement.
Unsupported logs reject before mutation; unknown outcomes fence the engine and
pending sessions. Physical delta adapters prepare and validate before inner I/O,
adopt fresh codecs on true or false acknowledgement, and fence uncertain errors.
Bounded native concurrency, exact retained history/allocators/pins, point locks,
admission, stale dependencies and saved branch bases pass. All 72 combinations
of bank mode, state, encoding, history and delta pass in Rust/C# and actual WASM
on Node, Bun and Deno. Complete local suites pass 1,002 native plus six doctests,
620 actual WASM, 2,210 C# per configuration, 230 JS and 221 repository checks.
See ENGINE-COMPACTION-PLAN.md and engine-compaction{,-before}.txt for exact
coverage, real-etcd and both 48-configuration persistent handoff controls.
The eight previously unregistered WASM wrapper tests now execute. All nine
workflows and Rust/C# Linux/macOS/Windows matrices pass on 133c4ff;
engine-compaction-ci.txt records exact heads, timestamps and job results.
Native ownership/process recovery is recorded in the following batch; all wider
gaps remain active.

Native checkpoint ownership (R1/R3/R5/R6/R7/R8/R9/R10/R11/R12) now acquires
every raw root/bank fence before validation, reset or mirrored repair. All acquired
owners close in reverse order, root last, on constructor failure and explicit
close; original and every cleanup error remain observable. Pending appends are
not flushed. Uncertain close retains affected fences and repeated close does not
retry native handles. Existing JS constructors also preserve acquisition and
directory synchronization errors through failing cleanup. Independent before
controls and finite replacement/repair exits pass in all native providers.

Complete final suites pass 1,013 native Rust tests plus six doctests, 620 actual
WASM, 2,227 C# per configuration, 239 JS and 221 repository checks. Each ordinary,
imported-checkpoint and native-replacement handoff passes 48 configurations and
288 writers; all three etcd modes and runtime comparisons pass. JS enforces 100%
lines/branches/functions, the new C# factory measures 100% lines/branches, and the
new Rust factory measures 214/221 lines, 5/6 branches and 57/58 functions. Complete
project denominators remain below target. See NATIVE-CHECKPOINT-PLAN.md and
native-checkpoint{,-before}.txt. Fresh native-batch CI and all wider unfinished
acceptance criteria remain active; process exits do not certify hardware power loss.

The native ownership CI correction now passes all nine exact-head workflows on
`6a0f7c9`, including both native platform matrices. Windows runs the unchanged
physical checkpoint probe and independent positive/negative scheduling controls.
native-checkpoint-ci-fix.txt preserves exact timestamps, permanent URLs and saved
logs. The earlier progress observation has no established cause. This evidence
does not validate the following uncommitted mirrored-root feature or complete
any wider unmet criterion.


Independently selected checkpoint-root copies (R1/R3/R5/R6/R7/R8/R9/R10/R11/R12)
now preserve acknowledged epochs through single-copy damage in either bank mode.
Missing portable/native selection controls precede implementation. Full bank
validation and all root-source checks precede root repair; both published root
copies synchronize before acknowledgement. Existing single-root bytes/APIs/costs
remain compatible, and mode conversions reject before altering existing bytes.
All four native ownership configurations retain fencing and reverse cleanup.

Full local suites pass 1,028 native plus six doctests, 631 actual WASM, 2,264
C# per configuration, 261 JS with enforced 100% production coverage, and 221
repository checks. Each native language independently executes 258 bounded
before/after process exits. Compaction passes 96 configurations and 576 writers
with independent closed root/bank damage; ordinary/imported controls retain
48/288 each. MIRRORED-ROOT-PLAN.md and mirrored-root.txt record exact coverage,
wire compatibility, selected costs and limits. Fresh exact-head CI is required;
all wider unfinished criteria stay active.


All nine workflows pass on mirrored-root head `df6e01c`, including Linux/macOS/
Windows native tests and actual WASM. `mirrored-root-ci.txt` preserves exact run
IDs and timestamps. This does not complete the wider acceptance criteria; later
maintenance work needs independent exact-head validation.


Optional namespace maintenance (R1/R4/R6/R7/R8/R9/R10/R11/R12) now supplies
revision-pinned complete bounded etcd scans and single-send exact-generation
deletion in Rust, C# and JS. Ordinary atomic stores keep their original contract;
selected codecs initialize on demand. Actual before failures, 94 independent
wire cases, 98 Bun/Deno controls, native examples and real three-member faults
verify the capability. Complete local suites pass: 1,032 native Rust, 2,268 C#
per configuration, 631 actual WASM, 359 JS and 221 repository checks. JS stays
at 100% measured coverage; complete native/C#/linked-WASM totals remain below
target. See MAINTENANCE-PLAN.md, maintenance-before.txt, maintenance.txt and the
maintenance recipe. Fresh exact-head CI is required after publication. This
capability alone does not implement object collection; fencing, complete root
discovery and coherent reader refresh remain required and unfinished.

All nine workflows pass on maintenance head `44e0fa8`, including both native
Linux/macOS/Windows matrices. `maintenance-ci.txt` preserves exact heads, run
timestamps and job URLs. The following selected reclamation batch needs its own
complete local and exact-head CI evidence; wider acceptance gaps remain active.

Selected epoch-fenced reclamation (R1/R4/R6/R7/R8/R9/R10/R11/R12) now supplies
conditional object retirement in Rust, C# and JS/WASM through the unchanged
atomic journal interface. Publishers guard the epoch; collectors validate every
complete namespace before exact-generation deletion and retain their own fence.
Owned readers retry one whole group only after pointer and generation change;
aliased live keys and retained history remain protected. Test-first missing APIs,
65 shared vectors, bounded concurrency models and actual three-member failures
verify the declared scope. This removes the append-only object's unbounded
retention cost for deployments that explicitly select complete bounded scans.

Full suites pass 1,057 native plus six doctests, 656 actual WASM, 2,307 C# per
configuration, 518 JS and 221 repository checks. Bun/Deno pass all159 selected
cases; complete JS production coverage remains enforced at 100% for all three
metrics. The rebuilt real-etcd integration passes eight cross-language handoffs,
ten mutation-loss cases, coherent refresh, cache/data outage and leader/majority
loss. See RECLAMATION-PLAN.md, reclamation{,-before}.txt and the recipe. Complete
native/C#/linked-WASM coverage, mapped-page repair, on-disk LSM, finer independent
decorators, wider competitor algorithms and scaling gaps remain unfinished.
Fresh exact-head CI is required after publication; no whole-issue completion
or physical power-loss claim follows from these tests.


The reclamation contract audit (R7/R8/R9/R12) adds seven matching public controls
per language without production changes. Invalid routes/configuration send no
request; read-only dependencies need no route; all durability combinations,
interrupted initialization/staging confirmation and whole-group refresh failures
retain their documented boundaries. Complete suites pass 1,064 native Rust plus
six doctests, 663 actual WASM, 2,314 C# per configuration, 525 JS, 166 selected
Bun/Deno each and 221 repository checks. Complete denominators remain in
VERIFICATION.md and reclamation-contracts.txt. All nine workflows already pass
on unchanged production source `419f7c7`; the new tests require fresh exact-head
CI. RECLAMATION-CONTRACT-PLAN.md tracks publication. Every broader unmet
criterion remains unfinished.


Physical immutable sorted journal tables (R1/R5/R6/R7/R8/R9/R11/R12) now use
native doublets pages in Rust, C# and JS, with portable Rust controls in actual
WASM. Selection is independent of SQLite and the existing default engine paths.
Complete bounded startup validation retains keys/addresses/page hashes without
a payload map; point reads verify physical pages and return owned records.
Positive u64 tombstone generations and live empty journals remain distinct.
Missing-API controls precede implementation. Twenty-five independent valid/
corrupt binary cases and all 25 Rust/C#/Node/Bun/Deno native writer/reader pairs
verify the declared table scope. Native readers preserve all three file hashes.

Complete suites pass 1,073 native Rust plus six doctests, 672 actual WASM,
2,349 C# per configuration, 533 JS, 174 selected Bun/Deno each and 221 repository
checks. JS retains enforced 100% production coverage; complete native/C#/linked
WASM coverage remains below target. The C# count includes splitting the nine
native checkpoint process-exit configurations into 36 cases after the Windows
120s case timeout on 9a9a95c. All original 258 cutpoints and deadlines remain.
See PHYSICAL-SORTED-TABLE-PLAN.md, CHECKPOINT-CI-SLICES-PLAN.md and the
sorted-table{,-before}.txt and checkpoint-slices.txt reports. Fresh exact-head
CI is required after publication. WAL/memtable/manifest recovery, compaction
and retirement remain unfinished under PHYSICAL-LSM-PLAN.md. A physical table
alone does not complete on-disk LSM or any wider unmet acceptance criterion.

All nine workflows and six native platform jobs now pass on physical-table/model
head `523888a`; sorted-table-ci.txt records exact timestamps and URLs. This
includes Windows checkpoint slices retaining every original process cut.

Selected semantic LSM WAL replay (R1/R5/R6/R7/R8/R9/R11/R12) now supplies matching
Rust/C#/JS checkpoint/manifest/group codecs without storage I/O. Actual missing
APIs fail before implementation; 111 independent shared vectors, owned values,
invalid public-state admission, exact u64 boundaries, actual-WASM execution,
lazy import controls and SQL-denial checks verify the component. Complete local
suites pass 1,075 native plus six doctests, 674 actual WASM, 2,351 C# per
configuration, 535 JS, 176 selected Bun/Deno each and all 221 repository checks.
The 25 native physical-table writer/reader pairs continue to pass. JS production
coverage remains enforced at 100%; complete native/C#/linked-WASM denominators
remain below target and are preserved in VERIFICATION.md and lsm-wal.txt.
LSM-WAL-PLAN.md tracks publication and fresh exact-head CI. Physical selected-file
validation, buffering/store APIs, publication, ownership, compaction, retirement,
native process tests and all wider unmet criteria remain unfinished.


The selected portable physical LSM store now composes immutable native doublets
tables, bounded buffering, semantic WAL publication/compaction and maintenance
consumers in Rust/C#/JS. Full binding/history/capacity validation precedes staged
repair and orphan retirement. All nine exact-head bc364d2 workflows pass;
lsm-store{,-before,-ci}.txt preserve reproductions, full counts and platform jobs.
Its already-constructed WAL still requires separate native discovery ordering.

Borrowed checkpoint inspection (R1/R5/R6/R7/R8/R9/R11/R12 prerequisite) now returns
owned selected-prefix records before initialization, truncation or repair. Actual
missing-API failures precede implementation. Complete suites pass 1,102 native
Rust plus six doctests, 698 actual WASM, 2,375 C# per configuration, 576 JS at
enforced 100% production coverage, 217 selected Bun/Deno each, all 25 existing
native table handoffs and 221 repository checks. Every native/C#/linked-WASM
production denominator remains included and below 100%; checkpoint-inspect.txt
and VERIFICATION.md retain them. NATIVE-LSM-DISCOVERY-PLAN.md continues native
exclusive directory ownership, semantic validation before WAL constructor effects,
whole-store file/process interchange and all wider unmet acceptance criteria.
Inspection alone does not complete those requirements. PR139 remains draft.

Selected JavaScript native LSM composition (R1/R5/R6/R7/R8/R9/R10/R11/R12 partial)
now validates bounded exclusive file discovery and all selected historical state
before WAL recovery effects. All 41 controls pass in Node/Bun/Deno, including
134 finite syscall cuts and ten actual reaped abrupt children per runtime. All
617 Node tests enforce 100% production coverage; actual-WASM direct/partitioned/
reclaiming consumers pass in each runtime. Native Rust/C# facade parity, complete
whole-store interchange and all wider unmet criteria remain required. These
results do not replace full native/C#/linked-WASM coverage or certify power loss.
native-lsm-js{,-before}.txt preserve scope and evidence; PR139 remains draft.


Native C# LSM discovery (R1/R6/R7/R8/R9/R11) now validates selected state read-only
under one outer owner before writable recovery. Both complete configurations pass
2,409 tests, including 34 new native cases, 124 before/after cuts at 62 operation
boundaries and ten reaped abrupt children. Three actual cluster consumers and all
32 C#/Node/Bun/Deno complete-store handoffs pass with physical hash preservation.
The surviving-copy regression first fails and is corrected without early repair.
Debug coverage is 9,280/9,606 lines and 5,559/6,004 branches; Release is
7,167/7,385 lines and 5,505/5,965 branches. Every production denominator remains
included and below 100%. See native-lsm-csharp{,-before}.txt. Preceding native JS
head 39a551d has nine green workflows in native-lsm-js-ci.txt. New C# source needs
fresh CI; Rust native discovery, five-runtime interchange and all wider unmet
criteria remain active in this draft, unmerged PR.


Native Rust LSM composition (R1/R5/R6/R7/R8/R9/R10/R11/R12 partial) now completes
exclusive bounded directory discovery, read-only physical/semantic validation,
all eight mirror modes and exact-once uncertain cleanup. The complete local run
passes 1,138 native tests plus six doctests, 698 actual WASM tests, three real
cluster consumers and all 50 Rust/C#/Node/Bun/Deno whole-store handoffs. It includes
408 finite compaction cuts, 730 opening cuts and 408 actual process exits. Native
coverage remains 16,712/17,614 lines and 3,211/3,698 branches; whole-linked WASM is
12,900/15,323 lines and 2,593/3,101 branches. No denominator was removed. Fresh CI
is still required for this step. Legacy physical repair, power-loss certification,
remaining distributed algorithms/composition, whole-project 100% coverage and
remaining scaling/replay bounds stay unfinished; this PR remains draft/unmerged.


Bounded append-only immutable journal chunks (R5/R6/R7/R8/R9/R10/R11/R12 partial)
now reuse complete unchanged prefixes under independently selected cache and
count/UTF-8-byte limits. Whole-journal defaults remain. Metadata admission,
full staging comparisons, owned reads and one catalog CAS pass matching native,
C# and actual-WASM controls; legacy epoch collectors refuse unsupported roots
before deletion. All 1,146 native, 706 actual-WASM and 2,429 C# tests per full
configuration pass. All 626 Node tests enforce full production coverage; strict
types and selected Bun/Deno controls pass. Four actual consumers per language
composition and 50 whole plus 50 chunked five-runtime complete-store handoffs
pass. journal-chunks{,-before}.txt records full nonexcluded coverage denominators,
finite failures and measured payload/cost tradeoffs. Complete native/C#/WASM
100%, epoch-fenced chunk reclamation, full replay/transfer bounds, catalog
throughput and multi-catalog/automatic balancing remain unfinished. Current-epoch
restaging and coherent whole-group reader retry require separate verification;
CHUNK-RECLAMATION-PLAN.md retains that next step. All nine workflows pass on
published append-only/dependency-correction head 58d35f8, recorded with exact
timestamps in journal-chunks-ci-after.txt. Preceding native Rust publication
8d2ecdf also has all nine green in native-lsm-rust-ci-after.txt. No wider
acceptance criterion is inferred complete.


Epoch-fenced chunk reclamation (R1/R4/R5/R6/R7/R8/R9/R10/R11/R12 partial) now
protects every live reference and restages all selected data under the captured
epoch after a fence. Whole-group reader retry discards all old prefixes/cache
hits. Shared independent vectors, all sixteen cache masks, before/after cuts,
corruption, overlapping collectors, ABA and following edits pass matching native,
C# and actual-WASM controls. Complete suites pass 1,159 native plus six doctests,
719 actual-WASM, 2,442 C# per configuration and 640 Node tests with enforced full
production coverage. Five real compositions, all 150 five-runtime complete-store
handoffs and two real three-member etcd clusters pass. Reproducing controls and
all nonexcluded coverage denominators remain in reclaiming-chunks{,-before}.txt
and VERIFICATION.md. Full native/C#/linked-WASM 100%, legacy physical repair,
physical power-loss verification, further algorithms/composition, full replay/
transfer and listing bounds, catalog throughput, automatic balancing and multi-
catalog transactions remain unfinished. Selected frequent collection restages
full current-epoch contents; measured payload savings depend on fence frequency.
Fresh exact-head CI remains required; the PR stays draft/unmerged.


Reclaiming admission/provider controls (R1/R7/R8 partial) verify array lengths,
generation-zero payloads, noncanonical live/scanned keys with eligible orphans,
and a missing unchanged object after a catalog-generation change. Whole/chunked
layouts preserve following operations in Rust, C# and JS; shared Rust controls
also execute in actual WASM, and all four JS controls pass in Node/Bun/Deno.
Full suites pass 1,163 native plus six doctests, 723 actual-WASM, 2,447 C# per
configuration and 644 Node tests with enforced full production coverage. Final
orphan assertions rerun all 50 native maintenance controls, all actual WASM and
all C# Debug, completing 08:23:27Z. Native/linked-WASM each gain four covered
branches; complete coverage and generic denominators remain below 100% and
unfiltered. See RECLAMATION-ADMISSION-PLAN.md and reclamation-admission.txt.
The preceding c1abe42 head passes eight workflows but C# Windows fails one
finite progress control. A before-failing deferred-context child/regression
establishes a helper dependency; context-independent capture/process awaits
preserve existing bounds and reaping. Complete local checks pass; fresh exact-
head Windows and all other CI remain required under PHASE-CONTEXT-PLAN.md.
No production contract changes. Legacy physical repair/power-loss verification,
remaining algorithms/composition, complete project coverage and remaining
scaling criteria stay unfinished; the PR remains draft/unmerged.

The helper/admission correction head 7e9799a now passes all nine workflows,
including 2,447 C# tests on Windows/Linux/macOS. Exact run links, commit and run
timestamps are in phase-context-ci-after.txt. That publication verification
does not complete the wider inventory.

Owned Rust mapped-view barriers (R1/R6/R7/R8/R9/R11 partial) now flush the live
Windows view before its owned file and retain lifetime bookkeeping through
growth/shrink, Drop and generic decorators. Explicit adoption attaches the
matching owner; a foreign path alone cannot supply view synchronization. The
missing APIs first fail against immutable published source. Independent ordered
syscall faults, range assertions, poisoned ownership, closed handles and actual
reopening controls pass; an independent range assertion also catches a draft
tail-only descriptor. All 1,175 native plus six doctests, 723 actual-WASM and
221 repository checks pass. Complete native LLVM is 17,080/17,958 lines and
3,288/3,774 branches; complete linked WASM is 13,144/15,667 lines and
2,663/3,171 branches. No denominator is filtered. mapped-flush{,-before}.txt
records all metrics, selected cost and limits; fresh exact-head platform CI is
still required. Upstream failed-resize safety, legacy physical repair and
hardware power-loss verification remain unfinished alongside every other
unmet R1–R12 criterion. PR139 remains draft/unmerged.


Retained plain-data mapped growth (R1/R6/R7/R8/R9/R11 partial) corrects the
actual old-view lifetime after rejected file extension, replacement mapping,
capacity overflow and initialization panic. A bounded quota child first faults
through safe allocation access; five durable regressions first fail against
immutable source. Replacement preparation preserves the pointer, contents,
barrier, true initialization bound and following retry. Windows extends through
a temporary mapping object while retaining the old view. Complete local suites
pass 1,184 native plus six doctests, 723 actual-WASM and 221 repository tests.
Every production denominator remains included: native 17,176/18,090 lines and
3,291/3,780 branches; linked WASM 13,144/15,737 lines and 2,663/3,171 branches.
See RUST-MAPPED-GROWTH-PLAN.md and mapped-grow{,-before}.txt for costs, immutable
failures and complete metrics. Fresh exact-head platform CI remains required.
Failed shrink, arbitrary destructor-growth errors, physical repair/power-loss
verification, complete project coverage and all other unmet requirements stay
unfinished. PR139 remains draft/unmerged.

R1/R7/R8 read-only discovery contracts now add nine independent C# controls.
All 2,460 tests pass in each complete Debug/Release suite; ownership, denied
effects and exact-once cleanup are exercised without production changes.
Debug covers 9,417/9,726 lines and 5,699/6,144 branches; Release covers
7,270/7,471 lines and 5,643/6,105 branches. Both retain the same denominators.
See DISCOVERY-GUARDS-PLAN.md and discovery-guards.txt. All 221 repository checks
pass after the tracked growth probe is added to Dependabot's Cargo directories.
Fresh exact-head CI, complete coverage and every wider unmet criterion remain
required; these audit controls do not certify hardware persistence.
