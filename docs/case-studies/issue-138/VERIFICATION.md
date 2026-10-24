# Verification record

This is evidence for [PR 139](https://github.com/link-foundation/link-cli/pull/139),
not a claim that the open-ended issue is complete. Commands run with one Cargo
build job and reduced debug information under a 2.92 GiB container limit.

## Reproductions and fixes

- `acid_readiness_tests.rs` and `AcidReadinessTests.cs` reproduce acknowledged
  commits lost without an explicit save, rollback persistence, checkpoint faults,
  invalid recovery markers and malformed transition records. Journal flush now
  precedes acknowledgement; data checkpoint flush precedes applied markers.
- C# Windows CI 40107157910 failed because checkpoint flush reopened an exclusively
  mapped file. The owned mapping/stream fix passes all three platforms in run
  40107962518 (562 tests per platform). Before/after excerpts are in
  `dev/log/issues/138/pulls/139/`.
- Rust Windows CI 40107962488 failed in two new durability tests with OS error 5
  (`Access is denied`). `File::open` supplied a read-only handle to `sync_all`.
  Commit 126168f uses a writable handle; run 40109338737 passes on Windows,
  Linux and macOS, including both previously failing tests.
- Shared journal vectors reproduced language differences: Rust accepted a leading
  plus in numbers and hex names, while .NET accepted some whitespace/trailing NUL
  forms. ASCII decimal/hex validation now precedes numeric decoding. The independent
  generator is `experiments/issue-138/generate-vectors.py`.
- A mocked short filesystem read reproduced loss of complete JS journal records.
  `FileCommitLog` now loops until each requested read completes and rejects zero
  progress; tests also inject partial/zero writes, truncation and constructor flush
  failure. The before log is preserved alongside other regression excerpts.
- A malformed subprocess startup banner leaked its owned server. A real finite
  child process reproduced the leak; address validation now occurs inside the
  startup cleanup boundary. Failure, cancellation, forced termination, protocol
  mismatch and ownership tests also exercise the process adapter.

- Staged-engine branch tests initially fail to compile because `fork` and
  `resume_branch` are absent (`engine-branch-before.log`). The implemented
  row-level three-way merge preserves independent changes, rejects overlapping
  changes, and applies parent constraints and admission rules. Flush failure
  poisons the parent; reopen plus a saved merge base discovers the durable
  result without adding a duplicate revision. Retention eviction and allocator
  recovery are covered in native tests and actual WASM conformance.
- C# release metadata CI 40109831621 rejected two changeset files. They are
  consolidated into one minor-release changeset; run 40110429498 passes. WASM
  CI 40109831578 rejected the bootstrap root `.gitkeep`; removing it makes the
  repository layout tests pass in run 40110429487.

## Local measurements before coordinated engines (2026-10-24, aef41c7)

| Surface | Check | Result and limits |
| --- | --- | --- |
| Rust | `cargo llvm-cov --all-features --json` | 394 pass, 2 subprocess entry points ignored; 7,728/8,950 lines (86.35%), 1,027/1,237 functions (83.02%). Branch instrumentation was not enabled; zero reported branches means unmeasured. |
| C# | `dotnet test --collect:"XPlat Code Coverage"` | 593 pass, 0 fail; 5,396/6,084 lines (88.69%), 2,186/2,760 branches (79.20%). Uncovered engine and legacy branches remain. |
| JS unit tests | `node --test --test-concurrency=1 --experimental-test-coverage --test-coverage-include='js/client/src/**'` | 69 pass; production modules: 100% lines, 99.61% branches, 99.20% functions. Uncovered branches/functions remain; this does not measure the WASM core. |
| Actual WASM | Existing conformance runner on Node, Bun and Deno | Shared vectors, staged request ordering, isolation, history, conflicts, rollback, Unicode, custom logs, replication partition recovery and branch merge/recovery pass. This is integration evidence, not a WASM coverage measurement. |

The full Node conformance run against the actual Rust server, C# server and WASM
reports 32 passed, 0 failed, 4 skipped (the skips are process-only scenarios for
in-process WASM). Three additional native Rust session API tests pass. Rust and
WASM Clippy deny warnings; .NET formatting verification and TypeScript checking
pass.

Coverage reports are saved under `experiments/issue-138/` locally and the Rust
workflow now preserves a coverage artifact. Retention, named API, recovery,
concurrent schedules and process-exit probes use finite inputs. Process exit
demonstrates recovery without destructors; it does not simulate a power cut or
prove the filesystem/controller's persistence guarantees.

## Acceptance gaps

The 100% coverage requirement is unmet. The optional etcd backend now supplies routed named-shard groups, consensus
failover and atomic cross-shard coordination. The three-member experiment passes
with native Rust/C# and actual WASM writers, lost responses, leader termination
and majority loss. The partitioned decorator additionally separates immutable
journals into independent data groups. Native/WASM coverage and efficient
LSM storage and full-state CPU costs remain unfinished. Optional delta encoding
now reduces physical journal bytes without removing those CPU costs.
Fixed-primary replica acknowledgements do not establish consensus.
See REQUIREMENTS.md and ENGINE-STRATEGIES.md for the implementation boundary.

## Actual WASM coverage and object-only configuration

The expanded `wasm-bindgen-test` suite runs 15 tests inside actual WebAssembly,
including JavaScript adapter throws and unexpected asynchronous return values,
uncertain commit recovery, option validation, branch conflicts, and protocol
operations. It reproduced a configuration inconsistency: Serde's struct decoder
accepted `[]` as default options. The public engine binding now requires a JSON
object before decoding its policies; native and WASM regression tests reject
positional arrays.

`experiments/issue-138/wasm-coverage.sh` collects LLVM execution coverage, including
branch instrumentation. The three wrapper source modules measure 327/327 lines,
68/68 functions and 16/16 reported branch outcomes. Regions measure 499/499;
reported branches do not cover every Rust match arm or every generated binding.
The report also includes linked core code, which remains substantially below
100%. Wrapper coverage must not be presented as whole-engine coverage. The
WebAssembly workflow now preserves the complete JSON report independently of
the normal stable-toolchain build and its Node/Bun/Deno integration tests.

The coverage runtime has a version constraint: `wasm-bindgen-test` 0.3.79 pins
`minicov` 0.3.8, which uses LLVM 22's profile layout. LLVM 23 instrumentation
reproduced a WASM runtime memory access error when capturing coverage. Only the
coverage script pins Rust 1.98.0 / LLVM 22, and the cdylib links minicov only under
the experimental coverage cfg. Normal production builds continue using stable.
See the [wasm-bindgen coverage guide](https://wasm-bindgen.github.io/wasm-bindgen/wasm-bindgen-test/coverage.html)
and [the pinned test runtime manifest](https://docs.rs/crate/wasm-bindgen-test/0.3.79/source/Cargo.toml.orig).


## Coordinated network verification

`engine_cluster_tests.rs`, `EngineClusterTests.cs`, and actual-WASM cluster
conformance cover all-or-nothing groups, read-only dependency validation,
independent concurrent shard commits, rollback and fresh discovery of uncertain
commits. Native HTTP tests and JS fetch tests cover gateway serialization,
strict generations, default false omission, safe read failover, one-attempt
mutations, malformed replies and byte limits. JS lifecycle regression tests
reproduce early close and missing custom-I/O outcome classification before fixes.
The JS cluster module measures 100% lines, branches and functions in unit tests.
That module measurement does not establish 100% project coverage.

`experiments/issue-138/etcd-cluster.mjs` launches three actual etcd v3.7.3
members with bounded quotas and memory targets. `cluster-etcd-interop.log` records
successful native Rust, C# and actual WASM writes into the same namespace,
a rejected stale group, lost-response outcome discovery, elected leader loss
with a surviving-majority commit, and refusal of fresh reads after majority loss.
CI runs this same experiment after verifying the official release archive hash.
No other node's local filesystem is shared by these three members.


The coordinated-engine change passes the full Rust suite (401 unit/integration
checks plus six documentation checks, two subprocess helpers and one doc example
ignored) and two additional named-API/constraint regression tests. The current
C# suite passes 604 tests, measuring 5,539/6,247 lines (88.66%) and 2,283/2,880
branches (79.27%). Actual WASM conformance, including coordinated groups, passes
on Node, Bun and Deno. Native and WASM Clippy, formatting and TypeScript checks
pass. These measurements still leave the project-wide 100% requirement unmet.

## Complete JS module coverage and deadline regression

The complete JS unit suite now passes 84 tests and measures 100% lines, branches
and functions across all 15 production modules. The CI unit job enforces all
three thresholds and includes `src/**`. These are V8 execution measurements for
the JS package, not execution coverage for the underlying WASM/native engines.

A deterministic clock reproduces a TCP deadline expiring while its cancellation
watcher is being registered. Previously the callback removed a different queued
request, then transmitted the expired request despite reporting `not-sent`.
Queueing the entry before registering its watcher preserves both requests'
semantics. The regression checks the actual next frame written to the socket.
Before/after excerpts and the full coverage summary are preserved with the PR's
other evidence. Additional tests verify current-revision snapshots, authoritative
replica reads, and cancellation before a queued WASM request reaches native code.

JS conformance run 40113225861 passed native/WASM writes but failed at member
status discovery with `ECONNREFUSED` (saved log lines 1045–1062). The harness had
waited for a majority, then queried every member. It now waits for each member's
status and nonzero leader before testing. A deterministic delayed-member test
covers refused connections, HTTP failure and election delay, plus finite failure.
The full three-member experiment passes locally after this correction, including
native writers, leader termination and majority loss. Fresh JS conformance run 40113656214 passes after this correction.

Full Rust coverage on 5155c80 passes 403 unit/integration tests (two subprocess
helpers ignored) and measures 8,095/9,351 lines (86.57%) and 1,084/1,312 functions
(82.62%). Branch instrumentation is not enabled. This measurement precedes the
partitioned-data decorator and leaves native coverage work open.

## Independent data groups

The partitioned-store regression first fails because the feature is absent.
Native Rust and C# tests then exercise the same atomic pointer publication,
read-only dependency conflicts, object reuse, failed staging, uncertain catalog
responses and missing/modified object rejection as JS. Shared portable fixtures
verify byte-length framing, empty records, NULs and multibyte UTF-8 addresses.

The real integration launches five bounded etcd processes: one three-member
catalog and two separate single-member data groups, each with its own directories
and consensus identity. Native Rust/C# examples and actual WASM use the same
pointer and object representations. The catalog contains only four-string
pointers. Killing the west group makes a multi-shard commit fail without changing
either pointer; east-only transactions continue to commit and read back. The
existing catalog leader-loss/majority-loss checks also pass. Single-member data
groups verify separate storage/failure domains, not production redundancy.

The JS suite, including the new decorator, passes 89 tests at 100% measured lines,
branches and functions across all 16 production modules, with enforced thresholds.
Actual WASM conformance passes Node, Bun and Deno. The full Rust suite passes
406 unit/integration checks plus six documentation checks; the additional
portable-address fixture then passes in the focused four-test suite. C# passes
607 full-suite tests followed by its four-test partitioned-store suite including
the new fixture. Its coverage before that fixture is 5,605/6,313 lines (88.78%)
and 2,334/2,938 branches (79.44%). Native coverage remains below the issue's
requested 100% scope.

## Native network failure-path coverage

The expanded suites pass 416 Rust unit/integration tests (two subprocess helper
entry points ignored) and 620 C# tests. New cases verify replica read/append/flush
faults, poisoned coordinator locks, concurrent immutable-object creation,
malformed metadata, request-body cancellation and the complete shard facade.
The stalled-body test has a finite fallback so a lost cancellation cannot hang.

Rust branch instrumentation is now enabled explicitly with `RUSTC_BOOTSTRAP=1`
and `cargo llvm-cov --all-features --branch`. This unstable measurement option
is confined to the coverage job; ordinary builds retain stable compiler settings.
The measured result is 8,283/9,509 lines (87.11%), 1,287/1,708 branches (75.35%)
and 1,105/1,326 functions (83.33%). Prior reports without branch instrumentation
must not be interpreted as branch coverage. The cluster coordinator measures
100% branches, the partitioned store 95.45%, and replication 72.73%.

C# measures 5,621/6,313 lines (89.03%) and 2,366/2,938 branches (80.53%).
`ClusterEngine`, `ClusterTransaction`, `ClusterShardSession` and
`PartitionedJournalStore` each measure 100% lines and branches. `EtcdJournalStore`
measures 98.87% lines and 100% branches. Project coverage still falls short of
100%; these module results do not remove that acceptance gap. The 168 workflow
policy tests, native Clippy and formatting checks pass.

The subsequent strict Rust etcd wire tests raise the full-suite count to 419 and
coverage to 8,299/9,509 lines (87.28%), 1,299/1,708 branches (76.05%) and
1,115/1,326 functions (84.09%). The etcd adapter measures 194/199 lines and 45/48
branches. Malformed JSON/protobuf shapes, noncanonical base64, missing values,
generation bounds and constructor validation use bounded real TCP peers. Actual
release WASM conformance passes Node, Bun and Deno after the options fix. Native
wrapper tests, native/target WASM Clippy and all 168 workflow policy tests pass.


## Optional physical delta journals

The adapter tests first fail with the missing Rust/C#/JS API. Five valid and 38
invalid independently generated sequences then verify the common `delta1`
format, checksums, canonical bounds, checkpoint fallback and exact replay. Fault
injection covers append-before-write, append-after-write, flush and failed
rereads. The reread regression fails before readiness is invalidated: it reports
"Missing expected exception" while appending against an unvalidated cached base.
All three implementations now require successful revalidation after a read error.

The final local suites pass 427 Rust checks (two subprocess entry points ignored),
629 C# tests and 95 JS unit tests. Native Clippy denies warnings, Rust/.NET
formatting checks pass, and the TS usage test composes the new adapter. Actual
release WASM conformance passes Node, Bun and Deno. A reusable shared-file probe
runs Rust, C# and actual WASM sequentially against the same physical journal:
six revisions, 203 rows and five deltas recover with full historical snapshots.
The rebuilt actual WASM coverage suite passes all 15 tests.

The finite actual-WASM byte experiment retains identical recovered history for
1,000 initial rows and ten localized updates. Full records total 109,276 bytes;
deltas total 10,697, a 90.21% reduction. The last update uses 77 versus 9,936
bytes. These figures exclude filesystem framing and measure neither latency nor
CPU cost. Full-state engine cloning, encoding and replay still cost O(N).

The delta adapter measures 201/201 lines, 26/26 functions and 54/54 instrumented
branches in Rust, and 100% lines and branches in C#. The full Rust report measures
8,500/9,710 lines (87.54%), 1,353/1,762 branches (76.79%) and 1,141/1,352 functions
(84.39%). C# measures 5,723/6,412 lines (89.25%) and 2,444/3,016 branches (81.03%).
All 17 JS production modules measure 100% lines, branches and functions with
thresholds enforced. Actual WASM wrapper source measures 327/327 lines,
68/68 functions and 16/16 reported branches; these narrow binding measurements
do not imply complete linked-core coverage. Native and linked-core 100% remain
unfinished. Curated reproduction and measurement evidence is retained in
`dev/log/issues/138/pulls/139/delta-journals.txt`.


## Conflict metadata selected by strategy

Before the optimization, all four Rust metadata tests and ten C# cases fail:
a 128-row seed retains 128 generations even with one writer and after reopening.
The generation map now starts empty. Only overlapping optimistic read-committed
or snapshot sessions track changed addresses; exclusive and whole-revision
serializable strategies skip it. Cleanup follows the last session's validation,
so ABA/delete-recreate and branch publication still invalidate stale writers.
Read-only commit, conflict, rollback and drop reclaim bookkeeping. C# allocates
the dictionary lazily and releases its backing storage, rather than just clearing
entries. Continuous overlap can still retain a large set until sessions end.

The full local suites pass 431 Rust and 639 C# tests. Rust/native-WASM Clippy and
both formatters pass. Actual WASM conformance passes the same ABA, disjoint-write,
branch and reopen schedules in Node, Bun and Deno. The instrumented WASM suite
passes 16 tests, with wrapper source still at 100% lines, functions and reported
branches. No newly excluded production paths are used for these measurements.

Full native Rust measures 8,514/9,718 lines (87.61%), 1,365/1,776 branches (76.86%)
and 1,141/1,352 functions (84.39%). C# measures 5,730/6,419 lines (89.27%) and
2,457/3,028 branches (81.14%). JS source is unchanged from its 17-module, 95-test
100% measurement. Native and linked-core 100% coverage remains unfinished.

## Serializable point-read validation

Before implementation, the actual WASM binding rejects `serializableValidation`
as an unknown field on commit `6d240a9`. Native tests fail to compile against the
missing enum/option. Optional read sets now preserve point dependencies, including
missing rows, ABA and branch publications. Scans retain revision validation;
exclusive admission fences publishers and allocates no read set. The default
revision validator is unchanged.

Each native implementation checks 1,296 small concurrent histories against a
separate serial-execution oracle. Targeted cases cover own writes/deletes,
blind allocation collisions, read-only commits and conflict retries. Rust also
checks shared point reads without losing the public type's Send/Sync bounds and
refuses publication after an injected read-set mutex failure. Bookkeeping is
reclaimed after successful or failed validation and rollback.

The actual WASM test suite has 17 tests, including binding options, disjoint
point writes, read/write cycles, scans and missing-row conflicts. The shared
Node/Bun/Deno schedules additionally cover read-only, ABA and branch conflicts.
Wrapper coverage measures 331/331 lines, 68/68 functions, 16/16 reported branches
and 505/505 regions. Linked-core coverage remains below 100%.
A finite acceptance probe opens 32 concurrent sessions editing separate profiles:
revision validation accepts one and aborts 31; read-set validation accepts all 32.
This is conflict acceptance, not a throughput/latency benchmark.

The full Rust suite passes 440 tests, with two subprocess helper entry points
ignored. Native coverage measures 8,576/9,767 lines (87.81%), 1,384/1,802 branches
(76.80%), 1,151/1,359 functions and 13,116/15,441 regions. No coverage exclusions
were added. The new read-set data type has all six executable lines and all three
regions covered; generic instantiation coverage is separately below 100%.

C# Debug and Release suites each pass 649 tests. Debug measures 5,746/6,435 lines
(89.29%) and 2,494/3,060 branches (81.50%). Release measures 4,377/4,810 lines
(91.00%) and 2,473/3,041 branches (81.32%); optimized sequence points change the
denominator. Compare configurations separately. `LinksEngine` and `EngineSession`
reach 100% lines/branches in both configurations; project coverage remains below
100%. Formatting, release example and TypeScript checks pass. JS remains 95 tests
across 17 production modules with 100% lines/branches/functions.

Exact-SHA CI for `e2e714f` finishes with 37 successful checks, ten skipped and
one neutral. This verifies the read-set commit independently of earlier runs.

See [validation choices and costs](../../ENGINE-VALIDATION.md),
[pinned FoundationDB source observations](RESEARCH.md),
[execution plan](READSET-PLAN.md), and the committed acceptance probe under
`experiments/issue-138/readset-contention.mjs`. Full-state O(N) work, conservative
scans and the separate distributed shard conflict boundary remain explicit limits.

## Diagnostic faults and finite recovery boundaries

The next boundary probe reproduces a separate defect on `e2e714f`: directing
stderr to `/dev/full` makes Rust engine commit and replica catch-up children
exit 101 after the first revision is already persisted. `eprintln!` can panic
after successful storage work and poison the engine mutex. Best-effort writes
now preserve the storage outcome. Both subprocesses complete two commits and
the reopened journals contain two rows. The failing destination is immediate
and bounded; the test does not fill a disk. It runs on Linux, while the other
boundary tests remain portable.

New native cases reject individually valid but noncontiguous/backwards journals
and forged branch merge bases; check UInt128/u128 allocator and revision limits
without long loops; verify log-operation fencing and closed read visibility;
and exercise poisoned-mutex recovery. Bounded network tests check custom header
forwarding and malformed headers, wrong object prefixes and unavailable object
staging without catalog publication. Default trait and real adapter contracts
check filtering, observation/error callbacks, search/get-or-create, sidecar
external changes, flush retry and deterministic pinned addresses.

The full native suites pass 462 Rust tests (three subprocess helper entry points
ignored) and 656 C# tests in each of Debug and Release. Rust measures
8,760/9,774 lines (89.63%), 1,409/1,806 branches (78.02%), 1,193/1,360 functions
and 13,345/15,446 regions. C# Debug measures 5,746/6,435 lines (89.29%) and
2,495/3,060 branches (81.54%); Release measures 4,377/4,810 lines (91.00%) and
2,474/3,041 branches (81.35%). Configuration denominators remain separate.

All 17 actual WASM tests pass. Wrapper source remains 331/331 lines,
68/68 functions, 16/16 reported branches and 505/505 regions. The complete report,
including linked core, measures 2,725/8,948 lines (30.45%) and 392/1,261 branches
(31.09%); the unused native paths included in that report are not newly excluded.
Native and WASM Clippy deny warnings; Rust and C# formatting verification passes.

The execution checklist and platform scope are recorded in
[BOUNDARY-PLAN.md](BOUNDARY-PLAN.md), with curated before/after evidence in
`dev/log/issues/138/pulls/139/diagnostic-boundaries.txt`. Coverage inclusions are
unchanged; the 100% acceptance criterion remains open.

## Existing decorator diagnostics and rollback retention

The next audit reproduces nine Rust storage/named/query/transaction/version
operations failing against `/dev/full` on `adb93d0`, plus the persistent-trigger
stdout path. A custom harness is necessary for stdout: the normal test harness
itself refuses to start when stdout returns ENOSPC. Restoring only the server's
old trace calls independently reproduces a failed real TCP request. Best-effort
diagnostics now preserve the original destination and opt-in flag while leaving
storage, journal and ordinary query output errors unchanged.

C# first reproduces 11 transaction/version/trigger failures, then 20 additional
named-store, simple-store, query and server failures through throwing writers.
The two server cases fault both before the request and after its mutation.
Console replacement is isolated from other collections and always restored.
All 31 diagnostic cases and eight retention cases pass after the changes.

Removing the diagnostic interruption exposes retention replaying rolled-back
creates. Two Rust tests reproduce this for sized and chunked policies; six C#
cases additionally reproduce updates and deletes. After the fix, both native
suites verify all three operation kinds, following commits, persistent reopen,
unmodified baseline rows and committed-only archive contents. Rollback records
are reclaimed without replay, and durable rollback markers remain in the sidecar.

A deterministic C# data-flush callback inserts a new pending transaction while
the previous asynchronous commit is being processed. Removing only the pending
guards makes both policies advance applied markers into uncommitted writes.
The guard leaves those records unmarked and unreclaimed until commit/rollback.
A bounded event and worker join provide completion without scheduler sleeps.

The full native Rust coverage suite passes 465 standard tests, with four child
entry points ignored, plus the separate stdout harness check. A subsequent
focused run verifies the added server fault mode. Coverage measures 8,833/9,800
lines (90.13%), 1,426/1,814 branches (78.61%), 1,193/1,360 functions and
13,390/15,467 regions. Native Clippy and formatting pass with the extended tests.

The final C# Debug and Release suites each pass 695 tests. Debug measures
5,775/6,444 lines (89.62%) and 2,531/3,068 branches (82.50%); Release measures
4,393/4,818 lines (91.18%) and 2,510/3,049 branches (82.32%). Configuration
denominators remain separate. C# formatting verification passes.

All 17 actual WASM tests pass. Wrapper coverage remains 331/331 lines,
68/68 functions, 16/16 reported branches and 505/505 regions. Including linked
core, the complete report measures 2,725/8,974 lines (30.37%), 392/1,269 branches
(30.89%), 346/1,296 functions and 3,938/14,112 regions. No new source exclusions
are used. Native and linked-core 100% coverage remains unfinished.

See [LEGACY-DIAGNOSTICS-PLAN.md](LEGACY-DIAGNOSTICS-PLAN.md) for the execution
checklist, fixture limits and retention semantics.

## Explicit-address writes and automatic failure cleanup

Four finite Rust regressions first reproduce unlogged explicit-address writes:
map and mapped allocations survive rollback, auto allocation has no journal
entry, and the version branch has no sequence for its allocation. Public
`ensure_created` now observes every backend allocation; recovery still calls
the inner backend directly. The map retains its target-only behavior and the
mapped backend retains its intermediate placeholders. Rollback and recovery
undo every recorded row. Existing and zero addresses remain no-ops.

Four additional regressions reproduce fallible transaction/version named-query
bridges panicking on checkpoint I/O errors. Their overrides now propagate the
error and validate reserved addresses before allocation. Three storage-fault
regressions reproduce stuck automatic transactions or a composite create's
leftover row. Automatic cleanup now releases admission and undoes that earlier
allocation; explicit transactions remain caller-owned for rollback.

The 12 explicit-address tests and five storage-failure tests pass, including
persisted pending data, repeated recovery, following writes, explicit version
commit/rollback and branch reopening. Two C# tests confirm its existing
callback-based explicit-address path already provides matching rollback,
durability and version behavior. Inputs request at most address eight.

The full Rust coverage suite passes 482 standard tests, with four child entry
points ignored, plus the separate stdout harness. Coverage measures
8,955/9,911 lines (90.35%), 1,441/1,832 branches (78.66%), 1,211/1,372 functions
and 13,587/15,647 regions. C# Debug and Release each pass 697 tests. Debug
measures 5,778/6,444 lines (89.66%) and 2,534/3,068 branches (82.59%); Release
measures 4,396/4,818 lines (91.24%) and 2,513/3,049 branches (82.42%).

See [ENSURE-PLAN.md](ENSURE-PLAN.md) and the curated evidence in
`dev/log/issues/138/pulls/139/explicit-address.txt`. The public observer default
costs O(N log N) time and O(N) rows; built-in map adapters observe directly.
Bare stores do not take that transaction path. Project-wide 100% coverage
remains unfinished.

All 17 actual WASM tests pass. Wrapper coverage remains 331/331 lines,
68/68 functions, 16/16 reported branches and 505/505 regions. Including linked
core, the report measures 2,729/9,085 lines (30.04%), 392/1,285 branches
(30.51%), 347/1,308 functions and 3,943/14,292 regions. No source exclusions
were added. Native/WASM Clippy deny warnings; Rust and .NET formatting pass.

## Observed partial writes and callback exceptions

Six new Rust regressions first fail after a storage adapter emits change
callbacks and then returns an error, or emits repeated create/update callbacks
before an explicit rollback. Fifteen C# regressions independently fail for
adapter errors and ordinary user callbacks throwing after mutation. Six further
C# cases check errors before mutation. The transaction wrappers now record
observed partial changes before propagating errors and roll back automatic work;
explicit work stays caller-owned. Repeated callbacks preserve the original
absent state, so rollback deletes a newly created row rather than retaining an
intermediate version. Tests check following transactions and persistent reopening.

The full Rust suite passes 488 standard tests plus the separate stdout harness,
with four child entry points ignored. Coverage measures 8,962/9,919 lines
(90.35%), 1,442/1,834 branches (78.63%), 1,211/1,372 functions and
13,594/15,662 regions. C# Debug and Release each pass 718 tests. Debug measures
5,793/6,447 lines (89.86%) and 2,538/3,066 branches (82.78%); Release measures
4,404/4,819 lines (91.39%) and 2,517/3,047 branches (82.61%). Percentages here
are calculated from integer counts rather than Coverlet's truncated rates.

See [PARTIAL-WRITES-PLAN.md](PARTIAL-WRITES-PLAN.md) and the curated evidence in
`dev/log/issues/138/pulls/139/partial-writes.txt`. Fixtures use at most three data
rows. This verifies Result/exception paths under the existing callback contract;
it does not establish Rust panic safety, silent-mutation recovery, or legacy
crash atomicity. Project-wide 100% coverage remains unfinished.

All 17 actual WASM tests pass. Wrapper coverage remains 331/331 lines,
68/68 functions, 16/16 branches and 505/505 regions. Including linked core,
the report measures 2,729/9,093 lines (30.01%), 392/1,287 branches (30.46%),
347/1,308 functions and 3,943/14,307 regions. Native/WASM Clippy deny warnings;
Rust and C# formatting checks pass. No source exclusions were added.

## Failed C# constructor ownership

Three regressions first reproduce owned typed/named/simple constructors retaining
the data handle when a directory blocks the names path. Immediate exclusive
reopening fails without any GC forcing. All three pass after guarded construction;
four borrowed cases preserve the caller's store, including the direct pinned
constructor. Two bounded memory adapters additionally fail before raw pointer
access and later during Unicode names initialization, checking cleanup before
the test's own disposal. All nine focused cases pass.

C# Debug and Release each pass 727 tests. Debug measures 5,821/6,482 lines
(89.80%) and 2,547/3,074 branches (82.86%); Release measures 4,423/4,843 lines
(91.33%) and 2,526/3,055 branches (82.68%). Formatting verification passes;
project-wide coverage remains below 100%, with no added source exclusions.
Rust and WASM production sources are unchanged from the preceding verified step.

See [CONSTRUCTOR-PLAN.md](CONSTRUCTOR-PLAN.md) and
`dev/log/issues/138/pulls/139/constructor-failure.txt` for reproduction and scope.

## Mapped cascades and journal append failures

Two no-fault Rust regressions first reproduce the mapped adapter omitting
dependent-row callbacks during cascading deletion and uniqueness merging.
Forwarding the upstream observations fixes the merge; deletion also exposes
empty intermediate slots allocated while undoing a higher address. Undo now
restores original contents even when such a slot already exists. Both tests
verify the complete baseline before and after persistent reopening.

With those adapter defects corrected, a two-row cascade reproduces ten Rust
journal-failure cases; two cases already pass. The independent C# fixture
observes both affected addresses and reproduces four missing commit-rejection
cases, with eight existing rollback cases passing. Both recorders now retain
the complete undo batch before attempting journal writes. An append error
requires rollback before further writes or commit. Automatic operations attempt
cleanup immediately; failed cleanup stays pending and can be retried through
the decorator, including the new C# `Rollback()` method.

The final focused fixtures cover 20 Rust journal cases, two mapped-observer
cases and 18 C# journal cases. They inject one failure before or after the first
or second append, check single-row allocation/update paths, and reject unsafe
commits. Rollback-marker failures retain admission until retry succeeds. State
assertions precede any redundant cleanup, so automatic rollback is verified
directly. Tests use at most four live rows and check subsequent writes and
persistent reopening where applicable.

The full Rust suite passes 510 standard tests and the separate stdout harness,
with four child entry points ignored. Coverage measures 9,030/10,005 lines
(90.25%), 1,449/1,840 branches (78.75%), 1,218/1,380 functions and
13,681/15,763 regions. C# Debug and Release each pass 745 tests. Debug measures
5,844/6,505 lines (89.84%) and 2,557/3,084 branches (82.91%); Release measures
4,435/4,855 lines (91.35%) and 2,536/3,065 branches (82.74%).

See [JOURNAL-FAILURE-PLAN.md](JOURNAL-FAILURE-PLAN.md) and curated before/after
evidence in `dev/log/issues/138/pulls/139/journal-failures.txt`. These tests verify
reported in-process failures under the observer contract; they do not establish
legacy crash atomicity for an unlogged mutation. The project-wide 100% coverage
criterion remains open, with no added source exclusions.

All 17 actual WASM tests pass. Wrapper source measures 331/331 lines,
68/68 functions, 16/16 reported branches and 505/505 regions. Including linked
core, the complete report measures 2,729/9,179 lines (29.73%), 392/1,293 branches
(30.32%), 347/1,316 functions and 3,943/14,408 regions. Native/WASM Clippy deny
warnings; Rust and C# formatting verification passes.

## Failed manual rollback admission

An ordinary explicit transaction can reach rollback with a complete journal,
then fail during undo or rollback persistence. Sixteen new tests in each native
language first reproduce accepting either commit or another write after that
failure. The earlier journal-error fence does not cover this path because no
append error preceded rollback. Both implementations now choose rollback-only
before beginning undo, retaining the abort decision until cleanup succeeds.

One-shot faults cover both sides of undo, data flush, rollback-marker append
and journal flush. Commit and write rejection have independent cases. All
focused cases pass, including exact baseline restoration on retry, a following
commit and persistent reopening. Fixtures start with two rows and do not crash
the process or stress memory. See [ROLLBACK-FAILURE-PLAN.md](ROLLBACK-FAILURE-PLAN.md)
and `dev/log/issues/138/pulls/139/rollback-failures.txt` for reproduction evidence.

The full Rust suite passes 526 standard tests and the separate stdout harness,
with four child entry points ignored. Coverage measures 9,033/10,008 lines
(90.26%), 1,450/1,842 branches (78.72%), 1,218/1,380 functions and
13,687/15,767 regions. C# Debug and Release each pass 761 tests. Debug measures
5,849/6,506 lines (89.90%) and 2,557/3,084 branches (82.91%); Release measures
4,439/4,856 lines (91.41%) and 2,536/3,065 branches (82.74%). Coverage remains
below the requested 100%; source inclusions are unchanged.

All 17 actual WASM tests pass. Wrapper source remains 331/331 lines,
68/68 functions, 16/16 reported branches and 505/505 regions. Including linked
core, the complete report measures 2,729/9,182 lines (29.72%), 392/1,295 branches
(30.27%), 347/1,316 functions and 3,943/14,412 regions. Native/WASM Clippy deny
warnings; Rust and C# formatting verification passes.

Commit `38eb3fb4cfa7e6c6e06b53159e1268d5115609c5` passes all exact-SHA CI
checks (37 successful, 10 skipped, one neutral), including Windows/Linux/macOS
native jobs and actual WASM coverage/integration. Rust run 40128031801, C# run
40128031799 and WASM run 40128031827 were created 2026-10-24 20:02:53 UTC.


## Mapped creation observations

Four Rust regressions first reproduce two creation defects: a duplicate's
surviving row is treated as newly allocated during rollback/recovery, and a
uniqueness rejection leaves its temporary allocation behind. The mapped adapter
now forwards all creation observations, including those preceding an error.
The transaction recorder retains that undo state before handling the outcome.
Two passing composite-creation controls and four equivalent C# controls preserve
existing behavior. Fixtures use three original rows and at most one temporary
row, verify exact state, persistent reopening and subsequent admission.

See [CREATE-OBSERVER-PLAN.md](CREATE-OBSERVER-PLAN.md) and curated reproduction
output in `dev/log/issues/138/pulls/139/creation-observations.txt`.

The full Rust suite passes 532 standard tests and the separate stdout harness,
with four child entry points ignored. Coverage measures 9,052/10,036 lines
(90.20%), 1,451/1,844 branches (78.69%), 1,220/1,383 functions and
13,714/15,807 regions. C# Debug and Release each pass 765 tests. Debug measures
5,849/6,506 lines (89.90%) and 2,557/3,084 branches (82.91%); Release measures
4,437/4,856 lines (91.37%) and 2,536/3,065 branches (82.74%).

All 17 actual WASM tests pass. Wrapper source remains 331/331 lines,
68/68 functions, 16/16 reported branches and 505/505 regions. Including linked
core, the complete report measures 2,729/9,210 lines (29.63%), 392/1,297 branches
(30.22%), 347/1,319 functions and 3,943/14,452 regions. Native/WASM Clippy deny
warnings; Rust and C# formatting verification passes. Project-wide coverage
remains below 100%, with no added source exclusions.


## C# CLI owner lifetime and coverage contracts

Five minimal cases first reproduce the real compiled CLI entry point returning
with mapped stores still open. The failed assertions request immediate exclusive
file access; finalizer forcing occurs only afterward for failing-test cleanup.
The command now owns every store in a using scope spanning construction and
execution. Wrappers stop before their borrowed stores close, and embedded
triggers retain a single base owner. Ten cases verify normal/error returns,
async mutation and failures constructing journal/version/trigger stores.

An independent 88-case enumerable fixture checks arities one through eight,
short/exact/long single-pass prefixes, bounded consumption and cleanup on error.
Four deterministic protocol cases inject transport failures during write/flush
and verify the category, original exception and progress. These contracts
already pass; no corresponding production bug is claimed. See
[CLI-LIFETIME-PLAN.md](CLI-LIFETIME-PLAN.md) and
`dev/log/issues/138/pulls/139/cli-lifetime.txt`.

Debug and Release each pass all 867 C# tests. Debug measures 5,929/6,485 lines
(91.43%) and 2,665/3,094 branches (86.13%); Release measures 4,495/4,835 lines
(92.97%) and 2,644/3,075 branches (85.98%), rounding the underlying counts.
The two transport catch lines are now exercised deterministically rather than
occasionally through TCP timing. No production source exclusions were added.
Rust and WASM production code is unchanged from the mapped-creation step.

C# formatting verification and the release-fragment validator pass. The bounded
LLVM generic-instantiation experiment and its control reproduce their documented
counts; see [COVERAGE-METHOD.md](COVERAGE-METHOD.md) for measurement semantics.

Mapped-creation commit `23644f1385a4b7710ee5eb34a0e307e3f55b94dd` passes all
exact-SHA CI checks (37 successful, 10 skipped, one neutral). Rust run
40128987034, C# run 40128987067 and WASM run 40128987143 were created
2026-10-24 20:15:47 UTC; Linux, Windows and macOS pass.

CLI ownership and coverage-method commit
`e295deebb2ef7b8093cd323e066382b7727ae71b` passes all exact-SHA CI checks
(36 successful, 11 skipped, one neutral), created 2026-10-24 20:21:49 UTC.
C# Debug/Release local verification and all three native CI platforms pass.


## Legacy signed sequence exhaustion

Four Rust regressions first panic at unchecked addition; four C# regressions
first accept writes without enough remaining positive signed sequence numbers.
Tiny valid boundary journals reproduce automatic/explicit creation and cascades.
Already-exhausted admission now fails before mutation. A larger observed batch
keeps complete unnumbered pending undo, rejects commit/further writes and never
publishes invalid numbers. Rollback restores all three baseline rows. The last
available number remains usable, and persistent reopening rejects further writes.
See [SEQUENCE-EXHAUSTION-PLAN.md](SEQUENCE-EXHAUSTION-PLAN.md) and
`dev/log/issues/138/pulls/139/sequence-exhaustion.txt`.

The complete Rust coverage suite passes 536 standard tests and the stdout
harness, with four child entry points ignored. Official LLVM totals measure
9,078/10,061 lines (90.23%), 1,460/1,852 branches (78.83%),
1,221/1,384 functions and 13,739/15,826 regions. C# Debug and Release each pass
871 tests. Debug measures 5,938/6,494 lines (91.44%) and 2,673/3,102 branches
(86.17%); Release measures 4,503/4,843 lines (92.98%) and 2,652/3,083 branches
(86.02%). Percentages round the underlying counts.

All 17 actual WASM tests pass. Wrapper source remains 331/331 lines,
68/68 functions, 16/16 reported branches and 505/505 regions. Including linked
core, the full report measures 2,729/9,235 lines (29.55%), 392/1,305 branches
(30.04%), 347/1,320 functions and 3,943/14,471 regions. Native and WASM Clippy,
Rust/C# formatting and the C# release-fragment validator pass. No production
source exclusions were added; project-wide coverage remains below 100%.


Sequence-exhaustion commit `3bb4790b3970a3ee9b4702b4b22eb1cf413fb105`
passes all exact-SHA CI checks: 37 successful, ten skipped and one neutral.
Runs were created 2026-10-24 20:40:36 UTC. Windows, Linux, macOS and actual
WASM coverage/integration all pass.

## Shared native contracts executed in WASM

Existing engine/model/protocol test bodies now register with wasm-bindgen-test
on WASM and retain native test registration elsewhere. Pure protocol options
are shared separately from native TCP helpers. Filesystem, process, native
thread and panic-unwind contracts remain native. The depth-limit test keeps
its finite 100,000-level rejection input on both targets; the WASM runner's
30-second deadline replaces only its unavailable native Instant measurement.
Production dependencies, behavior and coverage source inclusions are unchanged.

Actual WASM passes 142 tests: 59 shared engine contracts, 66 model/protocol
contracts, nine wrapper-engine and eight web tests. These include the finite
1,296-history serial oracle and shared wire vectors. The full native Rust
suite preserves 536 standard tests plus the stdout harness and four ignored
child entry points. Native/WASM Clippy with warnings denied and formatting pass.

The complete official LLVM report measures 3,786/9,657 lines (39.20%),
597/1,469 branches (40.64%), 514/1,368 functions and 5,484/15,071 regions.
The previous 17-test report measured 2,729/9,235 lines and 392/1,305 branches.
Additional generic instantiations enlarge the denominator. Wrapper source
remains 331/331 lines, 68/68 functions, 16/16 branches and 505/505 regions.
The linked engine subset measures 1,366/1,395 lines and 278/306 branches.
Complete coverage remains below the requested 100%, with no new exclusions.
See [WASM-CONTRACT-PLAN.md](WASM-CONTRACT-PLAN.md) and curated evidence in
`dev/log/issues/138/pulls/139/wasm-shared-contracts.txt`.


## Shared query and storage contracts executed in WASM

Another 126 existing native query, naming, Unicode, index, import and storage
contracts now run in actual WASM. A shared fixture retains native temporary-file
construction and ownership, and selects the existing in-memory backend on WASM.
Native-only filesystem persistence, revision and reload assertions are unchanged.
No production behavior or coverage source inclusions change.

All 268 actual WASM tests pass. The full native suite retains 536 standard tests,
the stdout harness and four ignored child entry points. Native/WASM Clippy with
warnings denied and both formatting checks pass. The final coverage run after
fixture deduplication measures 5,203/9,704 lines (53.62%), 884/1,521 branches
(58.12%), 742/1,372 functions and 7,677/15,141 regions. Wrapper source retains
331/331 lines, 68/68 functions, 16/16 branches and 505/505 regions. Linked engine
coverage remains 1,366/1,395 lines and 278/306 branches. Complete coverage remains
below the requested 100%; no denominator or source exclusions hide that gap.
See [WASM-STORAGE-PLAN.md](WASM-STORAGE-PLAN.md) and curated evidence in
`dev/log/issues/138/pulls/139/wasm-storage-contracts.txt`.

Shared-contract commit `71a8e980613615956efb058030dc28af6b7b7de0` passes all exact-SHA
CI checks: 37 successful, ten skipped and one neutral. Runs were created
2026-10-24 20:46:58 UTC; native Linux/Windows/macOS and actual WASM pass.

Shared query/storage commit `972d5c802603161ee6726ab919e0656f099a45d4` passes all
exact-SHA CI checks: 37 successful, ten skipped and one neutral. Runs were
created 2026-10-24 20:53:23 UTC; Linux, Windows, macOS and actual WASM pass.


## C# asynchronous shutdown and borrowed storage

Four bounded regressions first fail: one/two callers return while a persistence
callback remains active, and direct/Task.Run callbacks silently wait five seconds
trying to join their own worker. Shutdown now serializes callers and joins actual
work before returning or disposing its queue/token. It never holds the storage
lock while joining. Active worker context follows Task.Run descendants, and its
shared token expires after work so detached tasks may later shut down normally.
Calls from active worker context or a caller holding the storage lock throw
InvalidOperationException rather than waiting for their own work.

Seven focused lifetime/recovery controls pass, plus nine existing ACID-readiness
cases. Full validation caught five existing corrupt-recovery cases whose retained
facade disposed a partly constructed decorator; an initialized completed worker
makes that path safe before recovery has started real work. Full Debug and Release
each pass 878 tests. Debug measures 5,961/6,508 lines (91.59%) and 2,681/3,108
branches (86.26%); Release measures 4,521/4,856 lines (93.10%) and 2,660/3,089
branches (86.11%). These different sequence points retain the measurement method
already recorded above; project-wide coverage is still below 100%.

The decorator continues to borrow its stores. Callers finish foreground work,
stop the decorator, then close stores. Callbacks must return for shutdown to
complete. Cancelled queued work and failed checkpoints remain recoverable from
committed journal records; shutdown does not promise to drain cancelled work.
See [ASYNC-SHUTDOWN-PLAN.md](ASYNC-SHUTDOWN-PLAN.md), `csharp/README.md` and
`dev/log/issues/138/pulls/139/async-shutdown.txt` for evidence and ownership.


## Portable public recovery and enabled diagnostics

The public engine now executes every shared journal fixture on native and actual
WASM: three valid contiguous revisions and all 26 invalid u32 records. Assertions
cover Unicode names, deletion, empty names and final address allocation. Enabled
tracing preserves admission, private writes, old snapshots and recovered state
under four existing concurrency/validation configurations. No production behavior
or coverage inclusions change.

All 538 standard native tests and the stdout harness pass, with four child entry
points ignored. All 270 actual WASM tests pass. Native/WASM strict Clippy and
formatting pass. Complete official native LLVM coverage measures 9,078/10,061
lines (90.23%), 1,460/1,852 branches (78.83%), 1,221/1,384 functions and
13,736/15,826 regions. Complete WASM coverage measures 5,213/9,704 lines
(53.72%), 897/1,521 branches (58.97%), 746/1,372 functions and 7,693/15,141
regions. Wrapper coverage remains 331/331 lines, 68/68 functions, 16/16 branches
and 505/505 regions. Complete coverage remains below 100%.
See [PORTABLE-RECOVERY-PLAN.md](PORTABLE-RECOVERY-PLAN.md) and
`dev/log/issues/138/pulls/139/portable-recovery.txt` for evidence.

## Optional pessimistic point writes

The public concurrency enums initially lack the strategy, so the focused feature
contracts fail to compile before implementation. Optional point-write ownership
now rejects competing writes immediately while permitting independent addresses.
The existing session API, isolation choices and history/log composition remain
available. Acquisition validates generations under the publication mutex; branch
merges honor ownership. Every session outcome releases its addresses, including
constraint failures and uncertain journal writes. Unselected modes allocate no
ownership tracker or entries. Cluster workspaces explicitly reject local locks.
See [configuration and costs](../../ENGINE-PESSIMISTIC.md).

Nine shared Rust contracts run natively and in actual WASM. Sixteen C# contracts
also cover reflection-based private bookkeeping costs and cleanup. Both native
implementations compare 1,296 bounded histories with an independent serial oracle.
Failed busy writes retain any point reads they performed; tests exercise their
serializable dependency checks. Shared oracle execution is evidence for those
finite histories, not exhaustive ACID verification.

The full native suite passes 548 standard tests plus the custom stdout harness
(four child entry points ignored). All 279 actual WASM tests pass. Native/WASM
strict Clippy and formatting pass. Official native coverage is 9,143/10,126 lines
(90.29%) and 1,482/1,874 branches (79.08%). Official complete WASM coverage is
5,279/9,770 lines (54.03%) and 919/1,543 branches (59.56%). Wrapper coverage is
332/332 lines, 68/68 functions and 16/16 reported branches. The new ownership
module measures 29/29 lines and 8/8 branches on both targets. Complete project
coverage remains below 100%; no source exclusions were added.

The preceding async-shutdown and portable-recovery commits pass all nine
exact-SHA workflows. Portable recovery's link-check attempt received HTTP 502
from GitHub's existing release index (saved log lines 290–291, 311 and 315).
A direct request returned HTTP 200; unchanged workflow attempt two passed.
This did not require weaker checking or modified URLs. Evidence is preserved
in `dev/log/issues/138/pulls/139/portable-recovery.txt`.

C# passes 897 tests in Debug and Release. Debug measures 5988/6535 lines
(91.63%) and 2728/3156 branches (86.44%); Release measures
4547/4882 lines (93.14%) and 2708/3137 branches (86.32%). Different optimized
sequence points mean these are separate configurations. The C# ownership module
reaches 100% lines and branches in both.

The existing examples pass with `--pessimistic --read-set`. The same durable delta
journal is then opened sequentially by Rust, C# and WASM for two cycles; final
independent recovery verifies revision 22 with ten rows and names from previous
writers. Actual Node conformance passes all 32 Rust/C#/WASM scenarios (four
WASM process-only skips); Bun and Deno actual-WASM conformance also pass.
All 95 JS unit tests and TypeScript checks pass, with enforced 100% lines,
branches and functions across all 17 JS production modules. .NET format and
release metadata checks pass. Full evidence is in
`dev/log/issues/138/pulls/139/pessimistic-writes.txt`.

The published pessimistic-write commit `6087cfc3e01f8b8d953ce46b6c3184ae0d2859c1`
passes all nine exact-SHA workflows created 2026-10-24 21:24:47 UTC: 37 successful
checks, ten skipped and one neutral. Linux, Windows, macOS, actual WASM and shared
etcd conformance pass.

## C# whole-facade dependency lifetimes

Three initial runtime regressions fail because graph cleanup closes dependencies
before their borrowers. That includes a real checkpoint still using its mapped
data store and a borrower whose failed cleanup needs its dependency for retry.
The bounded integration probe always stops after managed flush and before native
applied-marker writes, then awaits both actual worker and disposer.

Cleanup now captures inner references before owner cleanup can detach them,
disposes borrowers first and shares cycle/duplicate detection across synchronous
nested calls. A borrower failure preserves dependencies for retry. Direct
transaction disposal still borrows stores; the explicit whole-graph helper owns
all reachable facades. Real version rollback, exclusive file reopening, cyclic
references, detached fields, corrupt construction and idempotence pass.

All six new contracts and 31 focused lifetime/ACID controls pass. Full Debug and
Release each pass 903 tests. Debug measures 5,999/6,542 lines (91.70%) and
2,732/3,158 branches (86.51%); Release measures 4,555/4,888 lines (93.19%) and
2,712/3,139 branches (86.40%). The disposer and its reference enumerator each
reach 100% measured lines/branches in both configurations. Full project coverage
remains below 100%. See [FACADE-LIFETIME-PLAN.md](FACADE-LIFETIME-PLAN.md) and
`dev/log/issues/138/pulls/139/facade-lifetime.txt` for the reproduction and ownership
boundary. Rust/JS/WASM production paths are unchanged in this step.

The published facade commit `421dd39fa42576d35fc43373bad309d7fba1fc65`
passes eight workflows but fails C# Windows run `40134580109` at the new test's
worker-join deadline. The downloaded full log and exact stack are preserved;
see `dev/log/issues/138/pulls/139/facade-lifetime-ci.txt`. The revised probe
removes its unrelated post-release native Flush, keeps the managed ownership
assertion and existing deadlines, fails against the old disposer and passes
ten finite corrected runs. Fresh Windows CI is required before closure.

## C# version admission after failed rollback

All 24 new pre-fix mapped-store cases fail because version Branch unexpectedly
succeeds after inner rollback fails. The version finally block drops admission
despite pending undo, and owner Dispose drops the reference needed for retry.
The fix retains that reference until inner rollback becomes terminal. Eight
before/after undo and persistence phases exercise explicit rollback, owner
disposal and whole-graph disposal. Every case verifies metadata/begin fencing,
rejected commit, retry through the same entry point, exact baseline/head recovery,
exclusive file reopening and a following committed/tagged transaction.

A real retention control also completes rollback, then fails checkpointing an
older committed transition. Its terminal inner owner must release admission
even though rollback reports that later error. All 25 cases and six facade
controls pass. Full final Debug and Release each pass 928 tests. Debug measures
6,014/6,542 lines (91.93%) and 2,742/3,162 branches (86.72%); Release measures
4,564/4,888 lines (93.37%) and 2,721/3,143 branches (86.57%). The version rollback
method reaches 100% measured lines and branches in both; complete project
coverage remains unfinished. Formatting and release metadata pass.

See [VERSION-ROLLBACK-PLAN.md](VERSION-ROLLBACK-PLAN.md) and
`dev/log/issues/138/pulls/139/version-rollback-owner.txt`. Rust already keeps its
active owner on failed rollback; its production path and JS/WASM are unchanged.

## Native CLI policy contracts

Ten contracts invoke the real binary for commit/retention policies, terminal
commands, change rendering, persistent branch/tag/checkout operations and
rejected-operation invariants. Actual text/binary clients exercise empty replies
against the existing in-process server framework. Network configuration and
server bind/query/protocol failures are covered without platform-specific error
strings. Every child has a finite deadline and is reaped on all paths.

The complete native suite passes 557 standard tests plus the custom stdout harness
(four child entry points ignored). The final added client contract and expanded
server cases then pass in the complete ten-test CLI target; its instrumented run
retains the full-suite profiles. Strict Clippy and formatting pass. Official LLVM
coverage is 9,269/10,126 lines (91.54%) and 1,524/1,874 branches (81.32%). Main is
377/387 lines, 112/114 branches and 34/34 functions. No source exclusions or
source-union replacement were used; complete project coverage remains unfinished.
This step preserves existing behavior and changes no Rust production code.
See [CLI-CONTRACT-PLAN.md](CLI-CONTRACT-PLAN.md) and
`dev/log/issues/138/pulls/139/cli-policy-contracts.txt`.
