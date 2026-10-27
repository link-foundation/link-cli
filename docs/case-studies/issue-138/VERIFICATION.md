# Verification record

The latest complete portable checkpoint batch passes every baseline device
operation with before/after faults, durable old/new reopening, following writes,
all truncated-root lengths, malformed checksummed roots, epoch boundaries and
single/mirrored reuse. Rust constructor and ordinary destruction preserve root
ownership until both banks are released; two before-failing controls verify it.
All 992 native tests plus six doctests, 597 actual WASM tests, 2,198 C# tests per
configuration, 226 JS tests and 221 repository checks pass. Each JS runtime also
passes 11 portable controls, actual-WASM conformance/48 policies and identical
36-case composition and payload reports. Strict formatting, Clippy, types,
optional-SQL, source-size, release and workflow checks pass. The initial Bun
harness used a plain script invocation; correcting it to Bun's native test runner
is not evidence of a product defect.

| Complete measured scope | Covered lines | Covered branches | Covered functions |
| --- | --- | --- | --- |
| Native Rust | 13,360/14,163 (94.33%) | 2,534/2,946 (86.01%) | 1,698/1,833 (92.64%) |
| Actual WASM, complete linked source | 10,831/13,240 (81.81%) | 2,149/2,597 (82.75%) | 1,371/1,756 (78.08%) |
| C# Debug | 8,195/8,490 (96.53%) | 4,489/4,856 (92.44%) | Not reported |
| C# Release | 6,286/6,473 (97.11%) | 4,436/4,821 (92.01%) | Not reported |

The root-format Rust component measures 58/58 lines, 22/22 branches and 7/7
functions in both builds. The two-bank Rust component measures 209/231 lines,
46/46 branches and 22/28 functions; private forwarding methods still have
uncovered paths. C# checkpoint decision/log classes measure 100% lines and
reported branches, while the optional interface's default capability getter
remains uncovered. JS complete production coverage remains 100% for all three
metrics, and WASM wrapper-only coverage remains 391/391 lines, 20/20 branches
and 76/76 functions. No denominator is excluded to meet a target.

LOCAL-CHECKPOINT-PLAN.md and local-checkpoint{,-before}.txt preserve exact scope
and evidence. This portable backend leaves C#/JS device cleanup with callers;
Rust owns supplied values and transfers cleanup responsibility on extraction.
Engine-lock publication, physical-delta replacement, native file ownership and
hardware power-loss verification remain unfinished. All nine workflows pass on
preceding head cdff1b4, including every native platform; checkpoint-paged-ci.txt
preserves the runs. This batch needs fresh exact-head CI, and every wider unmet
criterion remains active.

The preceding integration extension imports an independently generated revision-40
checkpoint into native pages, then passes all 48 physical/policy configurations
through Rust, Node, Bun, C#, Deno and Rust again. Its 288 writer invocations
finish at revision 52 with 44 rows, preserving exact checkpoint bytes, names,
retained history, pins and allocator gaps. Mirrored handoffs repair a damaged
closed copy. The ordinary revision-zero control independently passes 48/288.
This changes examples and CI, with no production page/engine/codec change and
no new full-coverage measurement. CHECKPOINT-PAGES-PLAN.md and checkpoint-paged.txt
record the before-example control, exact scope and local lint/build checks.

The preceding complete local validation corrects initial logical checkpoints rejected
by the independently selected physical delta codec. Independent vectors first
fail in Rust, C# and JS. Exact checkpoint bytes now survive, following complete
snapshots can compress, logical changes interrupt that dependency, and invalid
ordering rejects before append. Immutable pins, retained allocators, uncertainty
fences and both replication orders pass. See CHECKPOINT-DELTA-PLAN.md and
checkpoint-delta-before.txt.

Full suites pass 980 native Rust tests plus six doctests, 585 actual-WASM tests,
2,187 C# tests per Debug/Release configuration, 215 JS tests and 221 repository
checks. Node/Bun/Deno produce identical results for 36 checkpoint/delta/replica
compositions and 36 finite checkpoint payload measurements. Real etcd passes
default, checkpoint and checkpoint-with-cache modes with native/WASM handoffs
and consensus loss. All 48 existing physical/policy configurations pass 288
shared-file writers with mirrored repair. Those existing file fixtures start
at ordinary revision one; they do not prove persisted initial-checkpoint handoff.

| Complete measured scope | Covered lines | Covered branches | Covered functions |
| --- | --- | --- | --- |
| Native Rust | 13,093/13,871 (94.39%) | 2,466/2,878 (85.68%) | 1,669/1,797 (92.88%) |
| Actual WASM, complete linked source | 10,564/12,948 (81.59%) | 2,081/2,529 (82.29%) | 1,342/1,720 (78.02%) |
| C# Debug | 8,098/8,392 (96.50%) | 4,395/4,762 (92.29%) | Not reported |
| C# Release | 6,213/6,399 (97.09%) | 4,344/4,729 (91.86%) | Not reported |

The Rust delta module measures 224/224 lines, 72/72 branches and 26/26 functions
both natively and in actual WASM. C# DeltaCommitLog.cs measures 100% lines and
branches across all reported classes in both configurations. JS production
modules enforce 100% lines/branches/functions; WASM wrapper modules measure
391/391 lines, 20/20 branches and 76/76 functions. These component measurements
do not complete whole-project coverage. checkpoint-delta.txt preserves commands,
complete counts, exact costs and scope.

All nine workflows pass on preceding checkpoint head a329fa7, including native
Linux/macOS/Windows. Two initial C# Windows fixture cleanup failures pass on an
unchanged-code debug rerun; their original cause remains unknown. See
journal-checkpoint-ci.txt and checkpoint-ci-windows-before.txt. All nine workflows
also pass on delta-correction head 68fa923, including every native platform;
checkpoint-delta-ci.txt records exact heads and timestamps. Local atomic file replacement,
old object reclamation and every wider unmet acceptance criterion remain active.

The archive batch preserves all sixteen original native contracts: ten now run
both natively and inside actual WASM and six retain their native remote/CLI/file
coverage. Four new portable stream groups check every finite reader/writer fault
boundary in the 30-byte golden vector, truncation/trailing bytes, short I/O,
preserved rows/names and following use. A WASM-only control verifies unsupported
filesystem errors and successful following in-memory use. No new production
defect is claimed. Native archive lines/branches/functions reach 100%; actual
WASM measures 129/131 lines, 5/6 branches and 18/18 functions. Whole-project
coverage remains below target.

The preceding trigger/query composition batch passes all nine workflows at
c530a6a, including Linux/macOS/Windows; fallible-composition-ci.txt retains exact
evidence. All nine workflows also pass at preceding sorted-run head a1f12fec;
sorted-runs-ci.txt retains exact heads and timestamps.

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

## Version owner and CLI CI closure

Exact head `77bd2b7b95f8a39a0c7dd03ed301069af3877f3d` passes all nine
workflows created 2026-10-24T21:47:56Z. C# passes 928 tests on each of Windows,
Linux and macOS, including the revised facade lifetime probe. Rust passes all
three platforms, full native coverage and fresh-merge checks. This closes the
previous Windows probe failure without relaxing a deadline. Full IDs are in
`dev/log/issues/138/pulls/139/version-owner-ci.txt`. New source changes need
new exact-SHA checks. Complete native/linked-WASM coverage remains unfinished.

## C# committed version publication retry

Before the fix, 18 real mapped checkpoint cases leave head 2 after durable
commit 4. With metadata faults added, all 36 pre-fix cases fail: fallible
publication releases version admission too early. Two further before tests
reproduce dropped publication through Rollback and accepted subsequent writes.
Cleanup now finishes the known commit, keeps admission through failed metadata
writes and skips already named immutable markers during retry.

All 48 phase/entry-point cases verify fencing, exact recovered branch/head/data,
unique transition markers, rewind/forward checkout, exclusive file reopening
and a following committed/tagged transaction. Together with rollback/lifetime
controls, 79 focused cases pass. Full Debug and Release each pass 976 tests.
Debug: 6,027/6,555 lines (91.95%); 2,753/3,174 branches (86.74%). Release: 4,573/4,897 lines (93.38%); 2,732/3,155 branches (86.59%). Formatting and release metadata pass.
Complete coverage remains unfinished. These tests cover one-shot error/retry,
not abrupt process exit between journal and legacy sidecar publication. See
[VERSION-COMMIT-PLAN.md](VERSION-COMMIT-PLAN.md) and
`dev/log/issues/138/pulls/139/version-commit-publication.txt`.

Publication head `fa0838229e18184b677582b34483607dc2f76648` passes all nine
workflows created 2026-10-24T22:00:13Z. C# passes 976 tests on all three
platforms, and native Rust/actual WASM/JS, documentation, security, dependency
and workflow checks pass. IDs are in `dev/log/issues/138/pulls/139/version-commit-ci.txt`.

## Signed version boundaries and sparse histories

Before: all three finite C# cases fail at the transition marker budget
against published production. The corrected callback-address fixture again
fails all three, zero pass: version-sequence-corrected-before.log.
The original fixture used primitive Create's return value as an address;
that test mistake is preserved in version-sequence-focused.log and corrected.
No production recovery change was needed for that fixture mistake.
A guard rejects a ninth marker or the first nonpositive sequence, so even
the old signed overflow loop cannot exhaust memory or run indefinitely.
Rust's finite sparse-100 fixture fails before the fix because Checkout(3)
accepts an unrecorded version: version-sequence-rust-before.log, line 59.

The C# inclusive attribution loop no longer increments beyond long.MaxValue.
Both native default-history constructors now iterate actual retained records,
rather than every integer up to LastLoggedSequence. This avoids phantom gap
versions and work proportional to an arbitrary sparse sequence value.
Three cases per native language verify automatic/explicit final-number
writes, exhaustion admission, head/tag, sparse checkout and mapped reopening.
Two additional C# contracts cover async cancellation, transaction identity,
committed rollback/disposal, named forwarding and rejected metadata inputs.
Focused C#: 78 pass. Focused Rust version/sequence targets all pass.
Full C# Debug/Release: 981 pass each, zero skips.
Full native Rust: 561 standard tests pass, 4 child entry points ignored; custom stdout harness also passes.
Debug: 6,064/6,557 lines (92.48%); 2,762/3,174 branches (87.02%).
Release: 4,599/4,899 lines (93.88%); 2,741/3,155 branches (86.88%).
Native Rust: 9,277/10,127 lines (91.61%), 1,527/1,874 branches (81.48%), 1,238/1,388 functions (89.19%).
Strict Clippy (all features/targets), Rust formatting, .NET formatting and
minor release metadata validation pass. No coverage exclusions were added.
The staged JS/actual-WASM version API and code are unchanged by this step.
Complete native and linked WASM coverage remains below the requested 100%.
Logs and machine-readable reports remain under experiments/issue-138.

Sequence head `d23bc02260f6379d85ce6bed0b7e936241a7ce2f` passes all nine
workflows created 2026-10-24T22:12:10Z. C# passes 981 tests on all three
platforms; native Rust, actual WASM and the remaining checks pass. Run IDs are
in `dev/log/issues/138/pulls/139/version-sequence-ci.txt`.

## Unambiguous persisted version names

Before implementation: three independent cases per native language all fail.
C#: version-names-csharp-before.log lines 13,22,31: delimiter tag missing,
quoted branch has a changed parent, and delimiter branch is absent after reopen.
Rust: version-names-rust-before.log lines 14,22,29 shows the same independent
failures. No tests pass in either three-case before run.
A separate shared invalid-tag vector first demonstrates Rust accepting an
empty name: version-names-empty-tag-before.log lines 12–15, actual 7 tags vs 6.

New branch markers encode four hexadecimal digits per UTF-16 code unit and
retain an unambiguous null-parent field. Both readers retain ordinary legacy
records, including a legacy name beginning with the version-two prefix.
Tag readers split at the final equals sign and reject empty names. Shared
fixtures specify ten branch records, nineteen malformed branches, six tags
and six malformed tags. The protocol documents intrinsic ambiguity in older
separator-injected records and older binaries' inability to read new records.

Six real persistent scenarios per language verify exact names, nested ancestry,
head/tag values, current branch, main/child switching and rewind/forward rows.
Additional fixture/decoder tests cover punctuation, Unicode, NUL, newline,
empty versus null parent, signed bounds, malformed fields and legacy decoding.
C# also verifies exact unpaired UTF-16 code-unit encoding; Rust rejects invalid
UTF-16 because it cannot form a Rust string. Native database formats remain
separate; shared marker bytes and valid Unicode semantics match.

Full C# Debug and Release: 990 tests pass each, zero skips.
Full native Rust: 571 standard tests plus the custom stdout harness pass;
four subprocess entry points are ignored by the ordinary harness.
Actual WASM: all 279 tests pass; the three wrapper modules retain 100% lines,
functions and reported branches. These legacy format changes do not add a
new JavaScript wrapper API. Complete linked/native coverage remains unfinished.
Debug: 6,098/6,591 lines (92.52%); 2,792/3,198 branches (87.30%).
Release: 4,622/4,922 lines (93.90%); 2,771/3,179 branches (87.17%).
Native Rust: 9,363/10,211 lines (91.70%), 1,541/1,886 branches (81.71%), 1,247/1,397 functions (89.26%)
Complete linked WASM: 5,279/9,820 lines (53.76%), 919/1,553 branches (59.18%), 750/1,381 functions (54.31%)
The first strict lint run flagged constant-size chunks_exact iteration.
Using as_chunks::<4>() preserves the checked four-digit decoder semantics;
strict Clippy then passes, followed by a new full native coverage run and
actual WASM suite. .NET/Rust formatting and minor release metadata pass.
No coverage exclusions were added. Full logs/reports: experiments/issue-138.

## Known-commit owners and portable legacy timestamps

Before: eleven real-filesystem Rust version cases have ten failures and one
Commit control (version-commit-rust-before-all.log). Moving only the data
folder fails a checkpoint after the separately persisted journal commit;
cleanup restores the directory before assertions and repeats the fault once.

Nine generic Rust regressions first all fail (commit-checkpoint-rust-before.log).
The final shared native/actual-WASM matrix executes 108 schedules: six phases
before/after data flush, applied marker and final log flush; explicit and
automatic transactions; three cleanup and six write entry points. Retry must
finish the same commit, admit no later write and preserve a following edit
when reopened from separately persisted data and journal images. The normal
checkpoint remains O(changes), without a whole-log scan or pending-batch clone.

Version owners retain admission until checkpoint and branch publication finish.
Eleven real-filesystem cases verify repeated errors, cleanup through Commit,
Rollback and Save, eight write entry points, exact head/tag/main/topic rows,
applied markers and a following reopened transaction. Existing immutable
transition markers are reused during publication retry.

Both C# named-write cases first fail (version-name-fence-csharp-before.log).
The corrected fence and all 48 existing mapped phase/cleanup controls pass.
Two public recovery regressions per native language first fail; both recovery
layers now reject an open owner before clearing any live state. The shared
Rust recovery contracts also run in actual WASM.

Two actual-WASM clock regressions first panic in std::time::SystemTime::now
with "time not implemented on this platform" (portable-clock-wasm-before.log,
lines 14583-14706). web-time 1.1.0 preserves native std semantics and uses the
JavaScript wall clock in browser/Node WASM. Shared contracts verify Unix
milliseconds, distinct in-process ids, commit/rollback, delimiter/Unicode
branch/tag metadata and exact checkout rows. Twelve existing ordinary
transaction contracts and eleven history contracts now run unchanged on both
targets, with native file stores and WASM memory stores. Filesystem-only reopen
and archive cases remain native; no assertion was removed.

Full C# Debug and Release: 994 tests pass each, zero skips.
Full native Rust: 595 standard tests and the custom stdout harness pass;
four child entry points remain ignored by the ordinary harness.
Actual WASM: all 315 tests pass; wrapper lines/functions/reported branches
remain 100%. Strict Clippy on both targets, .NET/Rust formatting and release
metadata validation pass. No coverage exclusions or source-union totals added.
Debug: 6,115/6,608 lines (92.54%); 2,794/3,200 branches (87.31%).
Release: 4,634/4,934 lines (93.92%); 2,773/3,181 branches (87.17%).
Native Rust: 9,440/10,284 lines (91.79%), 1,558/1,906 branches (81.74%), 1,260/1,405 functions (89.68%)
Complete linked WASM: 6,540/9,893 lines (66.11%), 1,083/1,611 branches (67.23%), 900/1,389 functions (64.79%)

Complete project coverage remains below the required 100%. Abrupt exit between
legacy journal commit and version-sidecar publication is a separate unfinished
audit. These cleanup tests do not establish power-loss atomicity. The C# generic
checkpoint lifecycle is separately under investigation; the Rust retry change
does not imply that C# already has that guarantee.

## C# checkpoint retry and named-write ownership

Before: all 96 new C# checkpoint cases fail. Six phases inject errors before/
after data flush, applied-marker append and final journal flush. Explicit
transactions retry through Commit, CommitAsync, Rollback, Dispose and owner
Rollback; automatic transactions retry through owner Rollback. The final tests
repeat the error once and verify that admission remains fenced, cleanup finishes
the same decision, following writes survive persistent reopening, and names
remain exact. Sixty write cases cover Create/Update/Delete/SetName/RemoveName
with both explicit and automatic owners. The durable commit marker precedes
the injected checkpoint error; no duplicate commit decision is allowed.

Sixteen additional C# before-fix cases show named writes bypassing failed
rollback admission; sixteen existing row/commit controls pass. All thirty-two
now verify exact undo, allowed naming after cleanup and a following reopened
transaction. Six Rust real-filesystem cases likewise fail before their guard:
named writes bypass known-commit checkpoint and failed rollback ownership in
the transaction and version bridges. All six pass after sharing the existing
row-write guard. The data directory is restored before assertions/cleanup.

The C# version facade always calls inner Commit when retrying publication:
IsCommitted means a durable decision, not a completed checkpoint. All fifty
existing version commit/publication cases pass with an applied-sequence check.
Normal async commits still queue before releasing admission; shutdown/disposal
contracts remain covered by the full suite. No new copy or full-log scan is
added to ordinary synchronous commit.

Full-suite fixture updates preserve their original assertions. One verifies
repeated failing Dispose then repaired cleanup. The retention control now uses
a real failed asynchronous checkpoint, joins its borrower, then checks terminal
rollback admission. Its initial blocking wait passed isolated Release but failed
under the parallel suite; an awaited TaskCompletionSource removes that blocking
wait without increasing the five-second bound.

Full C# Debug/Release: 1,106 pass each, zero skips.
Full native Rust: 601 standard tests and the custom stdout harness pass;
four child entry points remain ignored by the ordinary harness.
Actual WASM: all 315 tests pass. All three wrapper modules remain 100% in
lines/functions/reported branches. Strict native/WASM Clippy, formatting,
release metadata and combined source-size checks pass.
Debug: 6,139/6,632 lines (92.57%), 2,809/3,212 branches (87.45%)
Release: 4,650/4,950 lines (93.94%), 2,788/3,193 branches (87.32%)
native: 9,464/10,291 lines (91.96%), 1,560/1,906 branches (81.85%), 1,265/1,406 functions (89.97%)
wasm: 6,545/9,900 lines (66.11%), 1,083/1,611 branches (67.23%), 901/1,390 functions (64.82%)

Project-wide coverage remains below the requested 100%. This batch establishes
in-process retry and fencing after a known durable commit. Noncontiguous async
applied markers, uncertain commit-decision flushes and abrupt-exit legacy version
sidecar publication are separate unfinished audits. It does not establish
power-loss semantics outside the adapter's persistence contract.

## Ordered recovery and retention after applied gaps

Before: all 39 corrected C# recovery cases fail. Three real failed asynchronous
checkpoints are followed by a later applied update, deletion or recreation;
reopening overwrites the latest value, resurrects a deleted row or loses the
recreated row. Thirty-six mapped interruption cases additionally check failures
before/after older redo, newer redo, pending undo, data flush, applied-marker
append and journal flush. Each opens the actual files twice after the fault.
Only corrected fixture runs form this baseline; setup/harness errors were
resolved before retaining it. Logs are csharp-recovery-order-before.log.

Four portable Rust contracts fail before the change; two deletion controls
pass because replay skips an update to an absent row. Shared contracts now
exercise three complete journal histories and 32 finite interruption schedules
from distinct durable data/journal images on native and actual WASM. Every
injected phase must execute; reopening twice verifies the latest committed row,
pending undo, applied marker and rollback marker.

Recovery starts at the first committed transition missing an applied marker,
replays every later committed transition in order, undoes pending work, and
flushes the complete final data before filling the gap. The old gap remains a
restart anchor until that final state is durable, including failures while redo
or undo is in progress. Normal already-checkpointed histories do not get redone.

Six additional C# regressions fail with sized/chunked live retention while the
39 ordered-recovery controls pass. Retention replayed an older failed async
checkpoint over a later commit and then closed its gap, making that loss
permanent on reopening. It now checkpoints current inline effects without
redoing old writes. The same six cases check live state, archive creation where
selected, exact markers and two persistent reopens. Ordinary async workers use
the same checkpoint helper. Existing rollback, version, ownership and shutdown
contracts remain in the full suites.

Full C# Debug/Release: 1,151 pass each; zero skips.
Full native Rust: 607 standard tests and the custom stdout harness pass;
four child entry points remain ignored by the ordinary harness.
Actual WASM: all 321 tests pass. All three wrapper modules remain 100% in
lines/functions/reported branches. Strict native/WASM Clippy, formatting,
release metadata and combined source-size checks pass.
Debug: 6,146/6,636 lines (92.62%), 2,816/3,216 branches (87.56%)
Release: 4,655/4,953 lines (93.98%), 2,794/3,197 branches (87.39%)
native: 9,478/10,300 lines (92.02%), 1,567/1,908 branches (82.13%), 1,266/1,407 functions (89.98%)
wasm: 6,577/9,909 lines (66.37%), 1,096/1,613 branches (67.95%), 903/1,391 functions (64.92%)

Project-wide coverage remains below the requested 100%. These tests verify the
adapter's flush contract and recovery interruptions; they do not establish
power-loss semantics outside it. Uncertain commit-decision append/first-flush
outcomes and abrupt-exit legacy sidecar publication are separate unfinished
audits. Broader algorithm/component and coverage requirements remain open.

## Uncertain commit-decision ownership and retry

Before: 76 C# failures and four passing controls across 80 mapped-store cases;
nine generic Rust failures; ten real-filesystem Rust version failures and one
passing commit-retry control. Cleanup undoes possibly durable edits, subsequent
writes bypass admission, version rollback/save omits the eventual branch head,
and generic append-only retry duplicates physical commit decisions. Complete
normalized baseline logs are retained beside this report.

Both native owners now retain an attempted decision before commit-marker append
or first flush. All writes remain fenced. Commit, rollback, disposal and Rust
save finish that attempt. IsCommitted still reports only a known durable
decision; false after an error can mean an unknown outcome. An uncertain retry
scans the journal for its exact decision before appending, then flushes it and
finishes the checkpoint. Ordinary commits and known-checkpoint retries add no
journal scan. Failed discovery retains ownership. C# asynchronous retry finishes
synchronously. Version cleanup publishes a decision that becomes known during
retry, retaining admission if sidecar publication then fails.

C# has 96 new cases: 80 ordinary-owner cases cover four append/flush points,
explicit/automatic owners, repeated failing cleanup, five write paths and
before/after decision reread faults. Sixteen version cases cover transaction,
owner, graph and rollback cleanup plus a second publication failure. Actual
mapped files are reopened, checked for exact rows/names/unique markers, changed
again and reopened. Generic Rust has eleven shared test functions executing 88
finite schedules, with distinct durable images and a following reopened edit.
Eleven filesystem version tests cover commit/rollback/save retry and eight row/
named admission paths. The shared generic functions execute on actual WASM too.

Full C# Debug/Release: 1,247 pass each; zero skips.
Full native Rust: 629 standard tests and the custom stdout harness pass;
four child entry points remain ignored by the ordinary harness.
Actual WASM: 332 pass. All three wrapper modules remain 100% in measured lines,
functions and reported branches. Strict native/WASM Clippy, formatting,
release metadata and combined source-size checks pass.
Debug: 6,157/6,647 lines (92.63%), 2,825/3,226 branches (87.57%)
Release: 4,665/4,963 lines (94.00%), 2,804/3,207 branches (87.43%)
native: 9,494/10,316 lines (92.03%), 1,573/1,916 branches (82.10%), 1,265/1,406 functions (89.97%)
wasm: 6,595/9,925 lines (66.45%), 1,100/1,621 branches (67.86%), 903/1,390 functions (64.96%)

Project-wide coverage remains below the required 100%. Resolving an uncertain
commit can finish it even when cleanup was requested through rollback. Abrupt
process exit between a legacy journal decision and version-sidecar publication,
pre-journal observer mutation and power-loss behavior outside adapter flush
contracts remain separate unfinished audits. Broader algorithm/component and
coverage requirements remain open. Fresh exact-head CI is required after push.

## Initial history excludes aborted records

Before: all nine corrected C# cases and nine Rust cases fail. Initial attribution
uses every journal record and LastLoggedSequence, exposing all-aborted head 2
instead of zero and trailing-abort head 3 instead of committed head 2. A middle
abort remains reachable through checkout/branch creation. Version construction
over a live owner and older sidecars containing aborted attribution are accepted.
The retained complete before logs contain behavioral failures only; earlier
fixture compilation errors are excluded from the baseline.

Initial attribution now selects known committed transitions and their maximum
sequence, or zero. Sparse gaps remain unavailable. Metadata recovery rejects a
transition marker known to belong to uncommitted work, including recovered
pending transactions. It reports the sequence without repairing or removing
existing files. An open inner owner blocks version construction/recovery and
metadata operations before they publish changes.

Six histories per language cover all-aborted, trailing and interleaved records
with explicit rollback or persisted pending recovery. They verify heads, rejected
checkout/branch creation, exact committed rows, checkout to zero and back, tags
and native file reopening. C# also verifies no aborted attribution markers.
Two cases per language reject exact legacy sidecar errors and preserve data;
one rejects a live owner. The borrowed C# transaction remains usable, and the
consuming Rust constructor releases its stores for recovery. All nine Rust
contracts also run on actual WASM; memory-only metadata recovery there is not
represented as an additional filesystem reopen.

Full C# Debug/Release: 1,256 pass each; zero skips.
Full native Rust: 638 standard tests and the custom stdout harness pass;
four child entry points remain ignored by the ordinary harness.
Actual WASM: 341 pass. All three wrapper modules remain 100% in measured lines,
functions and reported branches. Strict native/WASM Clippy, formatting,
release metadata and combined source-size checks pass.
Debug: 6,166/6,656 lines (92.64%), 2,831/3,232 branches (87.59%)
Release: 4,674/4,972 lines (94.01%), 2,810/3,213 branches (87.46%)
native: 9,513/10,336 lines (92.04%), 1,579/1,922 branches (82.15%), 1,267/1,408 functions (89.99%)
wasm: 6,638/9,945 lines (66.75%), 1,110/1,627 branches (68.22%), 905/1,392 functions (65.01%)

Project-wide coverage remains below the required 100%. Automatic version-owner
failure retention, abrupt-exit sidecar publication, history truncated before a
referenced transition, broader strategies and complete coverage remain open.
Rejecting previously invalid attribution does not repair data already changed
by an earlier aborted checkout. Fresh exact-head CI is required after push.

## Automatic version ownership

Before: all 32 C# cases and eleven Rust functions fail. Automatic writes have
no facade transaction after an inner commit/publication error. C# owner/graph
cleanup cannot publish the branch; Rust commit/rollback report no version owner,
and save leaves head 2 instead of committed head 3. Complete behavioral before
logs are retained; fixture and assertion corrections are excluded from baseline.

Automatic row writes now create the same facade owner as explicit transactions.
It retains the original branch and transition range until commit and publication
succeed. Pre-commit write/callback/journal errors undo automatically; failed undo
retains the owner for retry. Cleanup after a commit attempt resolves that decision
and finishes publication, without undoing a possibly durable commit. Existing and
zero-address allocation and existing lookup remain no-ops. Explicit ownership and
C# normal asynchronous checkpoint policy are preserved.

C# covers eight persistence phases before/after errors with owner/graph cleanup,
repeated publication failure, five row/name write fences plus metadata fences,
exact branch heads, checkout, actual mapped reopening and a following commit.
Eighteen pre-commit controls cover callback/transition faults, automatic undo and
failed undo retry; an async control observes its checkpoint then joins shutdown.
All 153 focused C# cases pass. Eleven Rust functions cover 22 real filesystem
fault schedules (decision flush and checkpoint), commit/rollback/save cleanup,
every existing write entry point, exact markers, checkout, reopening and following
automatic edits. Four portable controls check no-ops, ordinary errors, explicit
undo ownership and automatic entry points; they also run on actual WASM. All
59 focused Rust contracts pass. Filesystem faults are not represented as WASM I/O.

Full C# Debug/Release: 1,307 pass each; zero skips.
Full native Rust: 653 standard tests and the custom stdout harness pass;
four child entry points remain ignored by the ordinary harness.
Actual WASM: 345 pass. All three wrapper modules remain 100% in measured lines,
functions and reported branches. Strict native/WASM Clippy, formatting,
release metadata and combined source-size checks pass.
Debug: 6,169/6,659 lines (92.64%), 2,835/3,236 branches (87.61%)
Release: 4,677/4,975 lines (94.01%), 2,814/3,217 branches (87.47%)
native: 9,518/10,331 lines (92.13%), 1,582/1,922 branches (82.31%), 1,274/1,414 functions (90.10%)
wasm: 6,727/9,940 lines (67.68%), 1,122/1,627 branches (68.96%), 925/1,398 functions (66.17%)

Project-wide coverage remains below the required 100%. Abrupt-exit sidecar
publication needs a persisted intent and real finite process-exit tests; an
in-memory owner only establishes graceful cleanup retry. Truncated referenced
history, broader strategies and complete coverage remain open. C# CI at the
preceding SHA failed two Windows async waits; full logs are preserved and worker
scheduling is being audited separately. Fresh exact-head CI is required after push.

## Async checkpoint worker scheduling

The old worker occupies one ThreadPool thread per idle transaction owner,
including synchronous owners. Async commits queue another pool task from that
blocking worker, then wait for it. In an isolated child limited to two workers,
four tiny owners prevent an unrelated canary and a committed checkpoint from
running. The ordinary automated regression first fails with responsive=False,
checkpoint=False; its complete behavioral before log is retained beside this
report. The initial multiple-entry-point compiler fixture correction is not
counted as a behavioral failure.

A Channel now suspends the single consumer while idle. That consumer executes
admitted checkpoint batches directly and in order, without another pool task.
Shutdown completes admission, cancels queued work, joins actual in-flight work
outside the storage lock and releases queued references. Existing durable
decisions remain recoverable. Active callback/descendant self-join rejection,
concurrent/repeated shutdown and borrowed mapping ownership remain intact.
Neither the old Windows waits nor their timeouts were increased.

The portable child bounds waits and is killed and reaped if it exceeds its
deadline; it never changes the ordinary testhost's global pool configuration.
All 39 focused transaction/async/lifetime contracts pass, including the new
starvation regression. Full C# Debug and Release suites pass 1,308 tests each,
with zero failures/skips. Formatting verification, release metadata and combined
source-size checks pass. Rust and WASM production sources did not change in
this scheduling batch; their preceding full suites pass 653 and 345 tests.
Debug: 6,173/6,664 lines (92.63%), 2,836/3,238 branches (87.58%)
Release: 4,680/4,979 lines (93.99%), 2,816/3,219 branches (87.48%)

The preceding SHA's C# workflow also passes the full suite on all three
platforms, demonstrating that the historical Windows failures were
intermittent. The finite child independently establishes starvation and verifies
this change. Fresh exact-head CI is still required after publication. Complete
project coverage and the remaining requirement inventory stay unfinished.

## Durable version publication after process exit

A real process exit after journal commit could retain the updated row but lose
its branch attribution: expected head 3, recovered head 2. Before the fix, both
C# checkpoint-error cases fail; Rust also fails both acknowledged-commit cases.
Pending and rollback controls pass. Complete behavioral before logs are retained.
The finite children exit without disposal/destructors; native Rust restores its
injected directory rename before exiting. Deadlines kill and reap a stuck child.

The version layer now durably prepares one versioned intent with transaction
identity, selected branch and frozen (before, after] range before committing the
journal. It flushes committed attribution/head/applied metadata, then removes and
flushes the intent. Every fallible phase retains admission and cleanup retry.
Startup first completes transaction redo/undo, then publishes a committed intent
or clears an aborted one. Existing no-intent metadata remains compatible.
Rust metadata is replaceable through NamedTypeLinks; the default public alias
retains its existing backend/API. C# accepts an explicit metadata-flush callback
and reports SupportsDurablePublication. A separate constructor overload preserves
existing compiled calls; a reflection regression first fails against the draft.
An eight-combination capability matrix covers all selected stores, and the caller
callback contract verifies exact intent/decision/publication/cleanup ordering and
persistent checkout. Trace output remains optional/off by default.

Six real process cases per native language now pass: success/checkpoint errors
for explicit/automatic owners, plus pending and rollback controls. All verify
exact recovered data, branch heads, checkout and a following committed edit.
Ninety C# cases and 108 portable Rust fault schedules cover nine publication
phases, before/after effects, repeated cleanup errors and every existing entry
point. Fourteen interrupted recovery schedules per language verify two actual
reopenings. Shared exact/malformed wire vectors and nine contradictory schedules
per language verify rejection without rewriting sidecars. Native checks preserve
committed rows after rejection. Portable fault/vector/contradiction contracts run
in actual WASM; filesystem/process checks retain native coverage.

Full C# Debug/Release: 1,440 pass each, zero skips.
Full native Rust: 672 standard tests and the custom stdout harness pass;
five ignored child entry points are invoked only by their finite parent tests.
Actual WASM: 356 pass; all three wrapper modules remain 100% in measured lines,
functions and reported branches. Formatting, release metadata, strict native/WASM
Clippy and combined source-size checks pass. An initial Clippy test-module-order
violation was corrected; it is not a behavioral regression or baseline result.
Debug: 6,273/6,764 lines (92.74%), 2,940/3,346 branches (87.87%)
Release: 4,763/5,063 lines (94.07%), 2,920/3,327 branches (87.77%)
native: 9,696/10,518 lines (92.18%), 1,645/1,994 branches (82.50%), 1,292/1,432 functions (90.22%)
wasm: 6,904/10,110 lines (68.29%), 1,190/1,699 branches (70.04%), 945/1,415 functions (66.78%)

The added cost is one temporary named metadata link and three ordered sidecar
flushes per version commit, with more flushes on retry. Bare cache/transaction
paths do not select this cost. Removing an intent does not compact interned names.
Durability requires truthful adapter flushes and available storage. Process exit
is not device power loss; in-memory WASM is not a filesystem durability test.
Truncated referenced history, legacy name logging, broader strategy/composition
work and complete project coverage remain unfinished. All nine workflows pass
on the preceding scheduling SHA 24554b0; new exact-head CI is required after push.

## Missing retained legacy checkout history

All three regressions per native language first fail: checkout and branch
switching silently skip unavailable undo records; another branch move can undo
an available record before discovering its missing redo. A one-transition sized
journal reproduces all three. Complete before logs are retained beside this file.

Both implementations now snapshot/index the retained log once and resolve all
changed undo and redo records before the first mutation. They reject unavailable
records without changing rows, branch selection, current head or branch inventory.
No-op checkout and available suffix moves over an unavailable shared prefix
remain valid. Following edits and actual native save/close/reopen preserve exact
rows and heads. The same Rust contracts run in actual WASM.

Lookup costs O(N + V) time and temporary memory for N retained records and V
changed records, replacing repeated O(N * V) log scans/clones. Bare paths do not
select this work. Retention still does not promise every historical version is
replayable. This preflight addresses unavailable records; atomic recovery from
failure while applying available records is a separate unfinished audit.

Full C# Debug/Release: 1,443 pass each, zero skips.
Full native Rust: 675 standard tests plus the custom stdout harness pass;
five ignored child entry points run only through finite parent tests.
Actual WASM: 359 pass; all three wrapper modules remain 100% in measured lines,
functions and reported branches. Formatting, release metadata, strict native/WASM
Clippy and combined source-size checks pass.
Debug: 6,261/6,751 lines (92.74%), 2,941/3,344 branches (87.95%)
Release: 4,760/5,059 lines (94.09%), 2,921/3,325 branches (87.85%)
native: 9,710/10,530 lines (92.21%), 1,648/1,994 branches (82.65%), 1,293/1,433 functions (90.23%)
wasm: 6,918/10,122 lines (68.35%), 1,193/1,699 branches (70.22%), 946/1,416 functions (66.81%)

Complete project coverage remains below the requested 100%. The publication
SHA 2a1c9e3 exposes ten Windows preservation-fixture sharing violations; byte
comparisons now run outside all mapped-store lifetimes. Fresh exact-head CI must
verify that correction as well as the retention change after publication.

## Public unlogged replay ownership

Six cases per native language first fail: public apply/revert accepts replay
over a pending transaction, a known commit awaiting checkpoint, or an unfinished
rollback. Original behavioral before logs are retained beside this report.
The initial Rust fixture compile diagnostic is separate, not a reproduction.

Both public helpers now check for any live owner before setting the replay flag
or touching data. Internal recovery and rollback retain their private helpers.
Tests verify unchanged rows and journal sequence, continued admission fencing,
the original cleanup decision on retry, following edits and unlogged replay
without an owner. Existing version controls remain available. The success-path
check costs O(1), with no allocation, log scan or additional background work.

Focused suites: 22 native Rust and 24 C# pass. Full C# Debug/Release: 1,449 pass
each, zero skips. Full native Rust: 681 standard tests plus the custom stdout
harness pass; five ignored child entries execute through finite parent tests.
Actual WASM: 365 pass, including the same six replay contracts. All three WASM
wrapper modules remain 100% in measured lines, functions and reported branches.
Formatting, release metadata, strict native/WASM Clippy and source sizes pass.
Debug: 6,267/6,757 lines (92.75%), 2,943/3,346 branches (87.96%)
Release: 4,765/5,064 lines (94.10%), 2,923/3,327 branches (87.86%)
native: 9,720/10,540 lines (92.22%), 1,650/1,996 branches (82.67%), 1,294/1,434 functions (90.24%)
wasm: 6,928/10,132 lines (68.38%), 1,195/1,701 branches (70.25%), 947/1,417 functions (66.83%)

Complete project coverage remains below 100%. All nine workflows pass on prior
retention/Windows-correction head 1f742c5, with the ten formerly failing Windows
fixtures verified in passing logs. This replay change needs its own exact-head
CI. Atomic checkout failures and ordinary legacy name logging remain open.

## Same-handle file journal tail recovery

Six schedules per file adapter first fail behaviorally: ASCII/incomplete UTF-8
partial append followed by read, append or flush. Rust and C# use real Linux
8 KiB file-size quota failures with finite child deadlines; JS writes one byte
before the injected error. True before logs are preserved beside this report.
Initial C# JIT startup, error-type and coverage-exit fixture issues are recorded
separately and are not counted as product reproductions. The first full Rust
coverage run exposed inherited stderr already larger than the quota; a finite
9000-byte-stderr control reproduces the resulting diagnostic failure. Rust now
pipes child diagnostics and restores its child-only soft limit after all journal
operations, before LLVM exit profiling. All eight focused quota cases produce
valid complete coverage profiles. Initial fixture logs remain separate.

All file adapters retain uncertain framing until repair and its persistence
complete. Every read/append/flush retries repair first. Complete records survive
errors after their effects, and complete invalid UTF-8 remains an error rather
than being truncated. Successful operations check one flag, without rescanning;
repair scans backward using a fixed 8 KiB buffer. Rust opens a repair handle only
on that error path. C# disables the managed write buffer, so append immediately
performs a file write and failed buffered bytes cannot reappear during repair.
The public constructor signature and raw cache paths are preserved.

Focused expanded suites: 20 Rust and 31 C# pass. C# portable finite filesystem
faults cover partial/full writes, failed repair reads/truncation/sync, repeated
blocked operations, retry, exact reopening, resource cleanup and complete bad
UTF-8. Eight Rust Linux quota cases also verify retry after unavailable repair
paths. Existing JS controls retain short reads/writes, no-progress errors,
exclusive writer fencing and reopening; eleven new fault schedules pass.

Full C# Debug/Release: 1,469 pass each, zero skips on this Linux host. Six quota
cases return without using Linux quotas on other platforms; the fourteen new
portable cases still run there. Full native Rust: 690 standard tests plus the
custom stdout harness pass; six ignored child entries run via finite parents.
Actual WASM: 365 pass; file adapters are native and absent from that target.
All three WASM wrapper modules remain at 100% measured coverage. JS: 106 pass,
all seventeen production modules at 100% lines/branches/functions; types pass.
Formatting, release metadata, source sizes and strict native/WASM Clippy pass.
Debug: 6,286/6,775 lines (92.78%), 2,945/3,348 branches (87.96%)
Release: 4,782/5,080 lines (94.13%), 2,925/3,329 branches (87.86%)
native: 9,737/10,557 lines (92.23%), 1,652/1,998 branches (82.68%), 1,295/1,435 functions (90.24%)
wasm: 6,928/10,149 lines (68.26%), 1,195/1,703 branches (70.17%), 947/1,418 functions (66.78%)

Complete project coverage remains below 100%. This change repairs file framing;
it does not settle uncertain engine commits or simulate device power loss.
All nine workflows pass on journal-tail head 7826d95; see live-tail-ci.txt.
The earlier replay-owner head 31a50a0 also passes, including a successful retry
after an external Bun-download HTTP 502. Later code needs fresh exact-head CI.
Ordinary name history and the wider algorithm/composition requirements remain
unfinished; the movement audit below addresses checkout recovery.

## Durable legacy checkout and branch movement

Six cases per native language first fail behaviorally: checkout/switch with
before/after applied-marker faults or absent persistence. Additional before
logs reproduce accepted malformed/duplicate metadata, broken ancestry, sequence
reuse after payload eviction, C# wire timestamp precision and unreachable
cleanup ownership after constructor failure. Initial fixture compile failures
are excluded from behavioral reproduction evidence.

The optional version layer resolves all changed undo/redo before touching rows,
then retains exclusive inner replay ownership and a checksummed portable intent.
It flushes data before publishing target current/applied markers, flushes those
markers before removing the intent, and retries uncertain cleanup. Startup
validates paths, retained payloads, commit decisions and metadata before replay.
An intent retains payloads evicted from the journal; applied markers reserve
the sequence allocation floor. C# retains a failed constructor's recovery
callback on its borrowed inner owner, keeping retries reachable and writes
fenced. Async checkpoints cannot race movement replay.

C#: 102 movement cases and two UTF-16 compatibility cases pass. The movement
cases include 34 metadata failure schedules, twelve row/data/log fault
schedules with repeated whole-graph cleanup, 18 real child exits (including two failed async checkpoints), 25 recovery scenarios, twelve
failed-constructor/retry cases and shared wire vectors. Rust: 34 metadata fault
schedules and 25 recovery scenarios pass, with all shared contracts also running
in actual WASM. Native Rust parents cover sixteen original real process exits
and 24 additional recovery interruptions, exact repeated reopening and following
edits. Shared fixtures provide five common valid and 21 malformed inputs, plus
two unpaired-code-unit intents accepted by C# and rejected by Rust. Native
codecs verify every input; actual WASM exercises all 27 recognized-prefix cases
through recovery (the unrelated-prefix vector is outside movement recovery).
These cover canonical encoding, Unicode, corruption, overflow and payload
shapes. Prior publication tests pass. Runtime-created C# code-unit regressions
first fail under overly strict new Unicode validation, then preserve existing
branch/tag history and exact reopening, including two additional real exit
probes. Attribute string arguments replace unpaired units; the initial attribute control is not reproduction evidence.

Full local C# Debug/Release: 1,573 each, zero skips. Full native Rust: 733 standard
tests plus the custom stdout harness pass; seven ignored child entries execute
through bounded parents. Actual WASM: 404 pass. JS: 106 pass and TypeScript checks
pass, with all seventeen production modules at 100% measured lines, branches
and functions. Three WASM wrappers retain 100% measured coverage. Formatting,
source sizes, release checks and strict native/WASM Clippy pass. The initial
WASM lint failure identified duplicate fixture modules and an unused import;
one shared WASM fixture module fixes both without suppressing lint rules.
Debug: 6,501/6,993 lines (92.96%), 3,112/3,528 branches (88.21%)
Release: 4,959/5,262 lines (94.24%), 3,083/3,509 branches (87.86%)
native: 10,056/10,888 lines (92.36%), 1,745/2,120 branches (82.31%), 1,325/1,465 functions (90.44%)
wasm: 7,231/10,450 lines (69.20%), 1,301/1,825 branches (71.29%), 975/1,445 functions (67.47%)

Retained state/wire size are O(changed history); preparation indexes the retained
journal and traverses paths in O(N + V) temporary time/memory. It does not clone
database rows; bare stores pay none of this cost. Legacy reads remain live;
failed movement has an uncertain outcome until retry/reopen. Process exits do
not simulate device power loss. Complete project coverage, ordinary legacy
name/history logging and the wider algorithm/composition criteria remain open.
All nine workflows pass on preceding journal-tail head 7826d95. This movement
change needs fresh exact-head CI and does not complete the overall issue.

All nine workflows pass on movement head `d4cb43c`, with runs created
2026-10-25 at 02:44:35 UTC after its commit and push. Native platform, package,
fresh-merge, release, JS and actual-WASM checks pass; exact run URLs are in
`version-movement-ci.txt`. Later code requires fresh exact-head validation.

## Legacy Rust transaction identity across restart

Root cause: wall-clock milliseconds plus a process counter can repeat after a
restart. Recovery indexes commit/abort decisions by ID, so reusing one can
commit an uncommitted write, resurrect an aborted write or hide pending undo.
Two original finite probes first recover row 21 instead of expected row 11.
Four automated native contracts first fail, as do four actual-WASM contracts.
The before logs are preserved separately from successful validation. Initial
fixture compile failures are excluded from reproduction evidence. Verbose
WASM compiler DEBUG lines are omitted from its preserved behavioral log.

Explicit and automatic allocation now exclude zero and IDs in the existing
recovered commit/abort sets. Those decisions survive retention of payloads.
Normal candidate checks cost expected O(1); K collisions after restart cost
O(K) extra candidates while advancing the shared process counter. No extra
retained index, history scan, mandatory randomness or bare-store work is added.
A trace, disabled by default, reports skipped historical IDs once per affected
allocation. Existing public APIs and wire encodings are unchanged.

Four Linux parent cases use genuinely fresh processes and frozen realtime.
Each child has a ten-second monotonic deadline, 512 MiB address-space limit and
8 MiB stack limit. Prior commit/abort decisions are followed by explicit undo
or exit after flushing pending rows, two fresh reopenings and a following commit.
All four pass. Actual WASM imports old committed/aborted decisions and freezes
Date.now to verify explicit and automatic owners, recovery and following writes.
All four pass. Both original finite public-API restart probes also pass after
rebuilding against the corrected library. Special Linux clock interposition is
not claimed to execute on Windows/macOS; ordinary existing contracts remain.

Full local native Rust: 737 standard tests plus the custom stdout harness pass;
eight ignored child entries execute through bounded parents. Actual WASM: 408
pass. C# Debug/Release: 1,573 each, zero skips. JS: 106 plus TypeScript checks
pass, with all seventeen production modules at 100% lines/branches/functions
and all three thresholds enforced. The three WASM wrappers retain 100% measured
coverage. Formatting, strict native/WASM Clippy, release metadata and source
sizes pass. No container OOM occurred.
Debug: 6,501/6,993 lines (92.96%), 3,111/3,528 branches (88.18%)
Release: 4,959/5,262 lines (94.24%), 3,083/3,509 branches (87.86%)
native: 10,069/10,901 lines (92.37%), 1,753/2,130 branches (82.30%), 1,326/1,466 functions (90.45%)
wasm: 7,241/10,463 lines (69.21%), 1,309/1,835 branches (71.34%), 976/1,446 functions (67.50%)

IDs are journal-local, not cryptographic/global identifiers. This does not repair
older journals already containing conflicting reused decisions. A separate
finite probe reproduces chunk archives being overwritten when independent
journals share an archive directory and clocks/counters repeat; exclusive archive
ownership remains the next audit. Legacy name/history journaling, the observer's
pre-append window, finer composition and full project coverage remain open.
All nine workflows pass on preceding movement head d4cb43c; this change requires
new exact-head CI and does not complete the wider issue.

All nine workflows pass on transaction identity head `6d231fa`, with runs
created 2026-10-25 at 03:04:52 UTC after its commit/push. Native platform,
fresh-merge, package, release, JS and actual-WASM checks pass. Exact URLs are
in `transaction-identity-ci.txt`. Later code requires fresh exact-head CI.

## Exclusive chunk archive ownership

Root cause: a candidate timestamp/counter filename was opened with truncating
creation. Two fresh processes with independent journals and a shared archive
directory acknowledge commits, but the second replaces the first archive.
The original finite probe preserves only one file instead of two. Four native
regressions first fail for an existing file, symlink, dangling symlink and
directory. The C# writer, extracted with behavior unchanged, first fails three
occupied-path contracts; its directory rejection is a passing control. These
C# cases exercise supplied paths rather than asserting random GUID reuse.
Preserved before logs exclude incidental fixture build/framework mistakes.

Rust now atomically claims an absent archive path with create_new and retries
AlreadyExists using successive candidates, without following symlinks or
truncating existing entries. Other open errors retain their storage error.
Normal allocation adds no directory scan; K collisions cost O(K) additional
opens, and bare stores incur no work. Default-off tracing identifies occupied
entries. C# uses FileMode.CreateNew: a rejected optional archive preserves
records in the live log, and a later retention attempt chooses a new GUID.
Its existing optional-error behavior, public API and record encoding remain.

Four bounded Linux restart contracts now pass with independent journals,
repeated collisions, following commits and two actual reopenings per database.
Every child has a ten-second monotonic deadline, 512 MiB address-space limit and
8 MiB stack limit. An unrelated native filesystem error control verifies known
commit ownership, admission fences, repeated cleanup failure, successful retry,
following writes and two reopenings. The original public-API probe now retains
two complete archives and the first file's original bytes.

C# verifies four occupied entry types, source records not consumed on rejected
creation, a following distinct archive, eight competing writers with one complete
owner, and sync/async optional-error retries followed by repeated reopening.
All 21 focused ownership/retention cases pass. Symlink controls execute on Linux;
Windows privileges are not assumed. Full native platform CI remains necessary.

Full local native Rust: 742 standard tests plus the custom stdout harness pass;
nine ignored child entries execute through bounded parents. Actual WASM: 408
pass. C# Debug/Release: 1,580 each, zero skips. JS: 106 plus TypeScript checks
pass, with all seventeen production modules at 100% lines/branches/functions
and all three thresholds enforced. The three WASM wrappers retain 100% measured
coverage. Formatting, strict native/WASM Clippy, release metadata and source
sizes pass. No container OOM occurred.

Both runnable CLI examples also pass, including an independent second database
sharing the archive directory. Each requires additional archive entries and
compares every previous file byte for byte. Commands:
  bash examples/transactions/run-rust.sh
  bash examples/transactions/run-csharp.sh
These examples demonstrate ordinary use; frozen-clock and occupied-path fault
contracts are separate automated regressions.
Debug: 6,507/6,995 lines (93.02%), 3,111/3,528 branches (88.18%)
Release: 4,964/5,264 lines (94.30%), 3,083/3,509 branches (87.86%)
native: 10,085/10,911 lines (92.43%), 1,754/2,134 branches (82.19%), 1,327/1,465 functions (90.58%)
wasm: 7,241/10,473 lines (69.14%), 1,309/1,839 branches (71.18%), 976/1,445 functions (67.54%)

Live journals remain the recovery authority. Exclusive ownership does not make
archive contents atomically published after partial writes, prove power-loss
durability, eliminate duplicate exports after recovery, provide globally unique
transaction IDs, or add native filesystem support to WASM. Other legacy audits,
full project coverage and the wider issue remain unfinished. This batch requires
fresh exact-head CI before its publication checklist can be completed.

All nine workflows pass on archive ownership head `7b03c07`, with runs
created 2026-10-25 at 03:25:08 UTC after its commit/push. Native platform,
fresh-merge, package, release, JS and actual-WASM checks pass. Exact URLs are
in `archive-ownership-ci.txt`. Later recovery changes need fresh exact-head CI.

## Recovery validation and live ownership

Root causes: recovery accepted opposite decisions for one transaction, conflicting
records for one sequence, zero owners and nonpositive sequences. Ordered insertion
retained duplicate parsed records, including conflicts. Live recovery errors also
released admission while partially replayed data and incomplete persistence could
remain reachable. Rust reload errors had the same admission problem.

A finite public API probe reproduces all seven sampled contradictions: each is
accepted, changes row 11 to 21 and appends metadata. It runs with a ten-second
deadline, 512 MiB address-space and 8 MiB stack limits. Its source, builder and
bounded runner are retained in experiments/issue-138/recovery-contradiction-probe.rs,
build-recovery-contradiction-probe.py and run-recovery-contradiction-probe.py.
Native validation first
fails 17/18 tests; the applied-floor control passes. Actual WASM first fails
those 17 plus three live interruption functions. Initial C# validation fails
17/17; its expanded matrix fails 101/144: 17 validation cases and all 84 live
phase/cleanup cases. The 42 constructor interruptions and checkpoint control
pass. Native and WASM each separately fail the new reload ownership function.
Before loops stop at their first failed assertion; their logs do not prove that
every later phase failed before the fix.

A follow-up regression against the initial ownership draft reproduces cleanup
skipping the underlying failed reload. If an external writer already applied its
journal, replay alone cannot refresh a stale view. Native and WASM both first
fail. Cleanup now retains that refresh obligation separately. A further two C#
contracts first fail before adding recovery guards to queued checkpoints and
retention: after a failed read they append applied markers. A queue sentinel and
finite waits enforce the schedule without sleep-based races. Reflection controls
only admission/queue scheduling; assertions inspect public state and real stores.
Incidental fixture compilation errors are not preserved as behavioral evidence.

Recovery now parses recognized records into temporary indexes before replacing
live history or replaying rows. It rejects opposite decisions even after payload
retention, conflicting owner/payload/timestamp records, zero owners and nonpositive
sequences. It compares parsed values and collapses identical retries, preserving
legacy accepted numeric/extra-field forms, unrelated names, sequence gaps and
retained applied floors. Standalone parsers and wire formats remain unchanged.
Hash indexes and a final sort replace worst-case quadratic vector insertion;
preparation takes expected O(N + U log U) time and O(U + M) additional index space
for N records, U unique transitions and M unique markers, plus the adapter read.
Native pending-owner collection also uses a hash index while preserving undo
order. Bare stores incur no work; selected wrappers add constant admission flags.

Failed reads, validation, replay, data flush, marker append and journal flush
retain exclusive recovery admission. New transactions, all write paths and
unlogged/version replay reject until cleanup succeeds. Rust Recover/save/commit/
rollback retry recovery; reload additionally retries its incomplete data refresh.
C# Recover/decorator Rollback retry recovery; queued checkpoints and retention
wait for the same owner. Default-off tracing reports failure and retains the
original error. Existing commit/rollback and version-movement ownership remain.

Full validation initially exposes one older version-intent fixture that expected
contradictory decisions to survive transaction recovery. Align both language
fixtures with earlier transaction rejection while retaining metadata checks;
C# now preserves both journal files across two failed actual opens. Ten C#
focused cases and the shared native/WASM fixture pass. The first full Debug
run's single failure and 1,689 passes remain in the ignored experiment logs.

Focused after tests: 29 native and 29 actual-WASM tests pass: 18 journal cases,
eight interruption/reload functions and three suffix-ordering controls. Native
live functions cover 152 applicable phase/cleanup schedules. Reload covers two
initial before/after schedules and ten refresh/cleanup combinations with repeated
failure and following writes. These native/WASM restart controls load separate
durable images, rather than asserting physical filesystem crash behavior.
C# passes 146 focused cases: 18 validation, 42 constructor interruptions, 84 live
phase/retry cases and two queued-work/retention cases. Real mapped stores reopen
twice after following commits. Failed parsed input leaves rows, live indexes and
journal entries unchanged; repaired input permits recovery and subsequent writes.

A final assertion audit strengthens the three malformed-input fixtures with
populated live history. Restoring the seven changed production files to archive
head 7b03c07 reproduces three failures in each native language: rejection clears
two already committed records. All fixed source bytes are restored in a finally
block. The eighteen validation contracts then pass again on native Rust, actual
WASM and C# Debug/Release. Every final test source has passed. Complete-suite
coverage below was collected before this test-precondition strengthening, with
identical production source; final focused checks do not substitute new coverage
measurements for those complete runs.

Full local native Rust: 765 standard tests plus the custom stdout harness pass;
nine ignored child entries execute through bounded parents. Actual WASM: 431
pass. C# Debug/Release: 1,690 each, zero skips. JS: 106 plus TypeScript checks
pass. All seventeen JS modules and the three WASM wrappers retain 100% measured
lines/branches/functions coverage. Formatting, strict native/WASM Clippy, release
metadata and source sizes pass. Container OOM counters remain zero.
Debug: 6,531/7,020 lines (93.03%), 3,130/3,548 branches (88.22%)
Release: 4,994/5,295 lines (94.32%), 3,104/3,529 branches (87.96%)
native: 10,158/10,979 lines (92.52%), 1,775/2,156 branches (82.33%), 1,332/1,470 functions (90.61%)
wasm: 7,336/10,541 lines (69.59%), 1,334/1,863 branches (71.60%), 984/1,450 functions (67.86%)

Reads during failed recovery still see the mutable legacy view; no snapshot
isolation is added. Raw inner-store access requires caller ownership. Validation
does not repair an already damaged journal or prove device power-loss behavior.
Complete native/linked-WASM/C# coverage, ordinary name/history atomicity, other
legacy mutation windows and broader strategy requirements remain unfinished.
This batch requires fresh exact-head CI before its publication checklist closes.

## Recovered finite ancestry traversal and session feedback

All nine workflows for preceding recovery-validation head 0e95c81 pass; the
exact commit, creation times and links are in recovery-validation-ci.txt. The
interrupted ancestry work survives in f5af989 and was recovered by fast-forward
without rewriting its history. SESSION-RECOVERY.md links the requested upstream
report and distinguishes the observed interruption from its unknown cause.

Three finite native regressions per language first overflow a requested 256 KiB
thread stack while checking out, switching or validating a fork with 2,048
parents. Parent processes impose a 15-second deadline and kill/reap children;
Linux Rust children have a 512 MiB address-space limit and disabled core dumps,
and C# child GC is capped at 256 MiB. A metadata-only adapter isolates traversal
from name construction; it makes no persistence claim. The original real-name
deep probe times out while building its fixture and remains inconclusive.
Actual WASM already passes its deep control before the fix.

Iterative construction preserves root-to-leaf grouping, own branch heads and
descendant fork cutoffs without a stack frame per parent. Rust uses its existing
ordered attribution map, expected O(D + A + P) time and O(D + P) temporary space
for depth D, attribution entries A and returned sequences P. C# sorts selected
buckets, adding O(P log P) worst-case time. These bounds treat key operations
as unit cost; long names add hashing/comparison work. Bare stores do none of this work.
Independent shallow recursive references each verify 512 histories times five
targets, including cycles and missing parents, without claiming broken movement
topology is accepted. Public semantic controls preserve exact row replay,
unreachable-history rejection, following writes and two actual native reopenings.

Full final-production-source local validation passes: 770 native Rust tests plus
the custom stdout harness (ten ignored child entries execute through parents),
433 actual-WASM tests, C# Debug and Release with 1,695 each and no skips, and
106 JS tests plus TypeScript. All seventeen JS modules and the three WASM wrapper
files retain 100% measured lines/branches/functions. Strict native/WASM Clippy,
both Rust formatting checks, C# formatting, release fragments and source sizes
pass. The 217 repository tests first pass 216/217 because this session created an
empty root ci-logs directory. Removing only that empty workspace artifact makes
all 217 pass; both logs remain in experiments/issue-138. No production/test rule
was changed. Container OOM and OOM-kill counters remain zero.

| Scope | Covered lines | Covered branches | Covered functions |
| --- | --- | --- | --- |
| Native Rust | 10,154/10,972 (92.54%) | 1,779/2,158 (82.44%) | 1,329/1,467 (90.59%) |
| Actual WASM, complete linked source | 7,330/10,534 (69.58%) | 1,336/1,865 (71.64%) | 981/1,447 (67.80%) |
| C# Debug | 6,540/7,029 (93.04%) | 3,136/3,552 (88.29%) | Not reported by this collector |
| C# Release | 5,002/5,303 (94.32%) | 3,111/3,533 (88.06%) | Not reported by this collector |

Rust figures use official LLVM exported totals, not coordinate-union diagnostics.
C# percentages above are rounded from the integer counts; Coverlet's root rates
truncate some ratios. Complete coverage, other legacy mutation audits, independent
composition and wider competitor strategies remain unfinished. All nine workflows
pass for published ancestry head aa601e2, created 2026-10-25 11:08:29 UTC after
that commit; deep-ancestry-ci.txt preserves every run URL and exact head.


## Independent-session shared flush

The group-commit API is new. Temporary baseline coordinators that called ordinary
commit once per member first fail the two-session contract in Rust and C#:
their logs observe two flushes instead of one. These are prototype comparisons,
not claims that an older production group API existed. A separate regression
against the existing synchronous replica decorator first observes two primary
flushes in each of Rust, C# and JS. The committed group-commit-*-before.txt and
group-replication-*-before.txt files retain those failures.

The selected coordinator validates against private state in input order,
appends each accepted write's own revision, performs one physical flush and
publishes while retaining the existing engine gate. Ordinary commits keep their
path. Known validation aborts remain separate from unknown append/flush outcomes;
the latter fence the engine and preserve its old published view. Recovery may
find any valid accepted prefix. This is independent-session group commit, not
one atomic batch. Replication no longer flushes the primary twice; explicit
catch-up still flushes it before reading.

Shared portable contracts cover all selected isolation modes, write conflicts,
point reads and scans, reference/name/pair constraints, read-only and closed
members, bounded retention, allocator gaps and admission/point ownership release.
Four finite generation controls change and restore a row through ordinary or
grouped commits before stale write or read validation, preventing ABA acceptance.
Twelve before/after append/flush schedules use independent durable images and
write-through controls, then verify fencing, exact reopening and following writes.
Actual-WASM binding tests additionally exercise native handle consumption,
freeing unfinished groups, foreign members, closing the engine wrapper and
synchronous JS journal errors. JS rejects invalid groups before transfer,
drains already queued work and makes subsequent requests explicitly not-sent.

Native filesystem tests compose buffered append, delta encoding and two durable
fixed-primary replicas. A bounded blocked flush proves readers cannot observe
private publication before acknowledgement. Ten actual abrupt child exits per
native language cover five points in each snapshot/delta encoding. Parents kill
and reap after fifteen seconds; C# children cap their GC heap at 256 MiB. Two
actual reopenings verify exact prefixes, retained names/history, reserved-address
gaps and a following ordinary commit. Process loss leaves the OS running and
does not model loss of the OS page cache or physical device power.

The reusable sequential full command is
`bash experiments/issue-138/validate-group-commit.sh`. It builds fresh C# Debug
and Release binaries, uses Cargo jobs=1 and reduced debug information, executes
actual-WASM coverage, enforces all JS coverage thresholds and runs real generated
glue under Node, Bun and Deno. Runnable Rust, C# and JS examples successively open
the same journal and acknowledge revisions 1 through 6, separately for snapshot
and delta formats. See [the recipe and selected costs](../../ENGINE-GROUP-COMMIT.md).

The Coverlet gap diagnostic now accepts its capitalized `branch="True"`
attribute. Its former case-sensitive listing omitted partial branches; root
integer coverage counts were unaffected. An empty diagnostic listing has never
substituted for a collector's official summary.


Final full suites pass: 784 standard native Rust tests plus the custom stdout
harness, with eleven ignored child entries executed by bounded parents; 446
actual-WASM tests; C# Debug and Release with 1,722 tests each and no skips; and
111 JS tests plus strict TypeScript. All seventeen production JS modules and all
three WASM wrapper files retain 100% measured lines, branches and functions.
All 217 repository tests, strict native/WASM Clippy, both Rust formatting checks,
C# formatting, release fragments, source sizes and whitespace checks pass.

| Scope | Covered lines | Covered branches | Covered functions |
| --- | --- | --- | --- |
| Native Rust | 10,276/11,096 (92.61%) | 1,792/2,170 (82.58%) | 1,339/1,477 (90.66%) |
| Actual WASM, complete linked source | 7,446/10,650 (69.92%) | 1,352/1,881 (71.88%) | 991/1,457 (68.02%) |
| C# Debug | 6,607/7,096 (93.11%) | 3,168/3,584 (88.39%) | Not reported by this collector |
| C# Release | 5,060/5,361 (94.39%) | 3,143/3,565 (88.16%) | Not reported by this collector |

The figures use official LLVM summary counts and Coverlet integer totals,
rounded to two decimals. Container OOM and OOM-kill counters remain zero; memory
reclaim at the configured container limit did occur. Complete native/C#/linked-
WASM coverage, legacy name/history atomicity, independent composition and wider
competitor strategies remain unfinished. Exact-head CI is required for this
new publication; preceding ancestry head aa601e2 passes all nine workflows.


## Ordinary names in complete transaction history

The former legacy wrappers checked name-write admission without journaling
names. Six minimum regressions in each native language and actual WASM first
fail; the preserved failures distinguish rename, removal, unique-name movement,
deletion/cascade undo and name-only version attribution. Separate callback and
zero-address controls expose additional failures. See named-history*-before.txt
and named-history.txt under the PR evidence directory.

An additional complete JournalRecord preserves the existing row value/API and
v1 codec while adding canonical UTF-16 name payloads and name-only updates.
Recovery validates every record before changing live state. Rollback, recovery,
archives, checkout and durable movement intents consume complete records.
Explicit name writes flush their undo before changing names; actual resulting
row names are captured. C# drains pending rows before nested name writes while
retaining callback delivery/coalescing. Required capture failures retain undo
and cleanup ownership. Shared vectors state exact canonical/legacy compatibility
and the native string domains; old readers reject new complete payloads.

Twenty-four independent durable-image fault schedules per native language
verify exact state at failure, two reopenings, retained ownership and following
edits. Portable contracts run under actual WASM. Invalid/conflicting metadata,
complete public replay admission, sized/chunked archive replay, version movement
faults and bounded abrupt exits exercise the corresponding persistence paths.
These process exits leave the OS running and do not simulate physical power loss.

Complete local validation passes: 801 standard native Rust tests plus the custom
stdout harness, eleven child entries exercised by bounded parents, 460 actual
WASM tests, C# Debug/Release with 1,788 tests each, 111 JS tests and 217 repository
checks. Strict types, native/WASM Clippy, formatting, source sizes, release
metadata and whitespace pass. All seventeen JS modules and three WASM wrappers
remain at 100% measured lines/branches/functions. Generated actual-WASM glue
passes Node/Bun/Deno conformance. Cross-language persistent group examples pass
for snapshot/delta journals; ordinary native name/history examples pass too.

| Scope | Covered lines | Covered branches | Covered functions |
| --- | --- | --- | --- |
| Native Rust | 10,667/11,506 (92.71%) | 1,884/2,274 (82.85%) | 1,389/1,529 (90.84%) |
| Actual WASM, complete linked source | 7,867/11,060 (71.13%) | 1,445/1,985 (72.80%) | 1,047/1,509 (69.38%) |
| C# Debug | 6,803/7,316 (92.99%) | 3,327/3,756 (88.58%) | Not reported by this collector |
| C# Release | 5,219/5,537 (94.26%) | 3,299/3,737 (88.28%) | Not reported by this collector |

The figures use official LLVM summaries and Coverlet integer totals, rounded
to two decimals. Builds remain sequential under the configured memory limit;
reclaim occurred with zero OOM/OOM-kill events. See
[the recipe, compatibility and selected costs](../../TRANSACTION-NAMES.md).
Full sidecar scans and retained strings are selected transaction costs; bare
stores do not run them. Complete coverage, row-before-observer durability,
custom unobserved side effects, efficient structures, finer composition and
wider strategies remain unfinished. Fresh exact-head CI is still required.


## Ordinary pre-effect row undo and complete result reconciliation

The selected `WriteAheadImage` policy now flushes a mandatory canonical full
logical image before the first row/name effect in an owner. It retains that
image through preparation, decision and cleanup failures. Recovery interleaves
pending images with legacy undo and redoes the committed suffix even when some
records were previously applied. Complete result reconciliation records custom
unreported rows/names before success or version intent. Identical-name calls
also reconcile; genuine no-ops allocate no version. Default-policy unique-name
eviction now records the old unobserved name owner in the same pending batch.

Five portable operations and eight C# real-process cases fail before the change;
actual-WASM before cases and separate unreported-effect/identical-name failures
are preserved. Thirty-two fault schedules per native language, repeated undo
failures, detached durable images, shared canonical vectors, ordered recovery,
committed redo, version checkout and following edits verify the declared scope.
See [the complete local evidence](../../../dev/log/issues/138/pulls/139/row-wal.txt)
and ROW-WAL-PLAN.md. Bounded child exits leave the OS running.

Full local validation passes: 820 standard native Rust tests plus the custom
stdout harness, twelve bounded-parent child entries, 478 actual WASM tests,
1,901 C# tests per Debug/Release configuration, 111 JS and 217 repository tests.
Formatting, strict native/WASM Clippy, type checks, release metadata, source sizes
and whitespace pass. Generated actual-WASM conformance passes Node/Bun/Deno.
Persistent group examples pass across languages for snapshot/delta encoding;
ordinary native examples pass under both undo policies. All seventeen JS modules
and all three WASM wrapper files measure 100% lines/branches/functions.

| Scope | Covered lines | Covered branches | Covered functions |
| --- | --- | --- | --- |
| Native Rust | 11,099/11,968 (92.74%) | 1,995/2,412 (82.71%) | 1,430/1,570 (91.08%) |
| Actual WASM, complete linked source | 8,342/11,522 (72.40%) | 1,566/2,123 (73.76%) | 1,093/1,550 (70.52%) |
| C# Debug | 6,975/7,487 (93.16%) | 3,490/3,934 (88.71%) | Not reported by this collector |
| C# Release | 5,356/5,673 (94.41%) | 3,467/3,915 (88.56%) | Not reported by this collector |

These are official integer totals from complete LLVM/Coverlet runs, rounded to
two decimals. Sequential builds stay within the container limit with zero
OOM/OOM-kill events. See [the recipe](../../TRANSACTION-UNDO.md) for compatibility,
full-image/per-write costs and truthful-flush assumptions. Logical restoration
does not repair unreadable/torn physical files or verify physical power loss.
Whole-project 100% coverage, efficient structures, finer composition and wider
strategies remain unfinished. New exact-head CI is pending publication.

All nine row-undo workflows pass on `f95f59d` (created 2026-10-25
15:24:00–15:24:01 UTC after that commit). The preceding C# formatter failure
is preserved with its exact SDK and log lines. See row-wal-ci.txt and
row-wal-format-ci-before.txt under the PR evidence directory.


## Undo image replay compatibility and restoration verification

This batch extends tests and documents existing behavior; it changes no
production logic. The new controls pass on the published row-undo behavior.

- Row/name adapters acknowledge a restore without applying it. Cleanup fails
  twice, preserves write/admission fences and is retried after adapter repair.
  Independently persisted partial replays recover twice with exact rows/names;
  a following edit and reopening still work. Both languages cover each kind.
- Legal unnamed images restore rows without replacing unrelated external names.
  Empty named maps remain distinct and clear names. Following edits survive.
- Sparse compatible adapters restore the shared 2^32-address image twice, undo
  a selected owner's edits and perform a following allocation. C# bounds creator
  calls to eight; Rust directly reserves sparse addresses. No dense 2^32 probe.
  Portable Rust also covers selected unnamed owners and runs on actual WASM.
- Six explicit/automatic cases distinguish unreported replacement, allocation
  and deletion. Complete row counts, reverse/forward public history and two
  independent committed-redo reopenings verify each result.

Focused: 20 native Rust tests (one child entry ignored), 112 C# cases, 19 actual
WASM cases. Full checks run sequentially within the 2.92 GiB container:
- C# Debug/Release with Coverlet: 1,909 each, zero failed/skipped.
- Native Rust all-features LLVM branch coverage: 823 standard passes, zero fail;
  twelve ignored child entry points are exercised by bounded parents. The
  custom stdout harness also passes.
- Actual WASM coverage: 481 pass, zero failed/ignored.
- JS: 111 tests; all 17 production modules reach the enforced 100% line, branch
  and function thresholds. Strict TypeScript passes. Repository checks: 217.
- Native/WASM formatting and strict Clippy, C# formatting, source sizes, release
  metadata and whitespace pass.
- Generated actual-WASM runtime conformance passes Node, Bun and Deno.
- Persistent Rust -> C# -> JS group examples reopen snapshot/delta journals;
  ordinary native row/name/history examples pass under both undo policies.
- No OOM/OOM-kill events; heavy compilers run sequentially and reclaim occurs.

The initial repository-check attempt rejected root-level downloaded CI logs.
The unchanged logs were moved under experiments/issue-138/ci-logs; all 217
checks then passed. No source correction or test exclusion was needed.

Official complete-run integer totals (no filtered denominator or diagnostic union):

| Scope | Covered lines | Covered branches | Covered functions |
| --- | --- | --- | --- |
| Native Rust | 11,107/11,968 (92.81%) | 1,999/2,412 (82.88%) | 1,430/1,570 (91.08%) |
| Actual WASM, complete linked source | 8,350/11,522 (72.47%) | 1,571/2,123 (74.00%) | 1,093/1,550 (70.52%) |
| C# Debug | 6,978/7,489 (93.18%) | 3,498/3,934 (88.92%) | Not reported by this collector |
| C# Release | 5,357/5,673 (94.43%) | 3,473/3,915 (88.71%) | Not reported by this collector |

All three WASM wrappers measure 100% lines/branches/functions. Complete native,
linked-WASM and C# coverage remains below 100%. Logical images still require
readable storage, compatible replay and truthful flushes. These controls do not
establish physical page repair or real power-loss safety. Efficient WAL/state
structures, finer composition and wider competitor/cluster requirements remain
unfinished. Fresh exact-head CI is required after publication.


## Optional shared immutable state and retained paths

Flat remains the default. The independently selected Shared strategy uses
rpds 1.2.1's Arc-based ordered map in Rust and the existing .NET immutable sorted
dictionary in C#. Rust-backed JS/TS/WASM exposes stateStructure: 'shared'. The
existing engine1 wire and public snapshot interfaces remain compatible.

Reproduce the finite allocation control:
CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 cargo run --manifest-path rust/Cargo.toml --example engine_state_copy

Before implementation, the existing flat engine shares zero of 4,080 unchanged
row references across 256 named rows and sixteen one-row commits. Afterward,
Flat still shares 0/4,080 and Shared shares 4,080/4,080. Every step verifies row
values and the pinned original. C# independently counts immutable AVL nodes,
requiring unchanged subtrees across live and replayed revisions. These controls
measure identity, rather than elapsed time or complete-process allocation.
Feature contracts were written before the option existed; the missing-API
compile failure is separate from the runnable pre-implementation cost baseline.

Focused validation: eight native portable semantic/cost cases, two native
private root/cache/capability checks, eighteen C# cases and the same eight
portable cases in actual WASM. Public WASM option-validation also passes.
Independent 96-step row/name oracles retain every earlier snapshot. Contracts
cover all supported admission/isolation/validation combinations, all selected
constraints, no/one/all history, no-op/rollback, zero/maximum and sparse addresses,
append/flush uncertainty, mixed parent/branch choices, saved-base resume, grouped
commits, following edits and existing portable revision vectors.

Internal Rust reads, validation, encoding, branches and WASM export never request
the lazy public BTreeMap compatibility copy. Explicit materialization remains
O(N), shared only by clones of that snapshot; new candidates start uncached.
Readonly commit/rollback preserves the original root. C# exposes the immutable
IReadOnlyDictionary directly, with frozen candidates rejecting later mutation.
Native Send/Sync controls pass. The resolved rpds tree source has the exact
SHA-256 of the pinned primary source in RESEARCH.md; both locked dependency
graphs pass metadata validation. No minimum-compiler run is claimed.

Complete suites run sequentially within the 2.92 GiB container:
- Native Rust all-features LLVM branch coverage: 833 standard passes, zero fail;
  twelve ignored child entries are exercised by bounded parents. Custom stdout
  harness passes.
- C# Debug and Release with Coverlet: 1,927 each, zero failed/skipped.
- Actual WASM with coverage: 489 pass, zero failed/ignored.
- JS: 111 tests; all 17 production modules reach enforced 100% line, branch and
  function thresholds. Strict TS and 217 repository checks pass.
- Formatting, strict native/WASM Clippy, source sizes, release fragments,
  dependency lock metadata and whitespace pass.
- Generated actual-WASM conformance passes Node, Bun and Deno, including
  flat/shared snapshot/delta reopening, group outcomes, mixed resumed branches
  and invalid option types.
- Persistent Rust -> C# -> JS examples reopen the same journals under all four
  flat/shared and snapshot/delta combinations with read-set/pessimistic options.
  Existing group examples and both ordinary undo-policy examples also pass.
- Container OOM/OOM-kill counters remain zero.

Official complete-run totals; no filtered denominator or diagnostic union:

| Scope | Covered lines | Covered branches | Covered functions |
| --- | --- | --- | --- |
| Native Rust | 11,227/12,088 (92.88%) | 2,002/2,414 (82.93%) | 1,450/1,590 (91.19%) |
| Actual WASM, complete linked source | 8,471/11,643 (72.76%) | 1,574/2,125 (74.07%) | 1,112/1,569 (70.87%) |
| C# Debug | 7,019/7,534 (93.16%) | 3,536/3,972 (89.02%) | Not reported by this collector |
| C# Release | 5,390/5,710 (94.40%) | 3,511/3,953 (88.82%) | Not reported by this collector |

All three WASM wrappers measure 100% lines/branches/functions. Complete native,
linked-WASM and C# coverage remains below 100%. Full revision encoding, selected
constraint scans, complete-record delta comparison, history/conflict metadata
and group record buffers remain costs. Structural sharing does not implement
page WAL, LSM layout, physical torn-page repair or real power-loss verification.
Finer composition and wider competitor/cluster requirements remain open. The
native issue goal stays active; fresh exact-head CI is required after publication.


## Incremental logical revision records

Snapshot remains the default encoding. Independently selected Changes emits
canonical engine2 records containing the exact predecessor revision, allocator,
sorted complete puts/deletes and checksum. Mixed engine1/engine2 streams reopen
under either future encoding. Shared recovery applies immutable changed paths
without constructing a full decoded row map; Flat retains its copy cost.

Runnable pre-implementation replay failures and finite serialization baselines
are preserved in changes-before.txt. These fail on unsupported engine2 records,
separately from any missing-option compile failure. With 256 named rows and
sixteen one-row commits, both earlier Flat and Shared serialize 4,096 rows /
966,063 bytes. Changes serializes sixteen rows / 4,246 bytes under either map.
The reusable revision-record-probe.mjs checks old pins and exact reopening with
a 128 MiB Node heap; no timing or physical-durability measurement is claimed.

Independent Python-authored vectors contain valid, mixed and 41 malformed u32
streams, plus valid/invalid physical delta composition. Native Rust, C# and
actual WASM consume the same fixture. Tests verify canonical grammar, null/empty/
Unicode/NUL/newline names, exhausted allocators, read-only invalid-input rejection
and unchanged supplied logs. Older readers reject the mandatory format tag.
Branch exports remain standalone engine1 images; mixed branch replay attributes
saved bases after history eviction. Logical change records pass unchanged through
the physical delta decorator and reset its full-image comparison base.

Seven new portable native tests also execute in actual WASM. Contracts exercise
all map/encoding choices, admission/isolation/validation combinations, selected
constraints, histories, no-op/transient effects, maximum addresses, mixed branches,
groups, following edits and reopening. Empty operations preserve accepted identical
results and allocator-only revisions without erasing conflict/ABA information.
Each native language tests 32 independent durable group images and forty bounded
actual child exits across map/encoding/physical choices. Opposite-encoding reopen,
following writes and a second reopen verify that outcomes remain discoverable.
Child deadlines and guaranteed reaping bound these experiments.

Complete suites run sequentially within the 2.92 GiB container:
- Native Rust all-features LLVM branch coverage: 840 standard passes, zero failed;
  twelve ignored child entries are exercised by bounded parents. Custom stdout
  harness passes.
- C# Debug and Release with Coverlet: 1,946 each, zero failed/skipped.
- Actual WASM with coverage: 496 pass, zero failed/ignored.
- JS: 113 tests; all 17 production modules reach enforced 100% line, branch and
  function thresholds. Strict TS and 217 repository checks pass.
- Formatting, strict native/WASM Clippy, source sizes, release fragments and
  whitespace pass.
- Actual generated-WASM conformance passes Node, Bun and Deno, including mixed
  encodings, malformed records and public option validation.
- Persistent Rust -> C# -> JS examples reopen the exact same journal under all
  eight Flat/Shared x Snapshot/Changes x plain/delta combinations, with read-set
  validation and pessimistic admission. Existing group examples and both ordinary
  undo-policy examples also pass.
- Container OOM/OOM-kill counters remain zero.

Official complete-run totals; no filtered denominator or diagnostic union:

| Scope | Covered lines | Covered branches | Covered functions |
| --- | --- | --- | --- |
| Native Rust | 11,394/12,252 (93.00%) | 2,049/2,462 (83.23%) | 1,467/1,609 (91.17%) |
| Actual WASM, complete linked source | 8,636/11,811 (73.12%) | 1,619/2,173 (74.51%) | 1,129/1,588 (71.10%) |
| C# Debug | 7,109/7,624 (93.25%) | 3,643/4,078 (89.33%) | Not reported by this collector |
| C# Release | 5,460/5,780 (94.46%) | 3,612/4,055 (89.08%) | Not reported by this collector |

All three WASM wrappers measure 100% lines/branches/functions. Whole native,
linked-WASM and C# coverage remains below 100%. Selected constraint scans,
conflict/history metadata, complete-record physical delta comparisons and group
buffers remain costs. Dependent records require their complete logical journal
prefix; no checkpoint truncation or automatic compaction is supplied. Logical
redo does not implement a physical pager, doublewrite, LSM, torn-page repair or
real power-loss verification. Finer independent decorator composition and wider
competitor/cluster requirements remain open. The native issue goal stays active;
fresh exact-head CI is required after publication.


## Optional SQLite physical journal and Unicode preservation

The selected SQLite component supplies a maintained physical pager, checksummed
page WAL, WAL/FULL commit and explicit checkpoint counters at the replaceable
ordered-log boundary. Rust selects an optional bundled component; C# owns an
application-selected DbConnection; JS exports a separate provider-neutral entry.
The bare/default Rust, universal JS and actual WASM paths import no SQLite
provider. One engine owns cached state; stale independent owners reject rather
than claiming cross-engine MVCC. The [recipe](../../ENGINE-SQLITE.md) documents
same-host VFS assumptions, sidecar ownership, finite provider retries and costs.

First-authored adapter contracts precede implementation. Independent schema,
sequence and UTF-8 BLOB fixtures, pending/committed visibility, stale/busy owners,
pinned SQL readers, setting drift, malformed payloads, constructor/close cleanup,
finite storage-full and before/after COMMIT faults verify exact committed prefixes.
Forty finite real child exits per native language cover both maps, logical
encodings and physical delta settings; opposite-encoding reopen and following
writes pass. Intact/torn/checksum-damaged WAL copies compare actual recovery,
quick_check and following commits. These do not simulate physical power loss.

The work also reproduced nine existing JS payload regressions before changing
TextDecoder: a leading U+FEFF was stripped from literals, whole/fragmented framing
and file journals. ignoreBOM:true preserves accepted opaque data and fatal UTF-8
validation remains. Four new-adapter C# controls first expose missing numeric
provider rows silently converted to zero; explicit rejection, owned cleanup,
reopening and following writes now pass. Before evidence is in sqlite-before.txt.

Complete sequential suites pass: 852 native Rust standard tests plus the custom
stdout harness, 1,989 C# tests in each Debug/Release configuration, 496 actual-WASM
tests, 168 JS tests, strict TS and 217 repository checks. Formatting, strict
native/WASM Clippy, source sizes, release fragments, actionlint/ShellCheck and
latest-dependency checks pass. All eighteen JS production modules and three
WASM wrapper files reach 100% measured lines/branches/functions. OOM counters
remain zero. The final C# reruns include all 43 focused SQLite cases and the
10.0.14 provider; earlier C# totals are not substituted for these final results.

Generated WASM conformance passes Node, Bun and Deno. Existing persistent group,
ordinary name/undo and eight cross-language engine configurations pass. All eight
SQLite configurations also reopen the same database in Rust -> C# -> Node -> Bun
-> Deno order, checking prior names/rows/revisions, following independent group
commits, pins and checkpointing. Actual versions are recorded in sqlite.txt.

Official complete-run coverage, without denominator filtering or diagnostic unions:

| Scope | Covered lines | Covered branches | Covered functions |
| --- | --- | --- | --- |
| Native Rust | 11,643/12,506 (93.10%) | 2,099/2,524 (83.16%) | 1,496/1,638 (91.33%) |
| Actual WASM, complete linked source | 8,636/11,811 (73.12%) | 1,619/2,173 (74.51%) | 1,129/1,588 (71.10%) |
| C# Debug | 7,248/7,763 (93.37%) | 3,751/4,190 (89.52%) | Not reported by this collector |
| C# Release | 5,578/5,898 (94.57%) | 3,718/4,165 (89.27%) | Not reported by this collector |

The selected native SQLite component measures 98.03% lines / 80.65% branches /
100% functions. C# SQLite lines reach 100%, with its Debug class reporting 96.42%
branches. Full native, linked-WASM and C# coverage remains below 100%. SQLite
retains complete logical journals and does not repair legacy mapped files.
Doublewrite, LSM, real power loss, finer independent composition and wider
cluster/competitor requirements remain open. See SQLITE-JOURNAL-PLAN.md and the
acceptance inventory. All nine workflows pass on SQLite head `3322f36`; see
sqlite-ci.txt. Later changes require fresh exact-head CI. The native issue goal
remains active and PR 139 stays draft.


## Independently selected constraint indexes

Reproduce with experiments/issue-138/validate-constraint-index.sh after the
documented validation setup, then summarize-constraint-index.py. Compilers run
sequentially within the 2.92 GiB limit. An unnecessary Vec in the new Rust test
first failed strict Clippy; a fixed array corrects it. The --resume-rust path
retains the completed C# reports after that test-only correction and reruns all
later validation. Only completed successful suites supply the final totals below.

Scan remains the default. Independently selected Indexed retains persistent
pair/name-owner and incoming-reference-count maps only for selected constraints.
Private candidates remove every old changed key before adding new keys, allowing
atomic swaps. Publication follows journal append and flush; rejected or uncertain
candidates retain committed roots. Recovery validates every historical record
before building the final indexes. The journal format is unchanged.

Before-implementation native/C# contracts fail because the configuration API is
absent; constraint-index-before.txt preserves that evidence without claiming a
scan semantic defect. Each native language independently checks 6,400 finite
candidate edits against a simple full-state oracle across all constraint subsets,
both maps and both logical encodings. Tests cover zero/absent/self/cyclic refs,
both incoming-reference positions, unchanged incoming rows, Unicode/NUL/empty
names, swaps/transfers, transient rows, rejected/following writes, mixed replay,
historical invalidity, pins, groups, branches and independent admission/read
validation. The six portable Rust contracts also execute in actual WASM.
Existing all-prefix durable group fault matrices now run with scan and indexed
unique-name validation. C# additionally checks before/after append and flush
uncertainty with exact committed root identity and recovered following writes.

Finite cost controls verify only selected map allocation and retained root
sharing. C# forbids all candidate enumeration and bounds one-row preparation to
seven row lookups at 8 and 512 rows. The reusable native allocation example uses
shared rows, changed-row encoding and all constraints for eight name edits.
Scan versus indexed requests 54,805 versus 17,845 bytes at 128 rows, and 329,269
versus 20,117 at 1,024 rows in this Debug run. These count allocation requests,
including reallocations, excluding setup/recovery; they are neither live-memory
nor latency/durability measurements. Exact totals can vary with toolchains.

Complete local validation:

- Native Rust all-features branch coverage: 859 standard passes, zero failed;
  thirteen ignored child entries are exercised by bounded parent tests. The
  custom stdout harness passes.
- C# Debug and Release with Coverlet: 2,003 each, zero failed/skipped.
- Actual WASM with LLVM coverage: 502 pass, zero failed/ignored.
- JS: 168 tests; all eighteen production modules enforce 100% line/branch/function
  coverage. Strict TypeScript and 217 repository checks pass.
- Formatting, strict native/WASM Clippy, source sizes, release fragments,
  actionlint/ShellCheck, whitespace and 117 latest declared dependencies pass.
- Generated actual-WASM conformance passes Node, Bun and Deno, including the
  public scan/indexed option, invalid types, swaps, groups, branches and replay.
- All sixteen SQLite scan/indexed x flat/shared x snapshot/changes x plain/delta
  compositions reopen the same database through Rust -> C# -> Node -> Bun ->
  Deno. Both strategies select References only: repeated provider pairs/names
  intentionally remain valid. All three runtime SQLite adapter suites pass.
- Existing persistent strategy/group examples and both ordinary undo policies
  pass. The finite allocation example passes. Container OOM/OOM-kill remain zero.

Official complete-run totals, without filtered denominators or diagnostic unions:

| Scope | Covered lines | Covered branches | Covered functions |
| --- | --- | --- | --- |
| Native Rust | 11,778/12,645 (93.14%) | 2,143/2,570 (83.39%) | 1,506/1,648 (91.38%) |
| Actual WASM, complete linked source | 8,775/11,954 (73.41%) | 1,665/2,219 (75.03%) | 1,139/1,598 (71.28%) |
| C# Debug | 7,323/7,838 (93.43%) | 3,843/4,282 (89.75%) | Not reported by this collector |
| C# Release | 5,629/5,949 (94.62%) | 3,810/4,257 (89.50%) | Not reported by this collector |

ConstraintIndex component: Native Rust: 117/117 lines, 42/42 branches, 10/10 functions; Actual WASM: 117/117 lines, 42/42 branches, 10/10 functions. C# Debug/Release class summaries report 100% lines and branches.

All three actual-WASM wrapper files measure 100% lines, reported branches and
functions. Whole native Rust, linked WASM and C# remain below the required 100%.
Selected indexes do not remove flat-map copies, complete snapshot encoding,
historical recovery scans, branch comparisons or physical I/O. Finer independent
composition, legacy physical page repair, LSM, real power-loss verification,
automatic balancing/multi-catalog transactions and wider competitor criteria
remain unfinished. The native issue goal stays active and PR 139 remains draft;
fresh exact-head CI is required after publication.


## Optional immutable journal object cache

Reproduce with experiments/issue-138/validate-object-cache.sh after the
documented tools/etcd setup. The final constructor compatibility correction
was made after initial C# builds began: validate-object-cache-csharp-api.sh
reruns formatting and both full configurations with the corrected source.
summarize-object-cache.py and this evidence use only final successful reports.
Compilers run sequentially within the 2.92 GiB container limit.

The independently selected zero-default budget retains checked immutable
objects by partition/address/object generation. Every catalog snapshot remains
fresh. Returned arrays are independent; publication/staging still rereads and
compares complete data. Failed lookups never enter the cache; oversized objects
do not evict retained entries. Positive object/record overhead bounds empty
payloads. The existing C# three-argument constructor retains its binary signature.
Rust reuses audited lru 0.19.0 with default features disabled; C#/JS use standard
containers. No cache container or retained payload copy is allocated at zero.

object-cache-before.txt preserves absent-API Rust/C# failures and JS download
counter failures before implementation. These establish missing optional costs,
not an isolation defect. The first LRU test sequence accidentally reread a before
probing eviction; the corrected a,b,a,c,b,c,a sequence measures promotion and
eviction without that extra promotion. Final tests use the corrected oracle.
Each language independently compares 768 reads across six budgets with explicit
UTF-8 payload weights. Tests additionally cover empty objects, independent data
partitions, caller mutation, fresh catalog generations/removal/failure, wrong
object generations, corrupt fetch/repair, cached-data outages, staging collision
checks and rejected/failed/lost-response publication. C#/JS force overlapping
misses and verify one retained copy and correct eviction accounting.

Complete local validation:

- Native Rust all-features coverage: 866 standard passes, zero failed;
  thirteen ignored child entries are exercised by bounded parent tests.
  The custom stdout harness passes.
- C# Debug and Release with Coverlet: 2,016 each, zero failed/skipped.
- Actual WASM with LLVM coverage: 509 pass, zero failed/ignored, including
  every portable cache contract.
- JS: 177 tests; all eighteen production modules enforce 100% lines,
  branches and functions. Strict TypeScript and 221 repository checks pass.
- Formatting, strict native/WASM Clippy, source sizes, release fragments,
  actionlint/ShellCheck, whitespace and 118 latest declared dependencies pass.
- Generated actual-WASM conformance passes Node, Bun and Deno.
- Real etcd 3.7.3 interoperability passes with default zero and 65,536 budgets:
  Rust/C#/WASM use the same catalog and separate east/west data groups. A
  repeated cached WASM begin issues one catalog request and zero data requests.
  Atomic conflicts, lost responses, unavailable west/no publication, independent
  east commits, cached west reads during its data outage, catalog leader failover
  and majority-loss rejection even for cached shards all pass.
  Five finite etcd children are stopped and reaped after each run.

Official complete-run totals, without filtered denominators or diagnostic unions:

| Scope | Covered lines | Covered branches | Covered functions |
| --- | --- | --- | --- |
| Native Rust | 11,837/12,704 (93.18%) | 2,151/2,578 (83.44%) | 1,513/1,655 (91.42%) |
| Actual WASM, complete linked source | 8,834/12,013 (73.54%) | 1,673/2,227 (75.12%) | 1,146/1,605 (71.40%) |
| C# Debug | 7,360/7,875 (93.46%) | 3,862/4,300 (89.81%) | Not reported by this collector |
| C# Release | 5,658/5,978 (94.65%) | 3,828/4,275 (89.54%) | Not reported by this collector |

ObjectCache component: Native Rust: 39/39 lines, 4/4 branches, 4/4 functions; Actual WASM: 39/39 lines, 4/4 branches, 4/4 functions.
C# Debug/Release class summaries report 100% lines and branches. All three
actual-WASM wrappers report 100% lines, branches and functions.

Whole native Rust, linked WASM and C# remain below the required 100%. Cache
payload weights do not measure process heap. Complete begin replay, updated
journal transfers and persistent old/orphan objects are unchanged. Immutable
namespaces must not be rewritten/deleted; cached hits cannot detect later
out-of-protocol tampering. Catalog availability remains required. Finer
composition, legacy physical repair, LSM, real power loss, automatic balancing,
multi-catalog transactions and wider competitor requirements remain unfinished.
The issue goal stays active and PR 139 stays draft; new code needs exact-head CI.


## Unicode sequence graph integrity

Reproduce with experiments/issue-138/validate-unicode-graph.sh and summarize-
unicode-graph.py after the documented LLVM/wasm-bindgen tooling setup. C#
Debug/Release completed before strict WASM Clippy rejected a nested test module
with the same name as its parent. Renaming only the helper to graph_contract
fixes that composition lint. The resumed driver reuses unchanged completed C#
reports and runs every remaining check; final results use successful reports.
Compilers run sequentially within the 2.92 GiB container budget.

unicode-graph-before.txt preserves genuine before failures: both native
languages hang on a finite self-cycle; Rust silently returns Z for (missing,Z).
Bounded children use memory/CPU limits and parent deadlines and are reaped.
The corrected walkers track the current path with enter/exit frames. Missing
nodes and cycles reject; shared subtrees expand once per occurrence. No traversal
state survives an error. C# reuses its installed Unicode converter through
ISequenceWalker; Rust keeps the Vec-returning public compatibility wrapper and
adds try_walk. Existing UTF-16 and converter memoization policies are preserved.

Portable contracts cover both child directions, self/two-node cycles, repeated
errors, following healthy reads, names, Unicode order, shared DAGs, 2048-level
finite graphs, missing nodes, invalid symbols and UTF-16 policies. Native tests
also verify persistent reopening twice; Rust preserves file bytes on rejected
reads. The C# bounded cases exercise GetString and its public sequence converter.
Actual WASM executes the same portable Rust cases with the maintained test
runner and an explicit 256 MiB test-module linear-memory cap. The independent verify-wasm-memory-cap.py parser
checks the actual memory section of all six executed modules: each declares
a maximum of 4096 64-KiB pages (256 MiB).

Complete local validation:

- Native Rust all-features LLVM coverage: 875 passes, zero failed; fourteen
  ignored child entries are exercised by bounded parent tests. The custom
  stdout harness passes.
- C# Debug and Release with Coverlet: 2022 each, zero failed/skipped.
- Actual WASM with LLVM coverage: 516 passes, zero failed/ignored.
- JS: 177 tests; all eighteen production modules enforce 100% lines, branches
  and functions. Strict TypeScript and 221 repository checks pass.
- Native/WASM formatting and strict Clippy, C# formatting, release fragments,
  source sizes, actionlint, ShellCheck and whitespace checks pass.
- Generated actual-WASM client conformance passes Node, Bun and Deno. The
  universal JS API has no direct typed sequence-graph decoder; no new API is
  added merely for this integrity check.

Official complete-run totals, without filtered denominators or diagnostic unions:

| Scope | Covered lines | Covered branches | Covered functions |
| --- | --- | --- | --- |
| Native Rust | 11,911/12,717 (93.66%) | 2,165/2,580 (83.91%) | 1,529/1,657 (92.28%) |
| Actual WASM, complete linked source | 8,908/12,026 (74.07%) | 1,687/2,229 (75.68%) | 1,162/1,607 (72.31%) |
| C# Debug | 7,382/7,891 (93.55%) | 3,873/4,310 (89.86%) | Not reported by this collector |
| C# Release | 5,676/5,992 (94.73%) | 3,840/4,285 (89.61%) | Not reported by this collector |

Checked walker component: Native Rust: 35/35 lines, 8/8 branches, 4/4 functions; Actual WASM, complete linked source: 35/35 lines, 8/8 branches, 4/4 functions.
C# Debug/Release class summaries report 100% lines and branches. All three
actual-WASM wrappers retain 100% lines, reported branches and functions.

Whole native Rust, linked WASM and C# remain below the required 100%. This fix
adds O(depth) traversal metadata and does not limit decoded output from valid
DAGs. Cached converter results do not retraverse modified links. No stored-format
change, repair or new power-loss guarantee is supplied. Finer composition,
legacy physical repair, LSM, physical power loss, full journal transfer/replay,
persistent object retention, automatic balancing, multi-catalog transactions
and wider competitor requirements remain unfinished. The goal remains active
and PR 139 remains draft; publication requires fresh exact-head CI.


## Deep structure output

Reproduce with experiments/issue-138/validate-structure-depth.sh and summarize-
structure-depth.py after the documented LLVM/wasm-bindgen tooling setup. Heavy
compilers run sequentially with one Rust build job and bounded .NET heaps.

structure-depth-before.txt preserves real automated failures: a finite 2048-node
source chain overflows isolated native stacks in concrete Rust, generic Rust
and C#. The child processes have finite memory/CPU limits, parent deadlines and
dumps disabled; all are reaped. Corrected entry points use explicit frames and
one growable output buffer. Target references remain unexpanded. Both Rust
entry points reuse one formatter, and C# uses its standard StringBuilder.

Independent shallow oracles cover names/quoting, self/two-node source cycles,
shared children, missing roots, exact deepest-first name reads and failures at
every reference position. Rust also verifies caller-provided visited sets and
error path progress. Native bounded tests verify exact output at depth 2048;
portable Rust cases do the same in actual WASM. Ordinary Unicode named stores
reopen twice in both native languages. The independent existing output suites
also pass. A broader initial C# test fixture aborted in usage repointing when
merging an existing cyclic pair during setup; creating a separate self-cycle
corrects the fixture. That execution did not reach the output formatter.

Complete local validation:
- Native Rust all-features LLVM coverage: 882 passes, zero failed; fifteen
  ignored child entries are exercised by bounded parents. Custom stdout passes.
- C# Debug and Release with Coverlet: 2027 each, zero failed/skipped.
- Actual WASM with LLVM coverage: 520 passes, zero failed/ignored; all six
  executed modules declare the independently verified 256 MiB linear-memory cap.
- JS: 177 tests and all eighteen modules at enforced 100% lines/branches/functions.
  Strict TypeScript and 221 repository checks pass.
- Native/WASM/C# formatting, strict native/WASM Clippy, release fragments,
  source sizes, ShellCheck, actionlint and whitespace checks pass.
- Node, Bun and Deno generated actual-WASM client conformance passes.

Official complete-run coverage (no filtered project denominator):

| Scope | Covered lines | Covered branches | Covered functions |
| --- | --- | --- | --- |
| Native Rust | 11,946/12,749 (93.70%) | 2,165/2,574 (84.11%) | 1,536/1,664 (92.31%) |
| Actual WASM, complete linked source | 8,966/12,058 (74.36%) | 1,691/2,229 (75.86%) | 1,172/1,614 (72.61%) |
| C# Debug | 7,398/7,892 (93.74%) | 3,882/4,314 (89.99%) | Not reported by this collector |
| C# Release | 5,689/5,995 (94.90%) | 3,849/4,289 (89.74%) | Not reported by this collector |

Shared Rust formatter component: Native Rust: 39/39 lines, 6/6 branches, 1/1 functions; Actual WASM, complete linked source: 39/39 lines, 6/6 branches, 1/1 functions.
All three actual-WASM wrappers retain 100% lines/reported branches/functions.
The C# output-file component includes its other existing output methods; inspect
its complete Coverlet report rather than treating file coverage as method scope.

Metadata costs O(depth) plus resolved reference text and output. Assembly is
linear in output bytes; store lookups and name decoding retain their own costs.
No decoded-output limit, new stored representation, repair or stronger physical
durability is added. A separate observed legacy Rust inline text-name whitespace
truncation remains for the next atomic audit in this same PR; this traversal's
persistent controls use the ordinary Unicode named store.

Whole-project native Rust, C# and linked WASM coverage remain below 100%. Finer
composition, legacy physical repair, LSM, real power loss, full journal replay,
persistent object retention, balancing, multi-catalog transactions and wider
competitor requirements remain unfinished. Keep the goal active and PR draft;
publication requires fresh exact-head CI.


## Exact native text names and atomic reload

Exact native text names and atomic reload: complete local validation
PR: https://github.com/link-foundation/link-cli/pull/139
Date: 2026-10-26 UTC
Before implementation: faf191d10f1dc6fab561dd46dafb0d67561e0963

Reproduce with experiments/issue-138/validate-inline-names.sh, summarize-inline-
names.py and document-inline-names.py after setup-resumed-validation.sh. Heavy
compilers run sequentially, Rust uses one build job and .NET heaps are bounded.

The seven initial automated tests all fail before the correction; see
inline-names-before.txt. Whitespace truncates, quote edges disappear, a line
break creates an additional row, absent-address names vanish and invalid UTF-8
after a valid row discards the old live state during reload. The complete-name
and failed-reload assertions now pass, using both plain and selected indexes.

Four additional native contracts verify independent exact metadata bytes,
eighteen malformed/future cases with both index settings, following rename,
name movement/removal, deletion/reuse, legacy optional delimiters and repeated
reopening of 64 finite mixed-Unicode names. Names include empty strings, NUL,
CR/LF/tab/DEL, quotes, backslashes, literal-looking syntax, BOM and Unicode.
A portable memory-reload control also runs in actual WASM. Existing Unicode
names, history, versioning, query and output contracts remain in the full suites.
The runnable rust/examples/text_names.rs passes separately after the suite
with --no-default-features, including exact multiline/Unicode/zero-address names.

A separate existing TCP race test failed during initial full C# Release
validation. tcp-shutdown-before.txt preserves the actual NullReferenceException
in TcpListener.AcceptTcpClient. Commit 7e882d7 handles that known runtime race
only after shutdown starts. Both complete C# configurations then pass the same
200-connection regression without extending timeouts; TCP-SHUTDOWN-PLAN.md traces
its primary runtime source and records the finite scheduling limit.

Complete local validation:
- Native Rust all-features LLVM coverage: 894 passed, zero failed; fifteen
  ignored child entries are exercised by bounded parents. Custom stdout passes.
- C# Debug and Release with Coverlet: 2027 each, zero failed/skipped.
- Actual WASM with LLVM coverage: 521 passed, zero failed/ignored; six executed
  modules declare the independently verified 256 MiB linear-memory cap.
- JS: 177 tests and all eighteen modules at enforced 100% lines/branches/functions;
  strict TypeScript and 221 repository checks pass.
- Native/WASM/C# formatting, strict native/WASM Clippy, release fragments,
  source sizes, ShellCheck, actionlint and whitespace checks pass.
- Node, Bun and Deno generated actual-WASM client conformance passes.

Official complete-run coverage, with the whole project denominator:

| Scope | Covered lines | Covered branches | Covered functions |
| --- | --- | --- | --- |
| Native Rust | 12,001/12,799 (93.77%) | 2,185/2,588 (84.43%) | 1,546/1,674 (92.35%) |
| Actual WASM, complete linked source | 8,962/12,108 (74.02%) | 1,693/2,231 (75.89%) | 1,171/1,624 (72.11%) |
| C# Debug | 7,398/7,895 (93.70%) | 3,882/4,314 (89.99%) | Not reported by this collector |
| C# Release | 5,689/5,997 (94.86%) | 3,849/4,289 (89.74%) | Not reported by this collector |

Native text format component: 46/46 lines, 16/16 branches, 9/9 functions.
All three actual-WASM wrappers retain 100% measured lines/branches/functions.
The native text-file parser is not exercised through a WASM filesystem; its
linked-source coverage is retained in the complete WASM denominator.

The versioned comment namespace reuses the maintained UTF-8 literal codec;
legacy unquoted ~1{31} and raw backslashes retain literal meanings. Safe quoted
rows retain their readable representation. Older readers ignore the extension
and can lose its names when saving: updated readers are required. Metadata does
not allocate rows. Invalid/future metadata rejects without rewriting the file.
Reload holds the old and candidate states until complete load/index construction.
Parsing/encoding are linear in bytes; M metadata entries add O(M log M) sorting.
The same replacement/flush protocol and cooperating writer ownership remain;
no physical repair or stronger save() durability is claimed. TEXT-STORAGE.md
specifies compatibility, persistence assumptions and the exact example bytes.

Whole-project Rust/C#/linked-WASM coverage remains below 100%. Native physical
repair, page undo, LSM, finer composition, full journal replay/updates, persistent
object retention, balancing, multi-catalog transactions and the other inventoried
acceptance criteria remain unfinished. Keep the goal active and PR draft;
publication requires fresh exact-head CI. Native page prototype experiments are
separate future implementation evidence, not part of these production results.

Native page JS/TS integration: 197 full unit tests, 100% measured production
lines/branches/functions across all 20 modules, strict types, 221 repository
checks and Node/Bun/Deno actual-WASM conformance pass. Independent binary vectors,
finite I/O/cleanup faults, bounded child exits and fixed new-group byte counters
are retained. See native-pages-js.txt. This is JS provider evidence; matching
Rust/C# providers, redundant physical repair and fresh CI are still required.


## Native single-copy and mirrored page parity — current local run

This section supersedes the initial provider-only status above. Complete local
mirror validation passes 922 native Rust tests (17 child entries are exercised
by bounded parents), 539 actual WASM tests, 2,060 C# tests per configuration and
209 JS tests. JS retains enforced 100% production line/branch/function coverage
on Node 26 and Node 24. Strict native/WASM Clippy, formatting, optional-SQL guards,
strict TypeScript and 221 repository checks pass in the recorded provider runs.

The public shared-file recipe passes all 32 single/mirrored policy configurations
and 192 writer invocations through Rust -> Node -> Bun -> C# -> Deno -> Rust.
Each mirrored handoff damages one closed copy, and the next writer repairs data
and metadata before another group. Every final file has twelve exact revisions
and prior names. Independent vectors, persistence/repair faults before and after
effects, repeated reopening, ownership cleanup and bounded real process exits
are recorded in mirrored-pages-{js,rust,csharp}.txt.

| Complete measured scope | Covered lines | Covered branches | Covered functions |
| --- | --- | --- | --- |
| Native Rust | 12,812/13,643 (93.91%) | 2,381/2,816 (84.55%) | 1,638/1,771 (92.49%) |
| Actual WASM, complete linked source | 9,540/12,693 (75.16%) | 1,866/2,419 (77.14%) | 1,235/1,690 (73.08%) |
| C# Debug | 7,746/8,248 (93.91%) | 4,178/4,630 (90.24%) | Not reported |
| C# Release | 5,974/6,285 (95.05%) | 4,140/4,603 (89.94%) | Not reported |

The same selected next-record flush after 0/1/32 preceding groups performs two
new page writes, two metadata writes and four syncs, without historical device
reads or writes. The codec holds one page buffer; mirror selection holds two
candidate pages and damaged-index tracking. Open/read still validates history.
One expected surviving copy and successful synchronization remain assumptions;
physical power loss and arbitrary hostile corruption are not certified.

All nine workflows pass on the preceding JS mirror head be89fb7. Initial C#
head 288cc6d passes eight workflows but fails newer formatting and a Windows
exclusive-handle test. Their exact errors and correction are preserved in
mirrored-pages-csharp-ci-before.txt. All nine workflows now pass on complete
native/JS mirror head b0612d0, including both native platform matrices; see
mirrored-pages-parity-ci.txt. Subsequent command changes require fresh CI.
Complete coverage and all other inventoried gaps remain active. The next audit
of public command recovery and version names is in CLI-COVERAGE-PLAN.md.

## Native public command contracts

Before tests reproduce rejected delimiter-containing tag names in both native
executables. Parsing now prefers the earliest delimiter with a resolvable complete
suffix, preserving existing aliases such as alias=release=0. Exact listing,
checkout, repeated ownership, rejection without mutation and following edits
pass. Thirty-one serialized C# entry-point contracts measure real dispatch and
verify exclusive file reopening after every command, including errors. Existing
subprocess integration tests remain. Both native tag-name examples pass.

Complete runs pass 923 native Rust cases (17 bounded child entry points) and
2,091 C# cases in each configuration. No portable source changed since the
539-case actual-WASM run above. Finite nested-query controls confirm the existing
64-level parser limit and healthy reopening/edits after rejection; no new parser
behavior is claimed. See CLI-COVERAGE-PLAN.md and cli-before.txt.

| Complete measured scope | Covered lines | Covered branches | Covered functions |
| --- | --- | --- | --- |
| Native Rust | 12,815/13,646 (93.91%) | 2,381/2,816 (84.55%) | 1,640/1,773 (92.50%) |
| C# Debug | 7,901/8,259 (95.67%) | 4,240/4,632 (91.54%) | Not reported |
| C# Release | 6,072/6,296 (96.44%) | 4,194/4,605 (91.07%) | Not reported |

Whole-project 100% and the wider inventory remain unfinished. The next audit
continues through uncovered public query compatibility paths. Command publication
now passes all nine workflows on 63ccbc7, including native platform matrices;
cli-ci.txt preserves exact SHA, timestamps and links. Later commits require
their own fresh exact-head CI.

## Retained public query compatibility

Thirty-three new contracts pass without production changes: empty/comment/leaf/
incomplete shapes preserve exact rows, names and notifications in all three
retained C# processors, including native reopening and subsequent edits. Mixed
wildcard rejection, argument errors, existing-pair reuse and basic update-count
diagnostics preserve established semantics. The suspected leaf null-list issue
does not reproduce with the installed parser. An initial unsupported // comment
test was corrected to LiNo's # syntax; it is not product before-failure evidence.

Both full configurations pass 2,124 tests, with no failures or skips. All
production files remain measured: Debug 7,938/8,259 lines (96.11%) and
4,259/4,632 branches (91.95%); Release 6,094/6,296 lines (96.79%) and
4,214/4,605 branches (91.51%). See query-compatibility.txt and
QUERY-COMPATIBILITY-PLAN.md. Rust/actual-WASM/JS source is unchanged from the
recorded 923/539/209-test runs. Whole-project coverage and all broader criteria
remain unfinished. All nine workflows pass on query head 580dcb2; see
query-compatibility-ci.txt. Subsequent changes require fresh exact-head CI.

## Shared native and WASM argument contracts

Seven portable public contracts compare complete parsed configurations for
all long/short aliases, exact Unicode/whitespace/delimiter values, optional
booleans, signed/unsigned boundaries, arities, literal queries, error recovery
and independent decorator selection. No production defect reproduced. An
initial expected diagnostic used an alias rather than the established canonical
name; correcting that oracle is not product before-failure evidence.

Complete suites pass 930 native tests and 546 actual WASM tests, with strict
Clippy/format and 221 repository checks. WASM uses the verified 256 MiB linear
memory cap, finite test deadlines and the existing six executables.

| Complete measured scope | Covered lines | Covered branches | Covered functions |
| --- | --- | --- | --- |
| Native Rust | 12,867/13,646 (94.29%) | 2,397/2,816 (85.12%) | 1,644/1,773 (92.72%) |
| Actual WASM, complete linked source | 9,931/12,693 (78.24%) | 1,978/2,419 (81.77%) | 1,259/1,690 (74.50%) |

The parser component measures 404/405 native lines, 116/118 branches and 28/28
functions, versus 394/405 lines, 112/118 branches and 25/28 functions in WASM.
Every production denominator is retained. C# and JS source remains unchanged
from the recorded 2,124-per-configuration and 209-case runs. See arguments.txt
and ARGUMENT-CONTRACTS-PLAN.md. All nine workflows pass on argument head 164f6f0, including Linux/macOS/Windows
native matrices; arguments-ci.txt records exact SHA, timestamps and links.
Whole-project 100% and every broader unfinished criterion remain open.

## Rejected owned mapped-file growth

A bounded Linux quota child first fails after rejected expansion: the base
retains its old reservation and pointer while Flush throws ObjectDisposedException
because the old view was already disposed. The safe reproduction checks flush
before reading that pointer. Existing growth remains a passing control. See
mapped-growth-before.txt; the earlier exception-type/sharing test-oracle mistakes
are identified separately and are not counted as product failures.

The replacement retains its current mapping and owned stream until both new
mapping and view exist, then publishes the pointer. Failed initialization closes
its newly acquired handle. Mapping creation extends the file through the platform
API, avoiding SetLength against a live Windows view. A smaller reservation uses
a smaller view; physical truncation remains at used-length disposal.

Two focused contracts cover failed initialization, repeated rejected growth,
unflushed bytes, flush, following growth/writes, exact file length/reopening,
exclusive-handle denial, shrink/regrowth, invalid reservations and disposal.
The quota child has an 8 KiB file limit, 128 MiB GC heap and finite reaping deadline;
its largest requested file is 12 KiB. All 2,126 tests pass in both configurations.

| Complete measured scope | Covered lines | Covered branches |
| --- | --- | --- |
| C# Debug | 7,943/8,260 (96.16%) | 4,264/4,638 (91.94%) |
| C# Release | 6,099/6,298 (96.84%) | 4,219/4,611 (91.50%) |

DurableMappedMemory.cs measures 45/46 Debug and 39/40 Release lines. All
production files remain measured. Formatting, release/source-size, ShellCheck
and 221 repository checks pass. Other language source remains unchanged from
930 native / 546 actual WASM / 209 JS runs. See MAPPED-GROWTH-PLAN.md and
mapped-growth.txt. Fresh exact-head CI, complete coverage, legacy physical
repair/power-loss verification and every broader unfinished criterion remain open.


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

## Engine publication of retained checkpoints

Missing-feature controls fail in the actual Rust, C#, JS and WASM targets before
the publication APIs exist. The engine now holds its commit mutex through
checkpoint construction and replacement. Unsupported capability preserves
usability; every uncertain replacement error fences the engine and pending
sessions until fresh reopening. Physical delta logs validate and prepare their
complete input before inner I/O and adopt the new codec on either boolean result.

Complete local suites pass 1,002 native tests plus six doctests, 620 actual-WASM
tests, 2,210 C# tests in each configuration, 230 JS tests and 221 repository
checks. Seventeen native child entry points run through bounded parents. The
actual-WASM runner now registers eight pre-existing wrapper unit controls and
two direct replacement-trait controls. JS enforces 100% production lines,
branches and functions; strict TypeScript passes.

| Complete measured scope | Covered lines | Covered branches | Covered functions |
| --- | --- | --- | --- |
| Native Rust | 13,399/14,199 (94.37%) | 2,540/2,950 (86.10%) | 1,703/1,837 (92.71%) |
| Actual WASM, complete linked source | 11,068/13,424 (82.45%) | 2,165/2,603 (83.17%) | 1,403/1,778 (78.91%) |
| C# Debug | 8,225/8,519 (96.55%) | 4,500/4,868 (92.44%) | Not reported |
| C# Release | 6,310/6,496 (97.14%) | 4,447/4,833 (92.01%) | Not reported |

The shared Rust checkpoint and delta components measure 100% lines, branches
and functions in native and WASM runs. WASM wrappers measure 539/539 lines,
22/22 branches and 94/94 functions; finer LLVM regions are 908/909 and
instantiations 134/177. A final C# follow-up adds rejection/reuse of an ordinary
non-replaceable inner log. Its 32 cases per configuration bring both changed
components to 100% lines/branches. The complete-suite totals above remain the
raw earlier reports; they are not silently merged with the focused follow-up.

All 72 bank/state/encoding/history/delta combinations preserve history, private
reservations, pins and following groups in native Rust/C# and in actual WASM on
Node, Bun and Deno. Bounded native gates verify commit serialization; stale
read-set, pessimistic ownership, exclusive admission and saved branch descriptors
remain effective after replacement. Both existing native handoff controls pass
48 configurations and 288 writers each, including closed mirrored-copy repair.
Real etcd passes default, checkpoint and checkpoint-plus-cache modes. All eight
coverage modules use verified 256 MiB linear-memory caps; compilers run sequentially.

See ENGINE-COMPACTION-PLAN.md, engine-compaction-before.txt and engine-compaction.txt.
The temporary root-level CI log directory caused one local repository-layout
failure; moving preserved logs under experiments resolves it without a source
change. Native owning factories and their process-recovery composition remain
unfinished. Fresh exact-head CI, complete-project coverage and all wider unmet
requirements remain active. Volatile images/process exits do not certify
hardware power-loss durability.

## Native checkpoint ownership and process recovery

The following native factories acquire the raw root and all selected bank/mirror
owners before validation, reset or repair. Constructor failures retain the
original error and every reverse-order cleanup failure; explicit close releases
the root last, does not flush pending records, and never retries uncertain raw
handles. A remaining bank fence blocks recovery even after root release. Tests
also reject ordinary/checkpoint in-place conversion in both directions without
altering the original acknowledged journal.

Three missing-factory controls first fail in the actual language targets at
133c4ff. Independent existing JS acquisition/cleanup and directory-sync/close
regressions also first fail; final tests preserve the actual error identities,
all raw cleanup attempts, acknowledged records and following reopening. See
native-checkpoint-before.txt and the guarded reproducer/fixtures under
experiments/issue-138.

Final validation uses Rust 1.99.0, .NET SDK 10.0.403, Node 26.12.1, Bun 1.4.4 and
Deno 2.9.7. `validate-native-checkpoint-all.sh` preserves seven independent stage
reports. Cargo runs one compiler at a time with reduced debug information; .NET
uses a 640 MiB heap bound and actual-WASM modules have verified 256 MiB caps.
An early live edit of the shell driver lost its final C# success message; both
complete zero-failure reports and coverage attachments were verified separately.
The remaining stages completed without editing the driver. The saved original
driver error and initial strict-Clippy range-loop error remain available.

| Complete measured scope | Passing tests | Lines | Branches | Functions |
| --- | ---: | ---: | ---: | ---: |
| Native Rust, all features | 1,013 plus six doctests | 13,642 / 14,449 | 2,547 / 2,958 | 1,772 / 1,907 |
| Actual WASM, linked source | 620 | 11,068 / 13,424 | 2,165 / 2,603 | 1,403 / 1,778 |
| C# Debug | 2,227 | 8,267 / 8,561 | 4,513 / 4,880 | not reported |
| C# Release | 2,227 | 6,344 / 6,530 | 4,460 / 4,845 | not reported |
| JS production modules | 239 | 100% | 100% | 100% |

Eighteen native child entry points are ignored by direct test discovery and
exercised through bounded parents. The new C# factory measures 100% lines and
branches in both configurations. Rust file_checkpoint.rs measures 214/221 lines,
5/6 branches and 57/58 functions; its private durability method and some error
regions remain unexecuted. The actual-WASM engine wrapper measures 263/263 lines,
6/6 branches and 50/50 functions; LLVM regions/instantiations remain below 100%.
Whole-project 100% coverage is unfinished; no denominator is reduced.

Native process parents terminate children immediately before and after actual
writes, truncations and synchronization during replacement and mirrored repair.
JS observes 34 single-copy replacement cuts, 62 mirrored replacement cuts and
12 mirrored-repair cuts. Rust and C# independently enumerate their own actual
operation traces. Each case observes child termination before removing abandoned
fences, accepts a complete old/new publication (repair retains the acknowledged
one), then appends, flushes, compacts and verifies another fresh reopening.

Ordinary journals, imported initial checkpoints and selected native compaction
each pass 48 configurations and 288 persistent Rust/Node/Bun/C#/Deno/Rust writer
handoffs. Compaction advances root epochs one through six, retains two revisions,
preserves exact Unicode/BOM/NUL names, allocator gaps, private reservations, pins
and branch bases, and repairs deliberate closed mirrored-bank damage. Node/Bun/
Deno also agree on all 72 actual-WASM compositions and finite payload reports.
All three real-etcd modes pass leader/majority-loss and native handoff checks.

All 221 repository checks, strict native/WASM lint, formatting, TypeScript,
optional-SQL guards, vector regeneration, release/source limits, ShellCheck,
actionlint and whitespace checks pass. See NATIVE-CHECKPOINT-PLAN.md,
native-checkpoint.txt and docs/ENGINE-NATIVE-CHECKPOINTS.md. The preceding engine
head 133c4ff passes all nine workflows and native platforms; engine-compaction-ci.txt
preserves that evidence. This native batch still needs its own exact-head CI.
Root-media redundancy/repair, legacy mapped repair/efficient page undo, physical
LSM, persistent object reclamation, wider algorithms and every other unmet
criterion remain active. Truthful synchronization is assumed; finite process
exits do not certify physical power-loss behavior.

## Native checkpoint Windows test audit

Published head `600cf5f` passes eight workflows. C# Linux/macOS pass; Windows
fails an exclusive-file byte comparison and the existing physical-checkpoint
worker probe. See native-checkpoint-ci-before.txt for exact run timestamps,
URLs and downloaded log locations. Close the recovered owner before comparing
bank bytes, then reopen for following writes. Preserve the physical worker
probe and unchanged deadlines; add trace/sequence diagnostics, callback-only
positive scheduling and bounded occupied-worker negative controls.

An isolated base archive plus only these three test-file changes passes all
2,229 tests in both Debug and Release, formatting and 221 repository checks.
A stale archived Release assembly was detected by its 2,228-case count; its
report is retained separately, and a forced rebuild verifies all final cases.
See native-checkpoint-ci-fix.txt. This local pass does not establish the cause
of the historical checkpoint-progress failure. The test correction is published
as `6a0f7c9`; all nine exact-head workflows pass, including Windows with the
unchanged physical probe and both scheduling controls. Rust and C# both pass
Linux/macOS/Windows. native-checkpoint-ci-fix.txt records exact timestamps,
permanent run URLs and saved job/log evidence. Every broader unmet criterion
remains active; the following root-mirroring source is excluded from this
test-patch verification.


## Independent mirrored checkpoint roots

The optional root adapter selects two copies independently of mirrored page
banks, with flags 2/3 preserving the existing flags 0/1 formats. The checkpoint
log validates both advertised bank modes before I/O, validates both root copies
and the entire selected bank before root repair, rechecks every staged root
source before writing a destination, and synchronizes both published decisions
before acknowledgement. Unknown canonical versions/modes and contradictory
identities/epochs reject. Uncertain mutations fence the adapter. Native owners
acquire every selected root/bank fence before recovery and close in reverse order.
The old C# constructor signature and all single-root defaults remain available.

The portable missing-feature controls first fail against 600cf5f; native factory
selection first fails against 6a0f7c9. Guarded isolated-baseline reproducers and
minimal fixtures are committed under experiments/issue-138. Eleven new portable
controls per language cover independent full-u64 vectors, all 512 selected-root
single-byte corruptions across both bank modes, epochs and bank reuse, invalid
banks, contradictory/unknown copies, changed staged sources, and finite before/
after publication/recovery faults. Native controls cover all four owning modes,
foreign fences before any repair, cleanup failures, compatible reopening and
both directions of invalid mode conversion. See mirrored-root-before.txt.

The sequential seven-stage driver passes from 2026-10-26T19:38:18Z through
20:22:23Z; complete logs and coverage reports retain their original denominators.
Cargo uses one compiler with reduced debug information, .NET a 640 MiB heap,
and all eight actual-WASM modules have verified 256 MiB linear-memory caps.
No OOM event occurs. C# baseline trace collection is independently retained and
included in future driver runs; the small driver addition passes ShellCheck.

| Complete measured scope | Passing tests | Lines | Branches | Functions |
| --- | ---: | ---: | ---: | ---: |
| Native Rust, all features | 1,028 plus six doctests | 13,860 / 14,669 | 2,580 / 2,994 | 1,803 / 1,938 |
| Actual WASM, linked source | 631 | 11,241 / 13,598 | 2,196 / 2,637 | 1,430 / 1,805 |
| C# Debug | 2,264 | 8,327 / 8,621 | 4,583 / 4,950 | not reported |
| C# Release | 2,264 | 6,399 / 6,585 | 4,527 / 4,913 | not reported |
| JS production modules | 261 | 100% | 100% | 100% |

Eighteen ignored native child entry points execute through bounded parents;
one legacy doctest example remains ignored. Rust checkpoint_root.rs measures
164/164 lines, 29/30 branches and 27/27 functions in both native/actual WASM.
file_checkpoint.rs measures 259/267 lines, 7/8 branches and 61/62 functions.
The C# native factory measures 100% lines/branches; the root adapter measures
100% lines and 98.33% branches. Native private forwarding and canonical-error
regions remain uncovered. The WASM engine wrapper measures 263/263 lines,
6/6 branches, 50/50 functions and 415/415 LLVM regions; its instantiations and
complete linked-source metrics remain incomplete. Whole-project 100% is open.

Each native language independently observes and executes 258 actual before/
after process-exit cuts across nine replacement/repair scenarios: 34, 62, 12,
40, 68, 12, 6, 6 and 18. A child must terminate before abandoned fences are
removed. Recovery requires a complete old/new publication (the acknowledged
one during repair), then following append/flush/compaction and fresh reopening.
These are bounded process tests, not physical power-loss certification.

Selected native compaction passes 96 independent root/bank/state/constraint/
encoding/history/delta configurations and 576 Rust/Node/Bun/C#/Deno/Rust writers.
Root epochs advance one through six; exact names, allocator gaps, private
reservations, immutable pins and branch bases survive. Each mirrored mode
repairs deliberate closed-copy damage, including simultaneous root and bank
copy damage. Ordinary journals and imported checkpoints independently retain
48 configurations/288 handoffs each. Node/Bun/Deno agree on 36 payload/delta
cases and 72 compaction compositions. Portable controls, actual-WASM conformance
and all 48 ordinary native policies pass; four declared capability skips remain
visible in conformance. All three real three-member etcd modes pass.

All 221 repository checks, strict native/WASM Clippy, formatting, TypeScript,
independent vector regeneration, SQL optionality, changesets, source limits,
ShellCheck, actionlint and whitespace checks pass. Shared checkpoint fixture
changes now trigger native language workflows. Fresh GitHub comments/reviews
show no new request; latest main is already included. GitHub's diff endpoint
still rejects more than 300 files with HTTP406; preserved diagnostics and local
base/batch diffs provide review evidence. See MIRRORED-ROOT-PLAN.md, the native
recipe, root protocol and mirrored-root.txt. Fresh exact-head CI follows the
commit. All broader unmet criteria and the draft status remain active.


All nine workflows pass on mirrored-root head `df6e01c`, including Linux/macOS/
Windows native tests and actual WASM. `mirrored-root-ci.txt` preserves exact run
IDs and timestamps. This does not complete the wider acceptance criteria; later
maintenance work needs independent exact-head validation.


## Optional namespace maintenance

The existing atomic interface remains unchanged; the selected provider capability
adds complete bounded namespace snapshots and exact generation-conditional
deletions. Rust, C# and JavaScript etcd adapters pin all pages to one linearizable
revision, reject malformed/incomplete replies, discard all pages on read failover,
and send every deletion once. Generation reuse, including after restore, violates
the provider contract. No object-liveness rule, collector or automatic task is
introduced. Ordinary JS read/CAS loads no selected helper module; C# creates its
maintenance codec only on first use.

The actual missing-API tests first fail against df6e01c. Seventy-nine independent
scan vectors and fifteen deletion vectors are consumed by every implementation.
They cover canonical Unicode/base64/revisions, ordering, limits, omitted ProtoJSON
zero fields, historical pagination, malformed responses and exact deletion
outcomes. The finite real three-member etcd probe passes Rust/C#/Node/Bun/Deno
examples, concurrent insert/update/delete between pinned pages, compaction with
whole-snapshot restart, lost deletion replies, leader loss and majority loss.
Its election precondition requires a live elected leader and linearizable read;
the original readiness failure remains preserved, with no timeout extension.

The complete sequential driver passes from 2026-10-26T20:42:50Z to 21:13:57Z.
Cargo uses one compiler/reduced debug information, .NET a 640 MiB heap, and actual
WASM a verified 256 MiB linear-memory cap. No OOM event occurs.

| Complete measured scope | Passing tests | Lines | Branches | Functions |
| --- | ---: | ---: | ---: | ---: |
| Native Rust, all features | 1,032 plus six doctests | 14,062 / 14,876 | 2,639 / 3,054 | 1,829 / 1,966 |
| Actual WASM, linked source | 631 | 11,241 / 13,606 | 2,196 / 2,637 | 1,430 / 1,806 |
| C# Debug | 2,268 | 8,428 / 8,722 | 4,684 / 5,054 | not reported |
| C# Release | 2,268 | 6,481 / 6,667 | 4,629 / 5,017 | not reported |
| JS production modules | 359 | 100% | 100% | 100% |

Eighteen native child entry points execute through bounded parents; one legacy
doctest remains ignored. The native maintenance adapter measures 194/199 lines,
59/60 branches and 25/27 functions; the C# partial adapter measures 100% lines
and 98.07% branches. The actual-WASM wrapper remains at 263/263 lines, 6/6
branches and 50/50 functions; complete linked coverage remains below target.
No denominator is reduced or inferred from test counts.

Bun/Deno each pass 98 maintenance cases. TypeScript, strict native/WASM Clippy,
formatting, independent vector regeneration, SQLite optionality guards,
changesets, source limits, ShellCheck, actionlint and 221 repository checks pass.
All comments/review endpoints were refreshed with no new feedback; latest main
is an ancestor. GitHub's >300-file diff limit remains documented; local base and
batch diffs support review. See MAINTENANCE-PLAN.md, maintenance-before.txt,
maintenance.txt and docs/ENGINE-MAINTENANCE.md. Fresh exact-head CI follows
publication. Actual epoch-fenced reclamation, whole-project 100% coverage,
physical power-loss verification and the wider listed criteria remain unfinished.

All nine exact-head workflows pass on maintenance head `44e0fa8`, including
Linux/macOS/Windows Rust and C# test matrices. See maintenance-ci.txt for run and
job URLs with timestamps. This validates the maintenance capability; later
reclamation changes require independent verification.


## Selected epoch-fenced object reclamation

`ReclaimingPartitionedJournalStore` now explicitly composes the optional
maintenance capability in Rust, C# and JS/TS. Every publisher compares the
observed reclamation epoch alongside its shard guards. Each collector advances
one fence and validates a complete catalog and every bounded data namespace
before deleting only unprotected older-epoch objects by exact generation.
Aliased live keys remain protected across partition names. Owned readers discard
an entire retired group and refresh/retry once only after both its missing
pointer and catalog generation change. The independent cache still defaults to
zero. Existing append-only providers retain their API and behavior.

Actual missing-API controls first fail against `44e0fa8`. Sixty-five independent
address/epoch/pointer vectors execute in every implementation. Bounded models
cover 70,859 unique two-writer/two-collector states and retain unsafe negative
controls without the epoch guard or epoch-specific keys. A separate finite
reader model exposes a fractured group when refreshing only a missing shard;
the whole-group model retains five coherent terminal views. These models do not
prove the production implementation or physical durability.

Test-first regressions additionally expose sparse JS input staging, BOM loss,
aliased-provider live-object deletion and reused catalog generations. All three
implementations now retain aliased roots and require both pointer/generation
changes before refresh. Twenty-four malformed-snapshot controls and eight later
request-failure controls expose JS errors that hid earlier successful mutations;
selected composite boundaries now retain unknown outcome and the original cause.
Caller validation still precedes I/O. See `reclamation-before.txt`.

The complete sequential driver passes from 2026-10-26T21:49:03Z through
22:17:42Z. Cargo uses one compiler with reduced debug information, .NET a
640 MiB heap, and actual-WASM modules a verified 256 MiB linear-memory cap.
No OOM event occurs. Complete reports retain their original denominators:

| Complete measured scope | Passing tests | Lines | Branches | Functions |
| --- | ---: | ---: | ---: | ---: |
| Native Rust, all features | 1,057 plus six doctests | 14,480 / 15,307 | 2,737 / 3,164 | 1,868 / 2,008 |
| Actual WASM, linked source | 656 | 11,667 / 14,037 | 2,294 / 2,747 | 1,470 / 1,848 |
| C# Debug | 2,307 | 8,627 / 8,921 | 4,851 / 5,228 | not reported |
| C# Release | 2,307 | 6,627 / 6,813 | 4,795 / 5,191 | not reported |
| JS production modules | 518 | 100% | 100% | 100% |

Eighteen native child entries execute through bounded parent tests; one legacy
doctest remains ignored. Rust's new store measures 198/207 lines, 37/46 branches
and 14/14 functions; its codec measures 163/166, 47/50 and 22/24; its collector
measures 57/58, 14/14 and 3/4 in both native and actual WASM. C# covers all new
production lines; codec and store branches retain uncovered cases. Every new JS
module measures 100% lines/branches/functions, as does the complete JS production
scope. The actual-WASM engine wrapper remains at 263/263 lines, 6/6 branches and
50/50 functions; linked coverage remains incomplete. Whole-project 100% is open.

A freshly built bounded WASM package passes Rust → C# → Node → Bun → Deno
handoffs, followed by Rust → C# → Node after leader loss. Each checks atomic
two-shard checkpoints, every retained revision/name, deleted allocator gaps,
owned retired workspaces and stale commit conflicts. Ten before/after response
loss cases each send one mutation and recover through fresh state; whole-group
refresh, warm-cache data outage and majority loss also pass. Every harness child
is reaped. Etcd supplies its declared persistence; network/process failures do
not certify physical device power loss.

Bun/Deno each pass all159 selected cases. TypeScript, strict native/WASM Clippy,
formatting, independent vector regeneration, deterministic-provider controls,
SQLite optionality, changesets, source limits, 221 repository checks, actionlint,
ShellCheck and whitespace checks pass. Latest main is included; refreshed issue/
PR comment/review endpoints show no new request. GitHub still rejects the
>300-file PR diff with HTTP406; preserved diagnostics and local base/batch diffs
support review. `RECLAMATION-PLAN.md`, `reclamation.txt` and the runnable
`ENGINE-RECLAMATION.md` recipe retain exact evidence and selected costs.

Namespace ownership and preservation of nonreused generation/epoch identities
remain application/provider contracts. Borrowed physical roots are unsupported;
complete listing bounds, full-journal writes/replay and catalog contention limit
scale. Fresh exact-head CI follows publication. Every broader unmet acceptance
criterion remains active and the PR remains draft and unmerged.

All nine workflows subsequently pass on exact reclamation source `419f7c7`,
with run creation 2026-10-26T22:19:35Z after that commit. Rust and C# each pass
Linux/macOS/Windows; actual-WASM and JS integration pass. `reclamation-ci.txt`
retains permanent run/job URLs and exact timestamps. Subsequent contract tests
are tracked separately and require fresh CI after publication.


## Reclamation public contract audit

Seven matching Rust/C#/JS controls exercise invalid partition construction,
pre-I/O route validation with read-only dependencies, all eight durability
capability combinations, lost initialization confirmation, lost staging
confirmation and interrupted catalog/object refresh. They pass on published
production source `419f7c7`; no production bug or correction is claimed.
Failed confirmation does not replay acknowledged mutations; an unpublished
staged object can be collected. Interrupted refresh returns no partial group,
then a fresh read observes a coherent group. Rust controls also run in actual
WASM. The native/C# controls and JS controls use the existing deterministic
providers and bounded inputs; asynchronous JS/C# controls have 10-second limits.

The complete sequential audit passes from 2026-10-26T22:28:46Z through
22:53:33Z. Reproduce with
`bash experiments/issue-138/validate-reclamation-contracts.sh`.

| Complete measured scope | Passing tests | Lines | Branches | Functions |
| --- | ---: | ---: | ---: | ---: |
| Native Rust, all features | 1,064 plus six doctests | 14,485 / 15,307 | 2,739 / 3,164 | 1,868 / 2,008 |
| Actual WASM, linked source | 663 | 11,672 / 14,037 | 2,296 / 2,747 | 1,470 / 1,848 |
| C# Debug | 2,314 | 8,627 / 8,921 | 4,853 / 5,228 | not reported |
| C# Release | 2,314 | 6,627 / 6,813 | 4,798 / 5,191 | not reported |
| JS production modules | 525 | 100% | 100% | 100% |

Eighteen native child entries run through their bounded parents; one legacy
doctest remains ignored. The Rust reclamation store now measures 203/207 lines,
39/46 branches and 14/14 functions, equally in native and actual WASM. The codec
and collector totals remain 163/166, 47/50, 22/24 and 57/58, 14/14, 3/4.
No denominator is reduced. Bun/Deno each pass all166 selected cases and all221
repository checks pass. Strict native/WASM Clippy, formatting, TypeScript, JS
coverage thresholds, SQL optionality, source limits, changesets, actionlint and
ShellCheck pass. Compilers run sequentially; actual-WASM modules retain verified
256 MiB caps. No OOM event occurs.

`reclamation-contracts.txt` preserves the complete audit totals. Production
integration is unchanged and remains separately verified by `reclamation.txt`
and all nine workflows on `419f7c7` in `reclamation-ci.txt`. Fresh exact-head
CI must verify this test batch after publication. Whole-project 100% coverage
and every wider unfinished criterion remain active; PR139 stays unmerged.


## Physical immutable sorted tables and checkpoint case scheduling

Matching Rust/C#/JS tables seal one immutable native doublets-page flush. Open
validates every bounded physical/logical record before exposing an index of
keys, addresses and page hashes. Point lookup reads verified payload pages;
absent keys need no page I/O. Returned values and complete scans are owned.
Canonical positive u64 generations preserve tombstones and live empty journals.
Construction validates every input before I/O and refuses occupied devices.
Mirrored repair is completed only after the complete table validates.

Missing table APIs first fail in all three languages on baseline `9a9a95c`;
`sorted-table-before.txt` retains the compiler/runtime errors and isolated
reproduction driver. Twenty-five valid/corrupt native binary vectors are
independently generated without importing a production codec. Their generation
followed the initial implementation. Finite before/after creation failures,
128 truncated seals, decision conflicts/modes, read failures and post-open
damage controls pass. An 80-key table spanning more than 40 pages reads its
middle 1,000-byte payload with at most three physical page requests.

The complete sequential driver starts 2026-10-26T23:20:09Z and all checks finish
at 23:50:25Z. The first repository layout run correctly rejects a root `ci-logs`
directory created during CI investigation; moving the preserved log under
`experiments/issue-138/ci-logs` resolves it. The failed output remains preserved,
and every repository check plus the remaining lint/format gates is rerun.
No language-suite or interchange failure occurred. Reproduce with
`bash experiments/issue-138/validate-sorted-table.sh`.

| Complete measured scope | Passing tests | Lines | Branches | Functions |
| --- | ---: | ---: | ---: | ---: |
| Native Rust, all features | 1,073 plus six doctests | 14,754 / 15,587 | 2,808 / 3,252 | 1,898 / 2,041 |
| Actual WASM, linked source | 672 | 11,938 / 14,317 | 2,365 / 2,835 | 1,499 / 1,881 |
| C# Debug | 2,349 | 8,728 / 9,022 | 4,993 / 5,382 | not reported |
| C# Release | 2,349 | 6,705 / 6,891 | 4,937 / 5,345 | not reported |
| JS production modules | 533 | 100% | 100% | 100% |

Eighteen native child entries run through bounded parent tests; one legacy
doctest stays ignored. The new Rust table measures 269/280 lines, 69/88 branches
and 30/33 functions in both native and actual WASM. No complete denominator is
reduced or excluded. Whole-project native/C#/linked-WASM coverage stays open.
Bun/Deno each pass all174 selected cases. Strict TypeScript, native/WASM Clippy,
formatting, both independent vector regenerations, lazy import positive/negative
controls, SQL optionality, source limits, changesets, all221 repository checks,
ShellCheck, actionlint and whitespace checks pass. Compilers run sequentially
with jobs1/reduced debug, .NET640MiB and verified actual-WASM256MiB caps. Cgroup
OOM and OOM-kill counters remain zero.

The native Rust/C#/Node/Bun/Deno examples pass all25 writer/reader pairs. Each
reader verifies the complete 80-key scan and every indexed point, max-u64
tombstone, live empty journal and Unicode/BOM/NUL/newline values. It preserves
all three physical file hashes and releases its owner. Native examples and
explicit SQL-denial controls select the physical table without SQLite.

Separately, all nine workflows complete on published contract head `9a9a95c`,
but C# Windows fails its mirrored-bank/root replacement xUnit case at 120s.
The other eight workflows and C# Linux/macOS pass. Full log
`experiments/issue-138/ci-logs/csharp-40416245668.log` lines26794-26810 contains
the exact deadline; it does not establish a child/storage assertion failure or
the source of slowness. Four disjoint ordinal slices per configuration retain
all258 original real process cutpoints, 120s case and 10s child deadlines,
cleanup, ownership and recovery assertions. All36 focused cases pass and their
counts independently cover each original boundary exactly once. The combined
full C# suites above include this test-only correction and eight table tests.
Fresh exact-head Windows/macOS/Linux CI remains required after publication.

`sorted-table.txt`, `sorted-table-before.txt`, `checkpoint-slices.txt` and the
two implementation plans preserve the evidence. Physical tables alone do not
implement LSM WAL/memtable/manifest recovery, compaction or retirement. The
separate PHYSICAL-LSM-PLAN.md keeps that required composition active. Physical
power-loss certification, complete coverage and every wider gap remain open;
PR139 remains draft and unmerged.

All nine workflows now pass on exact physical-table/model head
`523888a302c49d8b148ab28d69acc9ad34bdf87d`, created at
2026-10-26T23:52:43Z. Both native Linux/macOS/Windows matrices pass, including
the retained checkpoint process-exit boundaries. `sorted-table-ci.txt` records
every workflow and native platform job URL. The earlier unsliced Windows
timeout remains preserved; its underlying source is not established by the
passing sliced cases. Subsequent semantic-WAL and store work needs fresh CI.

## Selected semantic LSM WAL component

Rust/C#/JS now explicitly select bounded `native-lsm1` checkpoint, manifest and
group replay. Canonical u64 strings, sorted unique keys/IDs, positive tombstone
generations, live empty journals, reservation floors, consecutive group numbers
and owned payloads match across ports. Encoding revalidates public state.
This is a pure codec: every selected physical table, combined retained capacity,
publication, namespace ownership and retirement still require store validation.

Missing APIs fail in all three languages before implementation on baseline
`523888a`; `lsm-wal-before.txt` preserves exact compiler/runtime errors and an
isolated-baseline reproduction driver. The independent oracle initially has
97 cases before production; 14 later controls bring it to 111 cases, 11 valid.
It regenerates identically without production imports. Strict optionality and
fresh-loader positive/negative guards verify SQL-free selection and lazy roots.

The complete sequential driver starts 2026-10-27T00:05:40Z and finishes all
checks at 00:43:23Z. Its first C# Debug run rejects the two new synchronous Facts
with Timeout, while all 2,349 existing tests pass. Both new declarations become
async Task-returning tests; the focused two tests and both complete C# suites
then pass. Original setup output remains preserved, and passed native checks
are retained because Rust source is unchanged. Reproduce the complete pass with
`bash experiments/issue-138/validate-lsm-wal.sh`.

| Complete measured scope | Passing tests | Lines | Branches | Functions |
| --- | ---: | ---: | ---: | ---: |
| Native Rust, all features | 1,075 plus six doctests | 15,025 / 15,863 | 2,887 / 3,332 | 1,926 / 2,074 |
| Actual WASM, linked source | 674 | 12,209 / 14,593 | 2,444 / 2,915 | 1,527 / 1,914 |
| C# Debug | 2,351 | 8,843 / 9,137 | 5,141 / 5,536 | not reported |
| C# Release | 2,351 | 6,796 / 6,982 | 5,088 / 5,499 | not reported |
| JS production modules | 535 | 100% | 100% | 100% |

The new Rust codec measures 271/276 lines, 79/80 branches and 28/33 functions
in native and actual WASM. All denominators retain uncovered production source;
whole-project native/C#/linked-WASM coverage is below the required 100%.
Eighteen native child entries run through bounded parents; one legacy doctest
remains ignored. Bun/Deno each pass 176 selected tests; every one of the 25
native table writer/reader pairs preserves physical hashes and releases owners.
All 221 repository tests, formatting, strict native/WASM Clippy, TypeScript,
vector regeneration, source limits, changesets, ShellCheck, actionlint and SQL
optionality checks pass. Compilers run sequentially with jobs1/reduced debug,
.NET640MiB and verified actual-WASM256MiB caps; OOM/kill counters stay zero.

`LSM-WAL-PLAN.md`, `lsm-wal.txt` and `lsm-wal-before.txt` preserve evidence.
Physical LSM store/provider/process composition and all broader unmet R1–R12
criteria remain active. Publication requires fresh exact-head CI; PR139 stays
draft and unmerged.

## Physical store composition experiment and missing-API baseline

Codec-only baseline `d12760f` fails the actual JS, Rust and C# store fixtures
with ERR_MODULE_NOT_FOUND, E0432 and CS0246 respectively. The sequential controls
finish at 00:45:26Z, 00:45:34Z and 00:45:53Z on 2026-10-27; lsm-store-before.txt
records exact log locations and the isolated-baseline reproduction driver.

An isolated experiment uses the production checkpoint WAL, sorted tables and
semantic codec over finite durable images. Thirteen controls pass in each of
Node/Bun/Deno, including 72 compaction and 80 group-publication before/after
mutation cuts per runtime. Every cut fences the uncertain old owner, recovers
whole acknowledged groups and permits following use. The live disk index retains
metadata rather than a journal payload map. Historical conflicts beneath a newer
version and a provider/device durability mismatch first fail separate controls,
then pass with corrected experiment logic. Six read-only selected-prefix controls
also pass in each runtime, preserving ignored suffixes until semantic validation.

The exact Node CI commands pass locally and are included in the JS workflow;
ShellCheck, actionlint and repository checks validate that workflow change.
`LSM-PROTOTYPE-PLAN.md` and lsm-store-prototype.txt delimit this evidence. These
results supply neither production store APIs nor native directory guarantees,
actual-WASM store results, process exits or cross-language store interchange.
Those requirements and every wider unmet acceptance criterion remain active.

## Movement process supervision after semantic codec CI

On exact head `d12760f`, eight workflows pass and Rust Windows fails three
movement parents during setup. The requested movement cut has not been reached;
setup logs show finite phases advancing through 7.9 seconds before the aggregate
ten-second deadline kills the child. No cause is established for the underlying
Windows barrier latency. movement-progress-ci-before.txt records the full saved
log path, failure line numbers, job URL and timestamps.

The revised supervisor retains ten seconds between finite phase advances and
rejects more than 64 phases or 192 bytes, invalid order, regressions and stalled
or repeated progress. Every supervisor error kills and reaps its child. The
three added controls and all 40 original movement/recovery scenarios pass in the
focused six-test target; its two child entries run only through bounded parents.
Formatting, strict Clippy and all 221 repository checks pass. The isolated clean
production archive excludes pending physical-store implementation. Fresh exact-head `f523b5c` Rust CI now passes Windows, Ubuntu, macOS,
coverage and the simulated fresh merge. Windows logs 44662–44693 show all six
controls pass in 48.53 seconds, including the original crash parents.
movement-progress-ci-after.txt preserves run/job URLs and the complete saved
log path. Eight workflows pass; Dependencies alone reports rustix 1.1.5 behind
1.1.6 in three locks (saved dependency log lines 209–212). The local latest
version audit passes after those three version/checksum updates; subsequent
publication requires fresh CI. All wider unfinished requirements remain active.


## Portable physical LSM store

Complete sequential validation finishes 2026-10-27T01:52:07Z. The new selected
Rust, C# and JavaScript stores compose the real checkpoint WAL, immutable
doublets tables, bounded payload buffer and metadata-only disk index. Each
language verifies 72 compaction and 80 group-publication before/after effects,
uncertain fencing and following edits. Missing-API fixtures precede production
implementation. Two JavaScript regressions first demonstrate early repair
completion and lost explicit close retry; every port now defers completion until
all bindings/history/admission validate and preserves retriable cleanup.

Native Rust verifies 1,096 distinct tests: the complete 1,091-test run followed
by five added mirror/admission controls in the expanded 18-test target. Its
coverage run merges that target without cleaning, with production code unchanged.
Six doctests pass; one legacy documentation entry remains ignored. Actual WASM
executes all 692 controls, including the same 18 store controls. Debug and Release
C# each pass 2,369 tests. Node passes all 568 tests with 100% production lines,
branches and functions; strict TypeScript passes. Bun/Deno each pass all 209
selected tests. Native Rust/C# cluster/name consumers use actual engines; the JS
cluster binding is mocked. Partition/reclamation consumers use physical stores
in every port.

Complete coverage denominators remain intact: native Rust 15,655/16,527 lines,
3,004/3,484 branches and 2,011/2,167 functions; actual WASM 12,839/15,257 lines,
2,561/3,067 branches and 1,612/2,007 functions; C# Debug 9,038/9,341 lines and
5,343/5,778 branches; Release 6,967/7,161 lines and 5,294/5,741 branches. These
results leave whole-project 100% coverage unfinished.

All 25 existing native immutable-table writer/reader pairs preserve the three
file hashes and release ownership. This is table interchange, not whole-store
interchange. Independent page/table fixtures and all 111 semantic-WAL vectors
regenerate identically. Formatting, strict native/WASM Clippy, SQL optionality,
lazy imports, source-size/release checks, 221 repository checks, actionlint and
ShellCheck pass. The first C# formatting gate rejects whitespace; formatting
correction precedes successful focused and full runs. Sequential compilers use
jobs1/reduced debug, .NET640MiB and verified linked-WASM256MiB maxima.

Reproduce with experiments/issue-138/validate-lsm-store.sh. lsm-store.txt
contains the complete measured report and original failure references.
PORTABLE-LSM-PLAN.md and native-lsm-store.md describe ownership, finite capacities,
startup ordering and costs. An already-constructed WAL can have performed
recovery effects; the portable API cannot retroactively prevent them. Exclusive
native namespace discovery, full semantic validation before WAL recovery effects,
physical store process/interchange and every wider unmet R1–R12 criterion remain
required. PR139 remains draft and unmerged. All nine exact-head bc364d2 workflows pass;
lsm-store-ci.txt preserves native platform jobs, timestamps and URLs.


## Borrowed checkpoint prefix inspection

The Rust/C#/JS inspection APIs validate the selected physical prefix without
identity allocation, initialization, truncation, synchronization, repair
completion or device closure. They retain caller ownership and return owned
records. Mirrored reads may stage volatile repairs; semantic failure requires
discarding those wrappers. Ordinary constructors retain their recovery behavior.
Missing APIs fail before implementation, with exact diagnostics preserved in
checkpoint-inspect-before.txt. Shared controls cover finite admission, empty
and partial initialization, mode/identity/decision conflicts, selected corruption,
ignored suffixes, every observed selected-prefix read failure before/after its
effect, owned results and deferred actual root/bank mirror repairs.

Complete sequential validation finishes 2026-10-27T02:37:37Z. Native Rust passes
1,102 tests and six doctests; 19 child entries require their bounded parents and
one legacy documentation entry remains ignored. Actual WASM passes 698 tests.
C# Debug and Release each pass 2,375, Node passes 576 with enforced 100% production
lines/branches/functions, and Bun/Deno each pass all 217 selected controls. All
25 existing native immutable-table writer/reader pairs preserve file hashes and
release owners. These are table handoffs rather than whole-store interchange.

Complete denominators: native Rust 15,716/16,589 lines, 3,036/3,518 branches and
2,014/2,170 functions; actual WASM 12,900/15,319 lines, 2,593/3,101 branches and
1,615/2,010 functions; C# Debug 9,066/9,369 lines and 5,391/5,826 branches; Release
6,990/7,184 lines and 5,342/5,789 branches. New Rust inspection measures 61/62 lines,
32/34 branches and 3/3 functions on both native and actual WASM. No uncovered
production source is excluded. Complete 100% coverage remains unfinished.

Formatting, strict native/WASM Clippy, types, optional SQL dependency/import
guards, independent page/table/111-WAL-vector regeneration, lazy root imports,
source/release/workflow/shell gates and all 221 repository checks pass. Compilers
run sequentially with jobs1/reduced debug, .NET640MiB and verified WASM256MiB caps.
Reproduce with experiments/issue-138/validate-checkpoint-inspect.sh; the measured
report is checkpoint-inspect.txt. The native facade, exclusive directory discovery,
whole-store file/process interchange and every wider unmet R1–R12 requirement
remain active. Publication requires fresh exact-head CI. PR139 stays draft.


## Native JavaScript LSM directory facade, 2026-10-27

Actual missing native APIs first fail on immutable archived bc364d2 in JS/Rust/C#.
The JS implementation now acquires one outer owner, bounds directory enumeration,
validates existing physical and semantic state read-only, closes temporary leases
and only then allows writable WAL/table recovery under that same owner. Each
mirror mode is independent. Uncertain live-handle cleanup retains the fence;
marker removal is attempted only after every known handle closes.

All 617 Node tests pass with every production module included at enforced 100%
lines, branches and functions, followed by strict TypeScript. The 41 new controls
also pass in Bun and Deno: each runtime executes all 134 before/after cuts of one
finite 67-operation native compaction trace, ten abrupt child exits reaped before
external marker recovery, all eight mirror combinations, read-only evidence
preservation and uncertain cleanup controls. The mutation probes initially could
not interpose captured named fs exports in Bun/Deno; selecting the shared fs object
corrects the instrumentation, after which every control actually triggers there.
This instrumentation correction is not claimed as a storage bug fix.

A stale generated WASM package first lacks exportCheckpoint. Fresh current source
builds sequentially at measured maximum child RSS 388,720KiB with a verified
256MiB linear-memory cap. Its actual links engine passes direct, partitioned and
epoch-reclaiming native-file consumers in Node/Bun/Deno, including checkpoints,
Unicode names, stale group rejection, actual reclamation, compact/close/reopen.
The example is examples/native-lsm-js/cluster.mjs. No external coordinator or SQL
is involved. Optional-import and root-laziness guards pass. Full native/C#/WASM
coverage remains below 100% as recorded above; no new denominator is excluded.
Native Rust/C# facade parity, complete whole-store interchange and all wider
unmet R1–R12 requirements remain active. native-lsm-js{,-before}.txt preserve the
commands and logs. Publication requires fresh exact-head CI; PR139 stays draft.


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


## Native Rust LSM directory facade, 2026-10-27

Complete sequential validation finishes 05:18:14Z. All 1,138 native tests, six
doctests (one legacy example ignored), 698 actual WASM tests, strict native/WASM
Clippy, eight SQL-optional dependency graphs, three real native cluster consumers
and 50 complete-store handoffs pass. The native focused controls include 408
compaction syscall cuts, 730 opening cuts and 408 actual abrupt child exits; all
are finite and reaped before fixture cleanup. No physical power-loss claim follows.
A surviving-metadata integration fixture initially removes the committed slot;
correcting it to keep the nonempty decision establishes the intended surviving
copy case. This is a fixture correction, not a production repair claim.

Whole native LLVM coverage: 16,712/17,614 lines, 3,211/3,698 branches,
2,167/2,334 functions, 25,748/27,903 regions and 12,023/21,595 instantiations.
Whole linked WASM: 12,900/15,323 lines, 2,593/3,101 branches, 1,615/2,012 functions,
19,861/24,157 regions and 4,281/7,400 instantiations. All production denominators
remain included; whole-project 100% coverage stays unfinished. The first-stage
native test runner reports 21 ignored child entries, each driven by its parent.

See native-lsm-rust{,-before}.txt for commands and logs. Rust opts into native-pages
plus physical-lsm without SQL. Unix advertises synchronized namespace capability;
Windows supports file-content sync and explicitly declines durable namespace.
The complete native source still needs fresh publication CI. Existing head
3dfbc74 has eight successful workflows; C# alone exposes an aggregate child
supervision deadline while the child is progressing. The finite supervisor
correction is a separate step. Every broader unmet R1–R12 criterion stays active.

C# finite phase supervision finishes local validation at 05:35:33Z. Both full
configurations pass 2,421 tests, including nine new parser/process cases; all 50
focused controls pass. Exact SDK 10.0.403 formatting, three actual native cluster
consumers, 32 complete-store handoffs and optional-SQL controls pass. Production
coverage remains Debug 9,280/9,606 lines and 5,559/6,004 branches; Release
7,167/7,385 lines and 5,505/5,965 branches. The independently failing finite
progress child now survives four completed operations over the former aggregate
bound; repeated progress fails and is reaped. Only complete consecutive phases
1..64 renew the unchanged 15-second idle bound. See csharp-phase-progress{,-before}.txt.
Fresh Windows publication CI remains required; lower-level filesystem latency
and every broader unmet R1-R12 criterion stay open.

That C# publication requirement is now verified on exact head 8d2ecdf, committed
05:38:18Z and tested by run 40456464654 created 05:38:28Z. Windows completes all
2,421 tests at 05:48:48Z, Ubuntu at 05:40:50Z and macOS at 05:41:00Z. Both finite
deadline controls and both actual current-before Windows movement cases pass.
The complete downloaded log and exact line references are recorded in
csharp-phase-progress-ci-after.txt. This verifies the supervision change without
assigning a cause to filesystem latency; broader incomplete criteria remain open.

All nine exact-head workflows on 8d2ecdf now conclude success. Rust Windows
finishes 06:17:50Z within the existing 45-minute job budget, followed by the
package build at 06:18:29Z. native-lsm-rust-ci-after.txt records all nine runs
and timestamps. This verifies publication of the native Rust facade and finite
C# phase correction; it does not verify subsequent unpublished chunk changes
or complete the wider acceptance criteria.


## Bounded append-only journal chunks, 2026-10-27

Missing selected APIs and ignored JS selection fail first; the sparse-array
admission control independently fails before dense copying. Shared independent
vectors cover UTF-8 boundaries, BOM, supplementary characters, NUL and empty
journals. Ten before/after publication cuts per language preserve complete old
or new groups and following edits. Owned cache hits retain manifest count
validation; staging compares actual complete contents rather than trusting cache.
Unsupported epoch collectors reject these manifests before any deletion.

Full sequential validation passes 1,146 native tests plus six doctests, 706
actual-WASM tests and 2,429 C# tests in each configuration. The 21 native child
entries remain parent-driven; one legacy doctest is ignored. All 626 Node tests
include every production module at enforced 100% lines/branches/functions.
Eight selected tests pass in Bun/Deno. All 221 repository checks, 645 source-size
checks, strict native/WASM Clippy, whole-solution SDK10.0.403 format, TypeScript,
workflow/ShellCheck and optional-SQL controls pass.

Whole native LLVM: 16,853/17,756 lines, 3,262/3,752 branches, 2,179/2,346 functions,
25,968/28,125 regions and 12,052/21,632 instantiations. Whole linked-WASM:
13,041/15,465 lines, 2,644/3,155 branches, 1,627/2,024 functions, 20,081/24,379
regions and 4,310/7,425 instantiations. C# Debug: 9,357/9,683 lines and
5,653/6,102 branches; Release: 7,224/7,442 lines and 5,597/6,063 branches.
Every denominator remains included; complete native/C#/WASM 100% stays unfinished.

Four actual native/actual-WASM cluster compositions pass. All fifty whole and
fifty chunked directed Rust/C#/Node/Bun/Deno complete-store handoffs pass in
plain/mirrored modes with read-only physical hash preservation. The finite
actual-WASM payload example produces byte-identical Node/Bun/Deno results:
two-record chunks write 2,958 data payload bytes rather than 16,530 for whole
journals, while catalog payload rises from 1,038 to 3,924 and uncached requests
rise from 94 to 304. Separate caching reduces data-read bytes from 31,086 to
19,232. These measure record payload at the provider seam, exclude physical I/O,
and establish no durability, latency or throughput result. Full staging and
engine replay remain linear. journal-chunks{,-before}.txt preserves exact scope,
commands and logs; fresh publication CI remains required. Chunk reclamation and
every wider unmet requirement remain active in this draft, unmerged PR.


Published append-only chunks and the separate dependency freshness correction
now pass all nine workflows on exact head 58d35f8. Rust Windows completes at
2026-10-27T07:03:21Z; native Linux/macOS and C# Windows/Linux/macOS all pass.
See journal-chunks-ci-after.txt for exact SHA, run links and timestamps.
This establishes publication verification for append-only chunks; subsequent
chunks2 reclamation and all wider unmet requirements still need verification.


## Epoch-fenced chunk reclamation, 2026-10-27

Full sequential validation finishes 07:27:05Z. Missing selected APIs and JS blob2
publication first fail against unchanged 58d35f8; an isolated immutable-source
rerun reproduces all three controls. Current-epoch staging, complete live graph
protection and whole-group retry preserve defaults and owned reads. Every
four-chunk cache mask, second retirement, ten before/after mutation cuts,
overlapping collectors, exact-generation ABA, late scan failures, aliases,
corruption and following edits pass matching native/C#/actual-WASM controls.
Eighteen independently generated shared vectors cover UTF-8 and epoch bounds.
The two finite models retain unsafe negative controls and their memory/state
bounds; their passing states do not replace production verification.

Complete suites pass 1,159 native Rust tests plus six doctests, 719 actual-WASM
tests and 2,442 C# tests per configuration. All 640 Node tests include every
production module at enforced 100% lines/branches/functions. Fourteen new tests
pass in Bun/Deno; strict types, 221 repository and 655 source-size checks,
SDK10.0.403 formatting, strict native/WASM Clippy, workflows/ShellCheck, optional
SQL, one minor changeset and 126 dependency declarations/Cargo.lock checks pass.

Whole native LLVM: 16,956/17,858 lines, 3,277/3,766 branches, 2,188/2,355 functions,
26,130/28,286 regions and 12,084/21,687 instantiations. Whole linked WASM:
13,144/15,567 lines, 2,659/3,169 branches, 1,636/2,033 functions, 20,243/24,540
regions and 4,342/7,471 instantiations. C# Debug: 9,400/9,726 lines and
5,695/6,144 branches; Release: 7,253/7,471 lines and 5,639/6,105 branches.
All three wrapper sources reach 100% lines/functions/reported branches; complete
regions/instantiations remain included. Full native/C#/linked-WASM 100% remains
unfinished. Twenty-one native child entries are parent-driven; one legacy
doctest is ignored. No production denominator is excluded.

Five actual native/actual-WASM cluster compositions and all 150 directed plain/
mirrored Rust/C#/Node/Bun/Deno complete-store handoffs pass with read-only file
hash preservation. Two separate real three-member etcd clusters verify blob2
and chunks2. Each passes native/WASM retained-history/checkpoint handoffs, ten
lost-reply cuts without automatic mutation replay, retired group refresh,
warm-cache data outage, leader loss and majority loss without partial success.
Every child/server is shut down and reaped; CI now preserves maintenance and
reclamation member logs as well as the existing cluster logs.

The actual-engine finite cost example yields byte-identical Node/Bun/Deno JSON.
Without collection, two-record chunks write 2,958 data payload bytes versus
16,530 for whole journals; catalog payload grows from 1,586 to 6,376. Collecting
every fourth revision writes 5,860 data bytes and retains sixteen objects;
collecting every revision writes 16,530 bytes and removes this workload's write
savings. Record payload counters exclude keys, framing, physical I/O, latency
and throughput. Final replay/collection checks preserve owned journals, every
retained name and following edits. Full replay, bounded complete listings and
one catalog remain costs; automatic balancing/multi-catalog atomicity remain
unfinished. reclaiming-chunks{,-before}.txt records exact scope and commands.
Fresh exact-head publication CI remains required. All wider unmet R1-R12
criteria stay active and PR139 remains draft/unmerged.

Published c1abe42 passes eight workflows, including Rust Windows/Linux/macOS.
C# Windows alone fails DistinctFiniteProgressSurvivesAnAggregateDeadline with
phase5/exit0; Linux/macOS and all other C# gates pass. See
reclaiming-chunks-ci-before.txt and phase-context-before.txt for exact-head run
timestamps, complete downloaded log locations and the actual exception lines.
This head does not have all passing CI. The isolated captured-context control
and durable regression reproduce that false-timeout symptom before helper
changes. Fresh correction-head Windows verification remains required.

The helper correction leaves capture and process continuations independent of
the caller context. The final linked-helper probe passes with zero queued
callbacks; all thirteen focused controls and 2,447 C# tests in each Debug and
Release configuration pass. Bounds, strict phase rejection and reaping remain
intact. All 221 repository checks pass after relocating ignored CI logs under
the case-study tree; type checks, source limits, changeset validation, 126
dependency declarations and workflow lint also pass. See phase-context.txt.
This is local verification of the proven helper dependency; fresh Windows CI
must still establish the published-head result.

## Reclaiming admission/provider controls (2026-10-27)

Matching Rust/C#/JS public controls now exercise malformed expected/update
array lengths, generation-zero shard/epoch/object payloads, uppercase epoch
and hash digits in both live and scanned keys, and a missing object after the
catalog advances without changing its pointer. Both layouts reject before the
relevant I/O/publication/deletion; malformed scans retain otherwise eligible
orphans. Restoring the provider preserves owned reads and following writes.
These are controls for existing behavior, not a new production correction.

Full suites pass 1,163 native plus six doctests, 723 actual-WASM, 2,447 C# per
configuration and 644 Node tests with enforced 100% production lines/branches/
functions. The four controls also pass separately in Bun/Deno. The final orphan
fixture is reverified in all 50 native maintenance controls, the complete actual
WASM suite and complete C# Debug suite, finishing 08:23:27Z; Release and JS run
after that fixture change in the initial driver. All 221 repository tests,
658 source-size checks, format/strict Clippy/types, existing minor changeset,
126 dependency declarations and workflow/ShellCheck/whitespace checks pass.

Complete native LLVM remains 16,956/17,858 lines, 3,281/3,766 branches,
2,188/2,355 functions, 26,131/28,286 regions and 12,084/21,687 instantiations.
Complete linked WASM remains 13,144/15,567 lines, 2,663/3,169 branches,
1,636/2,033 functions, 20,244/24,540 regions and 4,342/7,471 instantiations.
Each gains four covered branches over c1abe42; generic functions/instantiations
remain included. C# Debug is 9,400/9,726 lines and 5,695/6,144 branches;
Release is 7,253/7,471 lines and 5,639/6,105 branches. Complete native/C#/linked
WASM 100% remains unfinished. No denominator is filtered. The native final
focused rerun retains complete accumulated profiles rather than replacing the
report with maintenance-only scope. reclamation-admission.txt records all
commands, final-fixture checks, official totals and the helper's failing baseline.

Five-runtime file handoffs and both real three-member etcd layouts remain
verified at c1abe42 under reclaiming-chunks.txt; this step changes no production
source. Fresh exact-head CI remains required. All wider unmet criteria remain
active; PR139 stays draft and unmerged.

The published helper/admission head 7e9799a now passes all nine workflows.
Every run is created 2026-10-27T08:24:50Z, after the 08:24:39Z commit, and reports
the same full SHA. C# run 40473315951 passes 2,447 tests on Windows/Linux/macOS;
the deferred-context regression and previously failing finite-progress control
both pass on Windows. Rust run 40473315935 also passes all native platforms.
phase-context-ci-after.txt preserves every run link and timestamp. This closes
the earlier helper's platform-verification gap, not a production durability gap.

## Owned Rust mapped-view barriers (2026-10-27)

Microsoft's FlushViewOfFile documentation requires view synchronization before
file-buffer synchronization. Upstream platform-mem 0.3.0 and the previous Rust
adapter synchronized only file handles. This establishes the missing Windows
barrier from primary sources; readback is not a hardware power-loss reproduction.
The native API controls first fail with E0599 against immutable 7e9799a. The
reproducer isolates source and target artifacts; an initial shared target cache
collision is recorded separately. A further independent current-allocation
assertion first rejects the draft's tail-only descriptor, then passes when
publication covers the complete current mapping. mapped-flush-before.txt
preserves the exact diagnostics and reproduction commands.

The owned barrier invalidates before unmap, publishes after successful remap and
closes before Drop. One shared lock protects the view through both ordered I/O
barriers. Independent mocks verify each failure, retry and absence of later
barriers; real file controls verify growth, shrink, prefix/tail contents and
reopening. Closed, failed-remap and poisoned ownership reject without stale
access. Drop releases the duplicated file even when logical handles survive.
Generic adopted stores explicitly attach their matching handle; map_store
preserves it through decorators. A replaced owner rejects before mtime/revision
publication until its replacement handle is attached. The runnable adoption
example verifies following edits and reopening.

Sequential validation finishes 09:13:25Z. All six API controls, six independent
unit controls, six default-feature controls, the example, 1,175 complete native
tests plus six doctests, 723 actual-WASM tests and 221 repository checks pass.
Formatting, strict native/WASM Clippy, source-size checks, the existing minor
changeset, 126 dependency declarations, workflows/ShellCheck and whitespace
checks pass. Twenty-one native child entries remain parent-driven, and one
legacy doctest remains ignored. No C#/JS production code changed; their prior
complete suites and exact 7e9799a CI remain preserved separately.

Complete native LLVM: 17,080/17,958 lines, 3,288/3,774 branches, 2,210/2,372
functions, 26,285/28,420 regions and 12,218/21,855 instantiations. Complete linked
WASM: 13,144/15,667 lines, 2,663/3,171 branches, 1,636/2,050 functions,
20,244/24,674 regions and 4,342/7,488 instantiations. The new barrier source
reaches 66/66 native lines, 4/4 branches and 14/14 functions, while its generic
instantiations remain 40/64. Native-only mapped code remains in the complete
linked WASM denominator with zero coverage. No production denominator is
excluded, and complete native/C#/linked-WASM 100% remains unfinished.
mapped-flush.txt retains all full and component metrics.

The selected mapping adds one duplicated file handle and shared mutex allocation
per owner, two lock acquisitions per resize and one per explicit barrier;
ordinary link edits gain no extra lock. Drop cannot report failures. The barrier
does not fix upstream failed-resize safety, torn-page repair or in-place atomicity,
and ordinary file readback does not certify physical power-loss recovery. Fresh
exact-head Windows/Linux/macOS CI remains required for this production change.
Every wider unmet criterion stays active; PR139 remains draft and unmerged.


## Retained plain-data mapped growth (2026-10-27)

A finite Linux child first reproduces EFBIG followed by SIGSEGV through the
safe retained allocation API. Its within-quota control passes. Five durable
regressions fail against immutable 89fbfad; a separate generic destructor
single-owner control passes. `run-mapped-grow-before.sh` reconstructs isolated
source and target directories, applies only the new tests and preserves both
failure forms. mapped-grow-before.txt records exact limits and log lines.

The correction prepares and initializes a replacement owner while the original
mapping remains live, then retires/publishes under the existing flush lock.
Known initialization length is captured before minimum-page padding and is not
advanced after failed physical extension. Windows extends through a temporary
mapping object rather than SetEndOfFile while the old view remains live. Quota,
capacity overflow, injected failure after physical extension, clone rejection
and panicking initialization retain the pointer, prefix and checked barrier.
Following edits, retries and reopening pass. Zero growth invokes its callback
without replacing the view. Destructor values retain their single-owner path.

`validate-mapped-grow.sh` runs compilers sequentially with one build job and no
debug information; actual-WASM memory is capped at 256 MiB. It finishes
2026-10-27T10:22:42Z. All 1,184 native tests, six doctests, 723 actual-WASM tests
and 221 repository tests pass. Twenty-two native child entries are executed
only by their bounded, reaping parents; one legacy doctest remains ignored.
Both default/all-feature API suites, nine independent unit controls, the
finite quota probe and owned-flush example pass. Formatting, strict native/
WASM Clippy, source sizes, the minor release changeset, all 126 dependency
declarations, workflow/ShellCheck and whitespace checks pass.

Complete native LLVM is 17,176/18,090 lines, 3,291/3,780 branches,
2,223/2,389 functions, 26,422/28,631 regions and 12,326/21,983 instantiations.
Complete linked WASM is 13,144/15,737 lines, 2,663/3,171 branches,
1,636/2,057 functions, 20,244/24,787 regions and 4,342/7,495 instantiations.
Native-only mapped code remains in the linked WASM denominator at zero
coverage. The native barrier reaches 82/82 lines, 4/4 branches and 17/17
functions, while generic instantiations remain 46/74. mapped-grow.txt preserves
full and component metrics without excluding any production denominator.
Complete native/C#/linked-WASM 100% remains unfinished.

Selected growth adds an 8-byte known-initialization length, a temporary cloned
file and overlapping view, three brief lock acquisitions and a Windows mapping
handle when extending. It copies no old allocation into a heap buffer; ordinary
link edits add no lock. Failed callback prefix edits remain observable, and
failed preparation may extend the physical file. Failed shrink and failed
arbitrary destructor-bearing growth retain unsafe upstream failure states;
reopening alone cannot establish safe owner cleanup. Physical torn-page repair
and hardware power-loss verification remain unfinished. Fresh exact-head
platform CI is required, and every other unmet R1-R12 criterion remains open.
PR139 remains draft and unmerged.

### Native read-only discovery audit (2026-10-27T10:37Z)

Nine new contracts pass, followed by 2,460/2,460 tests in each full C# Debug and
Release suite. Exact physical bytes, owned reads, denied effects, acquisition
and cleanup failures, nonregular admission, deferred capabilities and borrowed
WAL ownership are independently checked. Debug covers 9,417/9,726 lines and
5,699/6,144 branches; Release covers 7,270/7,471 lines and 5,643/6,105 branches.
Both gain 17 covered lines/four branches without removing any denominator.
All 221 repository checks and strict local checks pass. The initial policy
failure after tracking the new growth probe manifest and its exact-directory
Dependabot correction are preserved in mapped-grow-manifest-policy.txt.
See discovery-guards.txt and DISCOVERY-GUARDS-PLAN.md. No production behavior
change or completion of wider R1-R12 gaps is claimed. Fresh platform CI remains
required and PR139 remains draft/unmerged.

## Empty and rejected Rust shrink boundaries (2026-10-27T11:01Z)

Four regressions first fail against immutable 2478d59: zero removal truncates
fresh/live files and erases sixteen unadopted persisted values; excessive
removal leaves the retained barrier invalid. Two complete-shrink/destructor
controls already pass. The wrapper now returns immediately for zero removal
and validates excessive removal before invalidating the live descriptor. Both
default/all-feature runs pass all six empty/shrink cases, six retained-growth
cases and six flush cases. Empty reopening, following growth and forty exact
destructor retirements preserve the existing genuine-shrink behavior.

Complete sequential verification finishes 2026-10-27T11:01:26Z: 1,190 native
tests, six doctests, 723 actual-WASM tests and all 221 repository checks pass.
Nine ownership unit controls, finite quota rejection and the owned-flush
example pass. Formatting, strict native/WASM Clippy, source limits, release
changeset validation, all 126 dependency declarations, workflow/ShellCheck and
whitespace checks pass. Twenty-two native child entries remain parent-driven;
one legacy doctest remains ignored. mapped-empty{,-before}.txt preserves the
commands, failures and complete metrics without filtering production source.

Current complete native LLVM: 17,156/18,036 lines, 3,293/3,780 branches,
2,217/2,379 functions, 26,395/28,542 regions and 12,351/22,019 instantiations.
Current complete linked WASM: 13,144/15,745 lines, 2,663/3,171 branches,
1,636/2,057 functions, 20,244/24,796 regions and 4,342/7,495 instantiations.
These are the current emitted report denominators, not a claim of a fixed
denominator or a percentage gain over an earlier build. Native-only mapped
source remains in linked WASM at zero coverage. Complete native/C#/linked-WASM
100% remains unfinished.

Zero removal adds no I/O, allocation, remap or lock. Excessive removal keeps
the upstream panic without retiring a valid owner. Genuine failed partial
shrink and destructor-bearing growth remain separate open safety hazards;
physical torn-page repair and hardware power-loss verification remain open.
Fresh exact-head platform CI is still required. Every wider unmet requirement
remains active, and PR139 stays draft and unmerged.

## C# version setup boundary and fresh platform evidence (2026-10-27T11:22Z)

The unchanged combined CreateAndUpdate helper reports no completed step between
its actual Create and Update. The immutable 950bcb3 boundary test fails; its
independent duplicate-row behavior control passes. The test child now borrows
an existing forwarding decorator to observe returned creation without replacing
the public helper, its callbacks, duplicate results or physical operations.
Existing transaction/version tracing is enabled only in that child. The
15-second idle limit, finite 64 consecutive phases, bounded captures and joined
reaping remain unchanged. No exact Windows scheduling/I/O cause is established.

Complete sequential local validation finishes 2026-10-27T11:22:31Z. All 31
focused controls pass: two setup boundaries, twelve actual version exits and
seventeen finite supervision controls. Debug and Release each pass 2,462 tests
with zero failed/skipped. Complete Debug covers 9,417/9,726 lines and
5,699/6,144 branches; Release covers 7,270/7,471 lines and 5,643/6,105 branches.
Both production denominators remain unchanged. All 221 repository tests, TS
declarations, formatting, source limits, release changeset, all 126 dependency
declarations, workflow/ShellCheck and whitespace checks pass.
version-setup{,-before}.txt retains commands, failures, metrics and limits.

All nine workflows now pass for previously published dec6cb7, created
2026-10-27T10:47:40Z after commit 10:47:33Z. Rust tests pass Windows/Linux/macOS,
coverage, merge simulation, lint and packaging. C# each platform passes all
2,460 tests, before this boundary change. The original Windows timeout is thus
intermittent; a successful run does not establish why it happened.
mapped-grow-ci-after.txt preserves exact SHA and run URLs/timestamps. Complete
C# log csharp-40491222901.log has Linux totals at 19043–19045, Windows at
25427–25429 and macOS at 31362–31364. Later Rust empty-shrink and C# progress
corrections require fresh exact-head CI. Complete native/C#/linked-WASM 100%,
physical repair/power-loss and every wider unmet criterion remain unfinished.
PR139 remains draft and unmerged.
