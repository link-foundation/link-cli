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
