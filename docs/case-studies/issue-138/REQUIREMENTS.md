# Acceptance inventory

No required sub-issues were returned by GitHub's sub-issue API. Initial issue
comments and all three PR comment/review endpoints were empty.

PR feedback added after that initial inventory requests recovery of an interrupted
session and reporting the interruption to Codex/OpenAI. The preserved ancestry
commit has been recovered; validation continues under DEEP-ANCESTRY-PLAN.md.
The reporting request is fulfilled by the linked public report in
[SESSION-RECOVERY.md](SESSION-RECOVERY.md). That request does not change or
complete the original R1–R12 acceptance criteria.

| ID | Issue requirement | Solution and verification plan | Status |
| --- | --- | --- | --- |
| R1 | Fully test ACID readiness | Failure injection at journal append/flush, data checkpoint, rollback and recovery; anomaly schedules and bounded concurrent histories; report exact persistence assumptions. | In progress: durability, corrupt recovery, Windows mapped flush, short filesystem reads, and cross-language lexical regressions reproduced and corrected. C# whole-graph disposal joins borrowers before closing mapped dependencies and preserves stores after cleanup failure. Real process-exit recovery probes pass; power-loss and distributed fault coverage remain incomplete. |
| R2 | Configurable concurrency strategies | Exclusive writer admission, optimistic validation and optional fail-fast pessimistic point writes; read committed, snapshot isolation and whole-revision or point-read serializable validation. Same transaction/storage API. | Implemented in Rust, C#, and Rust-backed JS/WASM; visibility, dirty writes, write conflicts, scan/write-skew validation, admission release, bounded concurrent writer tests, optional point-read tracking and independent finite serial-history oracles. Additional competitor strategies remain within R5's wider scope. |
| R3 | Configurable version control | Existing branches/tags plus optional no-history, bounded and complete immutable revision history; pin active snapshots independently of retention. | Retention and independent journal branches with atomic three-way merge implemented in all language surfaces. Tests cover conflicts, repeated/incremental merges, constraints, exclusive admission, unknown flush outcomes, allocator recovery and reopening from a saved merge base after history eviction. The legacy C# version owner retains admission and cleanup retry across failed rollback; 24 phase/entry-point regressions and a terminal retention control verify mapped recovery. Committed version cleanup retries publication after checkpoint or metadata faults without undoing durable data; 48 mapped phase/entry-point cases verify fencing, exact recovery and idempotent markers. Rust generic and version owners now retain known-commit checkpoints and reject subsequent writes/recovery until retry completes; 108 shared fault schedules and eleven real-filesystem version tests verify it. C# fences pending named version writes and both native languages reject recovery over a live owner. C# now retains known synchronous checkpoint ownership through failed persistence; 96 new phase/entry-point cases verify retries and five write fences. Sixteen additional C# and six real-filesystem Rust regressions cover named writes during failed cleanup. All fifty C# version-publication cases now also require checkpoint completion. Noncontiguous recovery now replays the committed suffix in order and retains its oldest gap until redo and pending undo are durable. Thirty-nine mapped C# and six portable Rust contracts cover real async gaps and interrupted recovery; six more C# regressions verify sized/chunked retention checkpoints current effects without stale redo. Full native/C#/actual-WASM suites pass. Uncertain append/first-flush attempts now retain commit ownership, fence writes and resolve through cleanup without duplicate decisions. Ninety-six C# cases, 88 shared Rust fault schedules and eleven filesystem version functions verify reread failures, repeated cleanup, publication retry and following edits. Automatic version writes now retain an implicit publication owner through failed decisions, checkpoints and metadata writes. Thirty-two C# cases and eleven Rust functions first fail; repeated cleanup, all existing write fences, exact native reopening and following commits now pass. Eighteen C# pre-commit controls verify ordinary/failed undo, an async control verifies queued completion, and four Rust controls also execute on actual WASM. Persisted publication intents now recover exact branch attribution across abrupt exit; six real child cases per native language and before/after failure matrices pass focused tests. Full suites pass (672 Rust, 1,440 C# per configuration, 356 actual-WASM); the ten Windows byte-snapshot sharing violations are corrected and all nine workflows pass on 1f742c5. Three new regressions per native language reproduce silent skipping of missing retained history. Both implementations now preflight all changed undo/redo before mutating rows; no-op and available suffix controls, following edits, native reopening and actual-WASM contracts pass. Full suites pass (675 Rust, 1,443 C# per configuration, 359 actual-WASM). Final signed version attribution terminates without overflow, and native initial histories use actual committed records rather than inventing gap or aborted versions; finite before/after tests verify checkout and persistent reopening. Nine additional cases per native language exclude explicit/recovered aborts from initial heads and reachable paths, reject older sidecars that attribute known uncommitted records, and reject construction over an open inner owner. Native reopening and actual-WASM contracts pass; invalid files are preserved without claiming to repair previously altered data. Shared versioned branch-name markers and final-delimiter tag parsing preserve accepted delimiter/Unicode/NUL/newline names, nested ancestry and checkout across reopening; shared malformed/legacy vectors verify compatibility. Existing branch/tag behavior remains available. |
| R4 | Replaceable distributed synchronization decorators | Common log contract, local-only, asynchronous catch-up and synchronous replica acknowledgements; explicit unknown outcomes, prefix validation and retry tests. | Fixed-primary decorators implemented in all languages. Partition/append/flush failures, divergent prefixes and idempotent catch-up tested. The optional atomic journal store and etcd JSON gateway now coordinate named shards; actual three-member leader loss, majority loss and lost-response recovery are tested. etcd owns leadership and membership. |
| R5 | Study competitor source and best strategies | Primary documentation and source survey covering SQLite, PostgreSQL, RocksDB, Redis, etcd and TiKV; costs/guarantee comparison and reusable component evaluation. | In progress; expanded primary-source audit and implemented independent-session group commit distinguish InnoDB flush sharing, MongoDB durable acknowledgement, Cassandra ordinary quorum/repair and current Accord routing; see RESEARCH.md |
| R6 | Same API / costs only for selected guarantees | Keep existing bare-store cache path; add optional control/history/journal/replication composition with explicit complexity and allocation costs. | Existing raw interfaces implemented by staged sessions; optional journals, delta encoding and replica decorators supplied. Bare stores incur no engine overhead. Delta encoding reduces physical bytes for localized edits; full-state CPU/cloning costs remain documented. Serializable read-set validation is independently selected and tracks only observed addresses; scans retain revision checks. Conflict metadata is skipped for exclusive/optimistic whole-revision serializable modes, allocated lazily for overlapping sessions that need it, and reclaimed after the last validation. Optional point-write ownership allocates only for selected sessions/addresses, releases on every terminal path and rejects use in uncoordinated cluster workspaces. Finer independent validation/history decorator composition remains to study. |
| R7 | Implement all supported languages | Native Rust and C# library surfaces, portable JS/TS surface and declared backend capabilities; shared wire vectors and scenario tests. | Staged engine surfaces implemented. Compiled WASM conformance passes Node, Bun and Deno; shared fixtures are consumed by Rust, C# and actual WASM. Ordinary legacy transactions/history now execute in actual WASM using portable timestamps; shared fault/recovery contracts verify checkpoint ownership. Newly added distributed strategies must maintain parity. |
| R8 | 100% test coverage, edge cases, unit + integration guarantees | Measure coverage by module/language; add deterministic faults, concurrent schedules, real process/filesystem checks; retain coverage artifacts and disclose uncovered branches. A test count is not coverage. | JS production modules measure 100% lines, branches and functions; CI enforces all three thresholds. Actual WASM wrapper source reaches 100% lines, functions and reported branches in 446 actual WASM tests, including shared native engine, model, query and storage contracts; complete linked coverage measures 69.92% lines / 71.88% branches and remains below 100%. Native Rust/C# project coverage remains below the required scope. |
| R9 | Document each scenario | Runnable cache, durable store and replicated/sharded examples with failure semantics, ownership and persistence settings. | docs/ENGINE-STRATEGIES.md, docs/ENGINE-CLUSTERS.md and Rust/C#/JS examples cover implemented options, including consensus-backed shard groups. Further deployment strategies need their own recipes. |
| R10 | Horizontal cluster, shared and shared-nothing options | TCP/advisory shared stores, independent shard namespaces, replicated logs and consensus-backed atomic shard groups. | Shared coordination and immutable journals on independently routed data groups implemented in all languages. Real leader failover, data-group outage, atomic pointer publication and native/WASM interoperability verified. Catalog throughput, whole-journal transfers and object retention limit scale; automatic balancing and multi-catalog transactions are not supplied. |
| R11 | Competitor comparisons/configuration parity | Map supported settings and list unsupported guarantees explicitly with primary references. | Comparison table, pinned SQLite/PostgreSQL/RocksDB/Redis/etcd/TiKV/FoundationDB/MySQL/MongoDB/Cassandra/Accord source studies and Iceberg atomic metadata analysis supplied. Consensus and cross-shard coordination implemented. Efficient WAL/LSM structures and other listed algorithm gaps remain unfinished. |
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
