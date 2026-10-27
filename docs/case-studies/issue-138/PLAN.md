# Issue 138 work plan

The objective is every requirement of issue 138, in PR 139. No issue comments,
PR comments/reviews, or required sub-issues existed at initial inventory.
The issue asks for an open-ended set of competitor strategies; coverage and
implementation claims must name their measured scope, never imply that a
finite test suite proves all failure modes or that all DBMS have been surveyed.

## Checklist

- [x] Read the entire issue, conversation, inline comments, reviews, and sub-issue inventory.
- [x] Verify prepared branch and clean starting tree; locate repository guidance.
- [x] Audit existing storage, transactions, history, and language surfaces.
- [x] Study recent related PRs and primary competitor documentation/source.
- [x] Record requirements, proposed solutions, costs, and verification per requirement.
- [x] Reproduce durability/recovery failures with failing automated tests before fixes.
- [x] Correct transaction failure and persistence ordering; retain opt-in tracing.
- [x] Add interchangeable concurrency strategies and transaction isolation tests.
- [x] Add configurable version retention and test historical visibility.
- [x] Add replaceable synchronization decorators and failure/partition tests.
- [x] Implement matching staged-engine public surfaces in Rust, C#, and JS/TS.
- [x] Compose independent branches and atomic three-way merges with staged engines.
- [x] Implement network transport, named shard routing and consensus-backed storage.
- [x] Implement independently stored shard data and multiple consensus groups.
- [ ] Document cache, durable store, and distributed deployment recipes, examples,
      competitor comparisons, component reuse, and exact guarantee limits.
- [ ] Measure coverage, add missing edge cases, distinguish measured coverage from guarantees.
- [ ] Run focused tests then all available local suites, formatting/lint/type checks.
- [ ] Add release fragments/changesets; commit independently useful steps after checks.
- [ ] Fetch/merge latest main; push only issue-138-adc2aa56c4cb.
- [ ] Update PR title/body with reproduction, tests, evidence, limits, and Fixes #138.
- [ ] Review gh pr diff, feature preservation, fresh feedback, and clean git status.
- [ ] List recent CI runs with timestamps/SHAs; save failed logs under experiments/issue-138/ci-logs;
      investigate actual errors and verify fixes against new runs.
- [ ] Verify all relevant CI on final SHA and mark PR 139 ready when implementation is complete.

## Resource and evidence discipline

C# run 40107157910 failed on Windows because Flush reopened an exclusively
mapped file. Commit 52d4da5 flushes the owned mapping and file handle; subsequent
run 40107962518 passes all three platforms, 562 tests each. Both logs are
preserved. Coverage measurements are currently below 100%; do not replace that
requirement with test counts. Final engine changes require new CI on their SHA.
Rust run 40107962488 separately failed two Windows durability tests: `sync_all`
used a read-only handle. Commit 126168f opens it writable; subsequent run
40109338737 passes Windows, Linux, macOS and fresh-merge checks.

Run compilers sequentially (Cargo jobs=1; reduced debug info), save long logs,
keep reusable probes in experiments/issue-138 and real examples under examples/.
Bound concurrency/stress probes; never exhaust the container's 2.92 GiB budget.
No direct merges or default-branch pushes. Preserve forward-moving history.

## Current failure-audit sequence

1. C# known-commit checkpoint ownership and native named-write fencing are
   verified locally and in all nine exact-SHA workflows at `8429c53`. The 112
   C# and six Rust before-fix failures are retained. See
   CSHARP-CHECKPOINT-PLAN.md and checkpoint-owner-ci.txt under the PR evidence.
2. Ordered recovery and live retention regressions pass in full native, C#
   Debug/Release and actual-WASM suites. Recovery keeps an anchor until the
   complete committed suffix and pending undo are durable. All nine workflows
   pass on `5905049`; see RECOVERY-ORDER-PLAN.md and recovery-order-ci.txt.
3. Uncertain commit-marker append/first-flush ownership passes full local
   suites in both native languages and actual WASM. Ordinary explicit/automatic
   owners and explicit version cleanup are covered. All nine workflows pass on
   `7c0da30`. See COMMIT-OUTCOME-PLAN.md and commit-decision-ci.txt.
4. Initial version histories exclude explicit/recovered aborts, reject invalid
   attribution and reject a live inner owner. Nine cases per native language
   first fail; full local suites and actual WASM pass. Eight workflows pass on
   1c1ef38; C# Windows fails two async waits. See INITIAL-HISTORY-PLAN.md and
   initial-history-ci.txt; resolve the scheduling root cause before finalization.
5. Automatic version writes now retain an implicit owner through failed decisions,
   checkpoints and metadata publication. Full local native/C#/actual-WASM suites
   pass, including pre-commit undo, async and no-op controls. All nine workflows
   pass on 86891b7; see AUTOMATIC-VERSION-PLAN.md and automatic-version-ci.txt.
   A finite isolated child reproduces idle ThreadPool worker starvation; an
   asynchronous channel wait fixes it while preserving ordered checkpoints,
   joining, cancellation and callback self-join guards. Full C# Debug/Release
   suites pass 1,308 tests each. All nine exact-head workflows pass on
   24554b0; see ASYNC-WORKER-PLAN.md, async-worker.txt and async-worker-ci.txt.
6. Persisted publication intents now resolve exact branch attribution after
   abrupt process exit. Six real child cases per native language first reproduce
   the missing head; publication/recovery faults and contradictory vectors pass
   full native/C#/actual-WASM validation. Fresh CI exposes ten Windows preservation
   fixtures reading files while exclusive mappings are live. Move byte snapshots
   outside mapped lifetimes. All nine workflows pass on 1f742c5; see
   VERSION-PUBLICATION-PLAN.md and PUBLICATION-WINDOWS-PLAN.md.
7. Truncated history checkout now rejects missing changed records before any
   undo or redo. Three reproductions per native language and actual-WASM shared
   contracts pass; native reopen and available suffix/no-op controls pass. See
   RETAINED-HISTORY-PLAN.md. The subsequent movement audit covers durable
   replay ownership and restart recovery; see VERSION-MOVEMENT-PLAN.md.
   Public unlogged replay now rejects pending and failed commit/undo owners.
   Six before/after cases per native language, existing version controls and
   full native/C#/actual-WASM suites pass. See REPLAY-OWNER-PLAN.md; all nine workflows pass on replay-owner
   head 31a50a0. Same-handle file-journal reuse after partial append now has
   six failing before schedules per native/JS adapter. Track framing until
   repair and its flush finish, then validate reopening and repeated faults;
   see INPROCESS-TAIL-PLAN.md. All nine workflows pass on journal-tail head
   7826d95; see live-tail-ci.txt. Durable checkout/branch movement now preserves
   exclusive ownership and a portable intent through failures. Focused tests
   cover metadata, data/flush faults, restart, corrupt topology, evicted payloads
   and repeated constructor recovery. Full local suites and all nine exact-head
   workflows pass on d4cb43c; see version-movement.txt and version-movement-ci.txt.
   Frozen-clock Rust restart identity reuse now has four native and four
   actual-WASM before/after contracts. Full local suites and all nine workflows
   pass on 6d231fa; see TRANSACTION-IDENTITY-PLAN.md and transaction-identity-ci.txt.
   A separate finite probe reproduces two databases overwriting a shared chunk
   archive. Four native and three C# occupied-path cases first fail; exclusive
   creation now passes full local validation and both shared-archive CLI examples.
   All nine workflows pass on archive head 7b03c07; see ARCHIVE-OWNERSHIP-PLAN.md,
   archive-ownership.txt and archive-ownership-ci.txt. Contradictory transaction
   journals and failed live recovery now have failing before contracts and
   passing full local suites. Validation preserves idempotent records and
   retained floors; cleanup retains recovery/refresh ownership. See
   RECOVERY-VALIDATION-PLAN.md and recovery-validation.txt. All nine workflows
   pass on exact published head 0e95c81 (2026-10-25 04:03:39 UTC creation).
   The interrupted ancestry work was preserved in f5af989 and recovered without
   rewriting history. Continue DEEP-ANCESTRY-PLAN.md and SESSION-RECOVERY-PLAN.md;
   ancestry changes pass full local checks and all nine workflows on aa601e2;
   see deep-ancestry-ci.txt.
   Full suites pass: 770 native, 433 actual WASM, 1,695 C# per configuration,
   106 JS and 217 repository tests. See deep-ancestry.txt for exact coverage.
   Reproduce ordinary legacy named writes across rollback/history/reopen. Their
   admission fence is distinct from logging names atomically; staged sessions
   already keep names in revisions.
8. The independent-session shared-flush strategy now passes full local checks:
   784 native, 446 actual WASM, 1,722 C# per configuration, 111 JS and 217
   repository tests. Separate revisions, known aborts, prefix discovery after
   unknown I/O, real files and twenty native process exits are verified. All nine
   workflows pass on 615df74; see group-commit-ci.txt and GROUP-COMMIT-PLAN.md.
   Continue the
   source-backed algorithm/component comparison, efficient journal
   and state structures, finer decorator composition and full native/linked-WASM
   coverage. Every unsupported strategy and unverified guarantee stays open.

9. Ordinary legacy names now have complete portable history, rollback,
   recovery and version movement. Full suites pass: 801 native, 460 actual WASM,
   1,788 C# per configuration, 111 JS and 217 repository tests. Shared codecs,
   independent images, bounded exits, archives and callback failures verify
   the documented paths; bare stores retain their path. See
   NAMED-HISTORY-PLAN.md and named-history.txt. This step is published and
   all nine exact-head workflows pass on `9d394d5`. ROW-WAL-PLAN.md follows with
   durable row-before-observer reproductions recorded before the protocol change.
   Complete coverage and all wider acceptance gaps remain open.

10. The optional ordinary row undo image now has failing before tests and passing
    focused native/C#/actual-WASM tests. Preparation precedes effects; complete
    result reconciliation preserves unreported changes for redo and versions.
    Existing observer behavior remains selectable. Independently authored codec
    and ordering vectors validate the mandatory protocol before mutation. The
    identical-name fast path has separate before failures and no-op controls.
    Complete local suites pass: 820 native, 478 actual WASM, 1,901 C# tests per
    configuration, 111 JS and 217 repository checks. Runtime conformance and
    persistent examples pass; see row-wal.txt for exact coverage. Forward
    publication is complete and all nine workflows pass on `f95f59d`; see
    row-wal-ci.txt. IMAGE-REPLAY-PLAN.md extends replay compatibility and coverage.
    See ROW-WAL-PLAN.md; physical page durability/repair and efficient state
    structures remain distinct follow-up requirements.

11. Undo-image compatibility controls pass full local suites and all nine
    workflows on `71cad1c`; see IMAGE-REPLAY-PLAN.md and image-replay-ci.txt.
    Shared immutable state now has finite pre-implementation allocation evidence
    and passing focused native/C#/actual-WASM contracts. The representation is
    independent of other strategy choices; old wire and public map interfaces
    remain compatible. Complete shared-state suites, examples and coverage pass, and all nine
    workflows pass on `3960c25`; see SHARED-STATE-PLAN.md and shared-state-ci.txt.

12. Incremental logical revision encoding now has runnable before failures,
    independent vectors and passing focused native/C#/actual-WASM-boundary
    controls. Serialization visits changed rows; shared recovery retains prior
    roots. Mixed encodings, branch attribution, groups, uncertain durable prefixes
    and forty bounded process exits per native language preserve prior behavior.
    Complete compatibility/coverage and all eight persistent cross-language
    configurations pass; changes.txt records the official totals. All nine workflows pass on dd22de1; changes-ci.txt records SHA, timestamps
    and links. See CHANGE-RECORDS-PLAN.md. Page WAL/repair, finer composition,
    full-project coverage and wider cluster/competitor criteria remain open.

13. The optional SQLite journal now has pre-implementation adapter contracts,
    independently authored BLOB/schema vectors and passing focused native/JS
    controls. The selected component supplies physical WAL/FULL and explicit
    checkpoints while preserving existing logical encodings and default builds.
    Complete local suites pass: 852 native, 1,989 C# per configuration, 496 actual
    WASM, 168 JS and 217 repository tests. All eight SQLite configurations pass
    Rust -> C# -> Node -> Bun -> Deno persistent reopening; sqlite.txt records
    official coverage. All nine workflows pass on `3322f36`; sqlite-ci.txt records
    exact SHA, timestamps and links. See
    SQLITE-JOURNAL-PLAN.md. Legacy page repair, LSM, complete
    coverage and wider cluster/competitor criteria stay open.

14. Independently selected persistent constraint indexes now preserve scan
    defaults and allocate only selected pair/name/reference maps. Pre-feature
    contracts fail before implementation; bounded independent oracles, root
    sharing, rejected candidates, group prefixes, branches and independent
    admission/read validation pass. Full local suites pass: 859 native, 2,003 C#
    per configuration, 502 actual WASM, 168 JS and 217 repository tests. All
    sixteen SQLite configurations use the same References policy and reopen
    through all five providers. Official coverage and finite allocation evidence
    are recorded in constraint-index.txt. All nine workflows pass on `864839b`
    after the SDK redirect correction; constraint-index-ci.txt preserves exact
    SHA, timestamps and links. See CONSTRAINT-INDEX-PLAN.md. Wider criteria remain open.

15. The independently selected immutable object cache keeps catalog reads fresh
    and retains only checked partition/address/generation payloads. Zero remains
    the default. Finite independent UTF-8/LRU oracles, caller ownership, failed
    fetch/repair, unchanged staging comparisons, concurrent fills and uncertain
    publication are covered in all three languages and actual WASM. Real etcd
    controls pass both zero and 65,536-byte budgets, including the public native
    and JS examples, cached data-group outages and cached catalog-majority loss.
    See OBJECT-CACHE-PLAN.md and the object-cache evidence. Complete local
    verification passes: 866 native, 2,016 C# per configuration, 509 actual WASM,
    177 JS and 221 repository checks. All nine workflows pass on `4729b53`;
    object-cache-ci.txt preserves exact SHA, timestamps and links.
    Wider acceptance criteria remain open.

16. Unicode sequence graph integrity now has bounded native before failures:
    cyclic strings do not terminate, and Rust silently accepts missing children
    as truncated output. Checked iterative traversal rejects these cases while
    preserving Unicode order, repeated shared subtrees, deep finite graphs,
    converter APIs and UTF-16 compatibility. C# reuses the installed converter
    through its walker interface. Focused tests include malformed names,
    repeated errors, healthy following reads and persistent reopening. Complete
    local suites pass: 875 native, 2,022 C# per configuration, 516 actual WASM,
    177 JS and 221 repository checks. The checked walker reaches 100% measured
    component coverage; whole-project totals remain below target. See
    UNICODE-GRAPH-PLAN.md, unicode-graph-before.txt and unicode-graph.txt.
    All nine workflows pass on `1a482ac`; the second recovery is recorded in
    SESSION-RECOVERY.md. Wider criteria stay open.

17. Deep structure output has bounded before stack-overflow failures in concrete
    Rust, generic Rust and C#. Shared explicit source-spine frames preserve
    cycle references and deepest-first name resolution, then assemble one output
    buffer. Complete local suites pass: 882 native, 520 actual WASM, 2,027 C#
    per configuration, 177 JS and 221 repository checks, plus Node/Bun/Deno
    generated-WASM conformance. Whole-project coverage remains below target.
    All nine workflows pass on `faf191d`; structure-depth-ci.txt records the
    exact head, timestamps and links. See
    STRUCTURE-DEPTH-PLAN.md and structure-depth.txt. The next independent
    audit addresses observed whitespace truncation in Rust's legacy inline-name
    text parser. Wider criteria remain unfinished.

18. SQLite optionality now has automated resolved-dependency guards for default,
    bare and WASM Rust targets, C# core packages and JS root/native-file paths.
    Explicit provider selection and a deliberate rejected JS SQL import verify
    the guards. All guards pass locally, including Node 24; 26 native mapped
    store/file-journal tests pass without optional providers. CI invokes these
    checks; all nine workflows pass on `faf191d`, including all new guards on
    their supported platforms. ENGINE-SQLITE.md maps native
    implementations separately from unfinished physical strategies. A pinned
    LMDB shadow-page source study evaluates the next native candidate without
    claiming implementation. See SQLITE-OPTIONALITY-PLAN.md and sqlite-optional.txt.

19. Rust legacy inline names now round-trip exact whitespace, quote edges,
    controls, line breaks and absent-address names without introducing rows.
    Seven automated before failures also reproduce destructive failed reload;
    temporary load/index construction now preserves the live state on error.
    Independent wire vectors, malformed/future metadata, repeated reopen/edit
    cycles and portable memory reload pass. Full local suites pass: 894 native,
    521 actual WASM, 2,027 C# per configuration, 177 JS and 221 repository checks,
    plus runtime conformance and the native document-label example. The existing
    TCP shutdown regression exposed a runtime socket-clear race during Release
    validation; a shutdown-only catch fixes it and both configurations pass.
    See INLINE-NAMES-PLAN.md, TCP-SHUTDOWN-PLAN.md and inline-names.txt. All nine
    workflows pass on `b125e6d`; inline-names-ci.txt records exact timestamps
    and links. Native physical strategies remain open;
    NATIVE-PAGES-PLAN.md records the next implementation and independent vectors.

20. The SQL-free JS/TS native doublets page journal now has independent wire
    vectors, finite I/O/corruption/ownership contracts and public exports/types.
    Full JS tests pass 197 cases with 100% production coverage, plus 221
    repository checks and 16 actual-WASM configurations in each of Node/Bun/Deno.
    New-group writes stay constant after 0/1/32 preceding groups. See
    native-pages-js.txt and NATIVE-PAGES-PLAN.md. Rust/C# parity, redundant
    repair and Rust/C# parity remain unfinished. All nine workflows pass on
    `21847f9`; native-pages-js-ci.txt records exact timestamps and links.

21. Rust native doublets pages now pass ten portable contracts on native and
    actual WASM, five real-file contracts, the private uncertain-close control,
    strict native/WASM Clippy and 221 repository checks. The public example
    reopens the same files through Rust/Node/Bun/Deno in all 16 configurations.
    The optional file feature adds no SQL dependency and no native dependencies
    to WASM. Complete suites pass 910 native and 531 actual WASM checks.
    See native-pages-rust.txt.

22. Matching C# native pages now pass 18 focused contracts and both full suites
    with 2,045 tests per configuration, plus formatting, dependency guards and
    221 repository checks. Rust/Node/Bun/C#/Deno/Rust sequential reopening passes
    all 16 policy configurations, ending with 12 exact revisions per file.
    See native-pages-csharp.txt. Independently selected mirrored repair and
    fresh exact-head CI remain unfinished, as do the wider inventoried criteria.

This sequence tracks remaining work; passing one audit or CI batch does not
complete the overall issue or authorize marking the draft ready early.

23. The independently selected JS/TS mirror now passes 12 new contracts, 209
    complete JS tests at enforced 100% production coverage on Node 26 and 24,
    strict types and 221 repository checks. Each JS runtime passes 32 actual-WASM
    configurations, including physical corruption, validated repair and following
    commits. Independent mode-1 vectors and 36 damaged-copy cases are preserved.
    Matching Rust/C# mirrors and all-language interchange remain in progress.
    See MIRRORED-PAGES-PLAN.md and mirrored-pages-js.txt.

24. Matching Rust/C# mirrored native pages now pass independent corruption,
    persistence and cleanup contracts. Full suites pass 922 native Rust, 539
    actual WASM, 2,060 C# per configuration and 209 JS tests. All 32 single/mirror
    compositions pass 192 shared-file writer invocations through all providers,
    repairing deliberate closed-copy damage before following commits. Native
    formatter/strict Clippy/optional-SQL guards pass. Fresh C# CI exposed a
    formatter difference and a test that read an exclusively owned Windows file;
    the correction and Rust mirror now pass all nine workflows on b0612d0.
    See MIRRORED-PAGES-PLAN.md and mirrored-pages-parity-ci.txt for exact evidence.

25. The complete-denominator command audit reproduces and corrects rejected
    delimiter-containing tag names in both native executables while preserving
    aliases to complete existing tags. Thirty-one C# entry-point contracts check
    configuration/error recovery, mapped ownership release, versions, triggers
    and file pipelines; the real Rust executable contracts and both public
    examples pass. Complete runs pass 923 native and 2,091 C# cases per
    configuration. C# command coverage improves from 597/759 to 736/767 Debug
    lines and 626/734 to 714/742 Release lines. Bounded nesting controls confirm
    the existing declared parser limit and subsequent usable files. See
    CLI-COVERAGE-PLAN.md and cli-before.txt. All nine workflows pass on 63ccbc7;
    cli-ci.txt preserves the exact head, creation timestamps and links.
    Whole-project 100% and all wider inventoried criteria remain open.

26. Thirty-three public query compatibility contracts preserve empty/comment/
    leaf/incomplete shapes across all three retained C# processors. Exact rows,
    names, notifications, following edits and native reopening pass. Rejected
    arguments/wildcards, existing-pair reuse and basic mismatched-update
    diagnostics retain established behavior. No null-list defect reproduces,
    so production code remains unchanged. Both complete C# configurations pass
    2,124 tests; complete measured lines reach 96.11% Debug and 96.79% Release,
    still below the target. See QUERY-COMPATIBILITY-PLAN.md and
    query-compatibility.txt. All nine workflows pass on 580dcb2; exact SHA,
    timestamps and links are preserved in query-compatibility-ci.txt. Every
    wider unfinished requirement stays active.

27. Seven public argument contract groups pass natively and in actual WASM,
    comparing complete configurations, aliases, exact literal values, booleans,
    numeric/arity boundaries, rejection recovery and independent strategy flags.
    No production defect reproduced. Complete suites pass 930 native and 546
    WASM cases with unchanged production denominators. Native coverage reaches
    94.29% lines and actual WASM 78.24% lines; parser coverage is 404/405 native
    and 394/405 WASM lines. See ARGUMENT-CONTRACTS-PLAN.md and arguments.txt.
    All nine workflows pass on 164f6f0; arguments-ci.txt records exact evidence.
    Every wider inventoried criterion remains open.

28. A finite real-quota child first reproduces failed mapped growth disposing
    the current view while retaining its stale pointer and reservation. C# now
    keeps its owned stream and current view until a replacement is ready. Failed
    initialization releases only new resources; logical shrink defers physical
    truncation until used-length disposal. Repeated rejection, unflushed bytes,
    flush, retry, following writes, exact reopening and normal shrink/regrowth
    pass. Both full C# configurations pass 2,126 tests with complete coverage;
    formatting/release/source and 221 repository checks pass. See
    MAPPED-GROWTH-PLAN.md and mapped-growth-{before,}.txt. Exact-head C# CI passes
    all three platforms; Rust Windows fails an existing movement child deadline.
    Journal-only e39ecce later passes all nine workflows before movement
    instrumentation. Every broader unfinished requirement remains active.

29. Seventeen journal value contracts preserve captured ownership, single
    enumeration, read-only names, complete equality/hash behavior, public header
    properties and finite full UTF-16 codec controls. No production defect
    reproduces. Both full configurations pass 2,143 tests with unchanged
    production denominators, plus format/release/source and 221 repository checks.
    See JOURNAL-VALUE-PLAN.md and journal-values.txt. Exact-head 1aafc31 C# CI
    passes all three platforms; Rust Windows fails an existing movement child
    deadline. Journal-only e39ecce passes all nine workflows, and diagnostic
    head 3c22e69 also passes all nine, without extending that deadline or
    establishing its earlier cause. See journal-values-ci.txt and
    movement-diagnostics-ci.txt. Every wider inventoried criterion remains open.

30. Independently selected immutable sorted runs now use the same native/WASM/
    JS/TS engine surfaces. A real pre-feature compiled binding rejects selection;
    finite map oracles, shared-array controls, all nine conversions and expanded
    replay/group/branch/fault matrices pass. Full local suites pass 935 native,
    546 actual-WASM, 2,164 C# per configuration and 209 JS tests on Node 26/24,
    plus strict types and 221 repository checks. All 48 physical/policy choices
    pass 288 shared-file writers, including mirrored damage and repair. Complete
    coverage remains below target; exact values and the initial wrapper-script
    error are preserved in sorted-runs.txt and sorted-runs-wrapper-before.txt.
    See SORTED-RUNS-PLAN.md. All nine exact-head workflows pass at a1f12fec,
    including Linux, macOS and Windows; sorted-runs-ci.txt retains the evidence.
    Physical SSTables and every wider unmet criterion remain unfinished.

31. Fallible allocation now composes through persistent transformations,
    query planning and transaction/version owners. Ten real-filesystem tests
    first reproduce journal-error panics; two portable tests reproduce ignored
    faults and wrong dispatch. Four portable contracts now verify eleven
    bookkeeping fault positions, successful/no-op triggers and parked errors;
    five C# cases are exception-propagation controls without production changes.
    Full suites pass 949 native plus six doctests, 550 actual-WASM, 2,169 C# per
    configuration and 209 JS, plus strict types and 221 repository checks.
    Cleanup, following writes and two exact native reopenings pass. See
    FALLIBLE-COMPOSITION-PLAN.md and fallible-composition{,-before}.txt. All 48
    physical/policy choices pass 288 shared-file writers with mirrored repair.
    All nine workflows pass at c530a6a, including Linux/macOS/Windows;
    fallible-composition-ci.txt preserves exact evidence.
    Import allocation bypasses are traced separately in IMPORT-ALLOCATION-PLAN.md;
    their fix is recorded in step 32. Every wider unmet criterion stays active.

32. Text and archive imports now use fallible allocation for explicit addresses,
    anonymous nested pairs and reallocation after a selected trigger deletes a
    reserved row. Six native panics and five portable ignored faults first fail;
    named-import controls retain their passing behavior. Full local suites pass
    963 native plus six doctests, 556 actual-WASM, 2,172 C# per configuration,
    209 JS and 221 repository checks. Cleanup, retries, following writes and two
    exact native reopenings pass. All 48 physical/policy choices pass 288 shared
    file writers with mirrored repair. See IMPORT-ALLOCATION-PLAN.md and
    import-allocation{,-before}.txt. All nine workflows pass at 847ed01,
    including native Linux/macOS/Windows; import-allocation-ci.txt preserves
    exact heads, creation timestamps and workflow URLs.
    Complete 100% coverage and every wider unmet acceptance criterion stay active.

33. Archive compatibility now shares ten existing native contracts with actual
    WASM while retaining all six native remote/CLI/filesystem contracts. Four
    finite stream groups check every read/write fault boundary, truncation,
    trailing bytes, one-byte I/O, exact rows/names, retries and following writes.
    A WASM-only file-helper capability control preserves path context and store
    reuse. No new production defect or production change is claimed. Complete
    suites pass 967 native plus six doctests and 571 actual-WASM tests, with
    complete official coverage of 94.37% and 81.45% lines respectively. All 221
    repository checks, optional-SQL guards and strict native/WASM Clippy pass.
    See ARCHIVE-STREAM-PLAN.md and archive-stream.txt. At d98c2ee, eight workflows
    pass; four Rust Windows version-exit cases hit their existing ten-second
    deadline. Phase/stream diagnostics are published at b03bd4e; all nine fresh
    workflows pass, including every native platform. The Windows exit suite
    passes seven contracts in 6.64 seconds with the unchanged child deadline.
    The original timeout cause remains unknown. See archive-stream-ci-before.txt
    and version-exit-diagnostics-ci.txt. Whole-project 100%
    and every wider unmet criterion stay active.

34. Optional logical checkpoint journals now preserve exact retained snapshots
    and committed allocator metadata through matching native and WASM/JS APIs.
    An independently selected cluster interval publishes through the existing
    generation CAS; default commits retain append behavior. Before-feature wire
    and binding failures, branch integration regressions, selected constraints,
    shrinking/equal-length journals, immutable objects and uncertain outcomes
    pass. Complete local suites pass 975 native plus six doctests, 2,180 C# per
    configuration, 580 actual-WASM, 211 JS and 221 repository checks. Real etcd
    passes default/checkpoint/cache modes; all 48 existing physical/policy
    configurations pass 288 writers. Node/Bun/Deno produce identical 36-case
    payload results. See JOURNAL-CHECKPOINT-PLAN.md and journal-checkpoint.txt.
    All nine workflows pass on a329fa7, including every native platform; see
    journal-checkpoint-ci.txt. Two initial Windows cleanup failures pass on
    an unchanged-code debug rerun; their cause remains unknown. Initial-checkpoint
    support in the physical delta decorator is verified separately under
    CHECKPOINT-DELTA-PLAN.md. This does not reclaim old objects, replace local
    files/pages or complete any wider unmet criterion.

35. Initial checkpoints now compose with the independently selected physical
    delta codec in Rust, C# and JS. Shared independent before vectors first fail;
    following snapshots compress, logical changes reset that dependency, invalid
    order rejects before mutation, and failed I/O fences the owner. Retention,
    allocator holes, immutable pins and both replica orders pass. Full fresh
    suites pass 980 native plus six doctests, 585 actual-WASM, 2,187 C# per
    configuration, 215 JS and 221 repository checks. Node/Bun/Deno agree on 36
    compositions; real etcd passes three modes and existing file interoperability
    passes 48 configurations/288 writers with mirrored repair. The file fixtures
    start at ordinary revision one. See CHECKPOINT-DELTA-PLAN.md and
    checkpoint-delta{,-before}.txt. Complete project coverage and every wider
    criterion remain unfinished. All nine workflows pass on 68fa923, including
    every native platform; checkpoint-delta-ci.txt records exact evidence.

36. Native page examples now select an independent revision-40 checkpoint with
    32 named rows and allocator gaps 33 through 40. All 48 physical/policy
    configurations pass 288 shared-file writer handoffs, ending at revision 52
    with 44 rows. The ordinary revision-zero control also passes 48/288. Both
    include damaged-copy repair in mirrored mode. Examples reject a different
    existing prefix instead of replacing it; no production storage change is
    claimed. CHECKPOINT-PAGES-PLAN.md and checkpoint-paged.txt record scope and
    verification. Atomic local replacement and all wider criteria remain active.

37. Optional portable two-bank journals now replace a complete ordered sequence
    by preparing an inactive bank and then synchronizing an epoch-bound root.
    Independent shared vectors and missing-feature
    controls precede implementation. Finite faults before/after every baseline I/O,
    all 128 torn-root lengths, stale bank identities, mirrored-bank reuse, pending
    input, exact Unicode bytes and u64 limits pass. Two additional Rust ownership
    regressions first fail, then verify that both banks drop before root on ordinary
    destruction and every tested constructor failure.

    Complete local suites pass 992 native Rust tests plus six doctests, 597 actual
    WASM tests, 2,198 C# tests in each configuration, 226 JS tests and 221 repository
    checks. Node/Bun/Deno each pass all 11 portable controls and actual-WASM integration;
    36 composition and payload reports agree exactly across all three runtimes.
    See LOCAL-CHECKPOINT-PLAN.md and local-checkpoint{,-before}.txt. The preceding
    persisted-checkpoint head cdff1b4 passes all nine workflows and every native
    platform; checkpoint-paged-ci.txt records exact heads and timestamps. This new
    batch requires fresh exact-head CI. Publication under the engine commit lock,
    physical-delta replacement and native owning factories remain unfinished, as do
    whole-project 100% coverage and every wider unmet acceptance criterion.

38. Engine checkpoint publication now retains the commit mutex through optional
    replacement, including independently selected physical delta codecs. Exact
    history, pins, private/committed allocator distinctions, pending sessions,
    point locks, admission and branch bases survive. Unsupported logs reject
    before mutation; uncertain publication fences the engine. All 72 bank/state/
    encoding/history/delta combinations pass in Rust/C# and actual WASM across
    Node/Bun/Deno. Full suites pass 1,002 native plus six doctests, 620 actual
    WASM, 2,210 C# per configuration, 230 JS and 221 repository checks. All nine
    workflows and every native platform pass on 133c4ff; engine-compaction-ci.txt
    preserves exact evidence. Native owning factories continue under
    NATIVE-CHECKPOINT-PLAN.md; whole-project coverage and all wider gaps remain
    active.

39. Native owning checkpoint factories now acquire every device before recovery
    mutation, preserve original/all cleanup errors, close banks before the root
    and retain uncertain fences without implicit flush or retry. Missing-feature
    controls and two JS error-masking regressions precede implementation. Bounded
    actual replacement/repair exits, exact following commits and independent
    format-conversion rejection pass in Rust/C#/JS. Complete suites pass 1,013
    native plus six doctests, 620 actual WASM, 2,227 C# per configuration, 239 JS
    and 221 repository checks. All three persistent controls pass 48/288 handoffs;
    etcd and runtime comparisons pass. NATIVE-CHECKPOINT-PLAN.md and
    native-checkpoint{,-before}.txt retain exact coverage and selected costs.
    Commit, fresh exact-head CI and every wider unmet criterion remain active.

The native checkpoint head `600cf5f` passes eight workflows, while C# Windows
fails an exclusive-file test snapshot and an existing physical async probe.
The independent test correction closes before snapshotting and keeps that
physical probe with unchanged deadlines, adding trace and positive/negative
scheduling controls. An isolated base-plus-test-patch archive passes 2,229
tests per C# configuration and 221 repository checks. See
native-checkpoint-ci-before.txt and native-checkpoint-ci-fix.txt. The atomic
test correction is now published as `6a0f7c9`: all nine workflows and
both native platform matrices pass, including the unchanged physical probe and
independent controls on Windows. native-checkpoint-ci-fix.txt records exact
timestamps and run URLs. Its earlier checkpoint-progress observation still has
no established cause. The selected mirrored-root batch and every broader
acceptance criterion continue independently.


40. Checkpoint roots now select two-copy redundancy independently of page-bank
    mirroring in all providers. Canonical decision validation, complete bank
    validation, staged source revalidation and synchronized repair preserve
    acknowledged groups; unknown formats, incompatible modes and uncertain I/O
    fail closed. Existing modes and constructor signatures remain available.
    Missing-feature controls, independent u64 vectors, all root-copy bytes,
    finite I/O schedules and 258 real process cuts per native language verify
    the declared scope. Complete suites pass 1,028 native plus six doctests,
    631 actual WASM, 2,264 C# per configuration, 261 JS and 221 repository checks.
    Native compaction passes 96 configurations/576 handoffs with root/bank damage;
    existing ordinary/imported 48/288 controls and all real-etcd modes pass.
    See MIRRORED-ROOT-PLAN.md, mirrored-root{,-before}.txt and the protocol recipe.
    Publication and fresh exact-head CI follow. Complete coverage, physical
    power-loss testing and every wider unmet acceptance criterion remain active.

41. Optional namespace maintenance now supplies complete bounded revision-pinned
    scans and exact-generation single-send deletion in Rust, C# and JS. Existing
    atomic providers retain their API; selected codecs initialize on demand.
    Missing-API failures, 94 independent wire cases and real three-member native/
    WASM handoffs precede publication. Full suites pass 1,032 native plus six
    doctests, 631 actual WASM, 2,268 C# per configuration, 359 JS and 221 repository
    checks. All nine workflows and native platform matrices pass on `44e0fa8`;
    MAINTENANCE-PLAN.md and maintenance{,-before,-ci}.txt retain exact evidence.

42. Independently selected epoch-fenced reclamation now composes that capability
    with immutable object publication. Publishers compare the observed epoch;
    collectors retain their own fence, validate every complete namespace before
    deletion, protect aliased roots and delete exact listed generations. Owned
    readers discard whole retired groups and retry once only after both pointer
    and catalog generation change. Tests and finite models precede implementation;
    RECLAMATION-PLAN.md tracks complete validation, publication and fresh CI.
    Wider acceptance gaps remain active throughout this batch.


43. Reclamation public contract controls now cover seven matching construction,
    routing, capability and interrupted-I/O behaviors in Rust/C#/JS, including
    actual WASM. No production correction is needed. Complete sequential suites
    pass 1,064 native Rust plus six doctests, 663 actual WASM, 2,314 C# per
    configuration, 525 JS, 166 selected Bun/Deno each and 221 repository checks.
    Full coverage denominators remain intact; whole-project coverage is open.
    All nine preceding production workflows pass on `419f7c7` across native
    platforms. RECLAMATION-CONTRACT-PLAN.md tracks the new test publication and
    exact-head CI; every wider unmet acceptance criterion remains active.

44. The immutable physical-table batch and checkpoint process slices now pass
    all nine workflows on exact head `523888a`, including all six native
    platform jobs. `sorted-table-ci.txt` preserves run/job URLs and timestamps.
    The semantic LSM checkpoint/manifest/group codec follows as a selected
    pure component in Rust/C#/JS, with a shared independent 111-case oracle.
    `LSM-WAL-PLAN.md` tracks its full validation and atomic publication.
    `PHYSICAL-LSM-PLAN.md` retains the production store/provider and native
    process work. Complete local suites pass; `lsm-wal.txt` retains measured
    scope and complete denominators. Atomic publication and fresh exact-head
    CI follow. All broader unmet criteria remain active.

45. Physical-store behavioral fixtures fail on the named missing APIs in all
    three languages on codec-only baseline `d12760f`. An isolated JS prototype
    composes the real checkpoint WAL, sorted tables and semantic codec; thirteen
    controls pass in Node/Bun/Deno, including 152 before/after publication cuts
    per runtime. Two additional before-failing controls correct hidden historical
    conflicts and a returned device's durability mismatch in the experiment.
    Six separate read-only selected-prefix controls pass in all three runtimes.
    The exact Node CI commands pass locally and now run in the JS workflow.
    `LSM-PROTOTYPE-PLAN.md` and lsm-store-{before,prototype}.txt retain evidence.
    Portable production stores, native ownership/discovery, process/interchange
    tests and all wider acceptance criteria remain active.

46. Semantic codec head `d12760f` passes eight workflows; Rust Windows fails
    three movement parents because the aggregate ten-second child deadline
    expires during progressing setup, before the intended crash point. The
    preserved log and exact line references are in movement-progress-ci-before.txt.
    A finite monotonic phase supervisor keeps the ten-second idle limit, rejects
    repeated/partial/malformed progress and kills/reaps on errors. All original
    40 movement/recovery scenarios remain. Three deterministic harness controls,
    focused execution, formatting, strict Clippy and 221 repository checks pass.
    A separate clean production archive verifies this test-only correction;
    fresh exact-head Windows CI now passes on `f523b5c`, including all six
    controls, both other native platforms, coverage and fresh merge.
    movement-progress-ci-after.txt records job URLs and log line references.
    Dependencies alone fails because rustix 1.1.6 supersedes three 1.1.5 locks;
    update those locks, retain the downloaded log and verify fresh publication.
    Wider R1–R12 work remains active.

47. Promote the finite physical composition into selected production Rust,
    C# and JavaScript APIs under `PORTABLE-LSM-PLAN.md`. Preserve the missing-API
    baseline and reproduce early repair completion and lost close retry before
    correcting them. Verify all historical bindings before repair/retirement,
    preflight publication and following-group bounds, retain metadata-only disk
    indexes, and exercise real cluster, partition and reclamation consumers.
    Native Rust and C# each pass the expanded 18-control target; JavaScript has
    33 controls, with its cluster binding explicitly mocked. Complete sequential local validation finishes 2026-10-27T01:52:07Z: 1,096
    distinct native Rust tests plus six doctests, 692 actual WASM, 2,369 C# per
    configuration, 568 JS, 209 selected Bun/Deno each, 25 existing native table
    handoffs and 221 repository checks pass. Full coverage denominators remain
    below 100% outside JS. lsm-store.txt retains measured evidence. Publish this
    verified step and inspect its own fresh exact-head CI. All nine workflows pass
    on bc364d2; lsm-store-ci.txt retains the exact-head jobs and timestamps.
    `NATIVE-LSM-DISCOVERY-PLAN.md` tracks the following read-only
    checkpoint inspection, exclusive directory ownership and actual file/process
    composition. Every broader unmet R1–R12 criterion remains active.

48. Add borrowed checkpoint inspection before native LSM directory recovery.
    Rust/C#/JS return owned selected-prefix records, preserve suffixes and staged
    repairs, and perform no initialization, identity allocation or device close.
    Actual missing APIs fail first. Complete sequential validation finishes
    2026-10-27T02:37:37Z: 1,102 native tests plus six doctests, 698 actual WASM,
    2,375 C# per configuration, 576 JS at enforced 100% production coverage,
    217 selected Bun/Deno each, all 25 native immutable-table handoffs and 221
    repository checks pass. Every native/C#/WASM production denominator remains
    included and below the whole-project target. checkpoint-inspect{,-before}.txt
    retain measured evidence. Publish this verified prerequisite and check fresh
    exact-head CI; continue the remaining NATIVE-LSM-DISCOVERY-PLAN.md steps and
    every wider unmet R1–R12 requirement in this draft PR.

    Fresh c501b2a workflow policy rejects two historical baseline manifests as
    unwatched maintained packages. Preserve those manifest contents with .fixture
    suffixes and keep the dependency policy unchanged. The staged correction
    passes the workflow assertion; move downloaded CI logs under the existing
    experiment tree to satisfy the root-layout check. The complete local rerun
    passes all 221 repository checks. checkpoint-inspect-ci-before.txt preserves
    the exact failure, log line and cause; inspect fresh correction-head CI.

49. Implement the JavaScript native LSM directory facade before porting the same
    contract to Rust/C#. Missing native APIs first fail on archived bc364d2 in all
    three languages. Exclusive bounded discovery validates the complete selected
    semantic state read-only before any constructor recovery effects. Independent
    mirrors, exact-once uncertain raw cleanup and actual file persistence pass
    41 controls each in Node/Bun/Deno, including 134 finite compaction syscall cuts
    and ten reaped abrupt children per runtime. All 617 Node tests enforce 100%
    production coverage; strict types, optional SQL and lazy imports pass. A fresh
    256MiB-capped WASM binding passes direct/partition/reclamation consumers in all
    three runtimes. native-lsm-js{,-before}.txt retain measured evidence. Publish
    this atomic step after repository/workflow gates; inspect fresh exact-head CI.
    Rust/C# native facades, whole-store interchange and all wider unmet R1–R12
    criteria remain active. Inspection prerequisite 7baafed now has all nine green
    exact-head workflows, recorded in checkpoint-inspect-ci.txt.

50. Port native LSM discovery and ownership to C#. Missing APIs first fail on
    archived bc364d2. A surviving-copy test also first exposes a new reader's
    incorrect exception category; EndOfStreamException permits checked mirror
    reads while all selected validation still precedes repair. Both complete
    2,409-test configurations, 34 new native controls, three actual consumers and
    32 C#/Node/Bun/Deno whole-store handoffs pass. Keep complete coverage below
    100% explicit. Publish this verified atomic step after all repository gates,
    inspect its exact-head CI and continue the Rust port, five-runtime interchange
    and every wider unmet R1-R12 requirement. Preceding JS head 39a551d now has
    all nine workflows green, preserved in native-lsm-js-ci.txt.

51. Investigate exact-head native C# publication CI. Preserve the failure log
    and separate formatter mismatch from Windows process failures. Match the
    pinned SDK in local validation; reproduce missing finite supervision APIs,
    then verify child-stream retention, actual kill/reap and primary-plus-cleanup
    aggregation. Keep the existing child deadline while collecting coarse phase
    diagnostics. Both full configurations pass 2,412 tests; three actual native
    consumers, 32 handoffs and repository gates pass with the exact pinned SDK.
    Publish this atomic correction. Fresh Windows CI must establish the remaining timeout
    cause; a passing Linux suite does not complete that criterion.

    Diagnostic head 78e2c2a passes all C# jobs, including all 2,412 tests on each
    platform and the fresh merge. The preceding timeout cause is not established
    by this passing run. native-lsm-csharp-ci-after.txt retains the exact evidence.

52. Investigate Rust Windows failures on diagnostic head 78e2c2a. Preserve the
    complete log, actual failing scenarios and child phase timings. Each failed
    child progresses through distinct persistence operations until the aggregate
    ten-second deadline expires. Reuse the existing finite movement supervisor,
    retaining ten-second idle detection, monotonic bounded phases and kill/reap.
    All original 12 version/name scenarios remain; focused execution passes the
    seven parents and three supervisor controls. Formatting, strict Clippy and
    all 221 repository checks pass. Publish this test-only correction and inspect
    fresh Windows CI. version-progress-ci-before.txt preserves the reproduction;
    lower-level Windows filesystem latency and wider issue criteria remain open.

53. Complete the native Rust LSM directory facade using the existing SQL-free
    native pages, checkpoint WAL, mirrors and immutable tables. Read-only bounded
    discovery precedes constructor recovery effects; outer ownership survives
    uncertain live cleanup. All 21 native unit controls, ten integration cases,
    408 compaction cuts, 730 opening cuts and 408 actual reaped abrupt children
    pass. Full sequential validation finishes 2026-10-27T05:18:14Z: 1,138 native
    tests plus six doctests, 698 actual WASM, three real cluster consumers and
    all 50 Rust/C#/Node/Bun/Deno complete-store handoffs. Preserve whole-project
    coverage denominators below 100%. native-lsm-rust{,-before}.txt retain exact
    scope and fixture corrections. Publish this verified atomic step and inspect
    fresh exact-head CI. All broader unmet R1–R12 criteria remain active.

    Preceding supervision head 3dfbc74 passes all Rust jobs, including Windows;
    version-progress-ci-after.txt preserves its exact run. Eight workflows pass.
    C# Windows alone preserves a progressing movement child killed by its total
    15-second deadline; csharp-phase-progress-before.txt records actual log lines
    and a separately failing finite local control. Keep its 15-second idle bound
    while adding strict bounded consecutive phase supervision. Full local and
    fresh Windows checks are required before claiming that correction verified.

54. Replace the C# child's aggregate deadline with strict finite phase admission.
    The actual progress control first fails under the previous helper. Keep the
    same 15-second idle bound, accept only complete consecutive phases 1..64,
    reject malformed/repeated progress and retain bounded streams and kill/reap.
    All 50 focused controls and both complete 2,421-test configurations pass with
    SDK 10.0.403. Three actual native consumers, 32 complete-store handoffs and
    optional-SQL controls pass; full production coverage remains unchanged and
    below 100%. csharp-phase-progress{,-before}.txt preserve exact evidence.
    Publish this test-only correction with the locally verified native Rust port;
    inspect fresh exact-head Windows CI and continue every wider unmet criterion.

    Exact published head 8d2ecdf now passes all 2,421 C# tests on Windows, Linux
    and macOS in run 40456464654. The two finite deadline regression controls and
    both current-before movement cases pass on Windows. Preserve their exact
    log lines in csharp-phase-progress-ci-after.txt. This verifies the helper
    correction; it does not establish the cause of filesystem latency.

    All nine workflows on 8d2ecdf now pass, including Rust Windows within its
    existing job budget. native-lsm-rust-ci-after.txt records every run and exact
    completion time. This establishes publication verification for steps 53–54;
    subsequent chunk work and every wider unmet requirement remain active.

55. Select bounded immutable journal chunks independently of the existing
    partitioned store's cache and engine settings. Preserve blob1 defaults,
    reject malformed selected metadata before data access and admit all updates
    before staging. Missing APIs fail first in Rust/C# and JS first publishes
    blob1 instead of chunks1. Shared vectors, owned-cache and complete staging
    checks pass. All 626 Node tests reach enforced 100% production coverage;
    eight focused controls pass in Bun/Deno, with strict types. Seven chunk
    controls per native language, actual native engines and fifty directed
    plain/mirrored five-runtime chunk-store handoffs pass. Complete validation
    passes 1,146 native tests plus six doctests, 706 actual-WASM and 2,429 C#
    tests in each configuration. Full denominators remain included below 100%.
    Both fifty-handoff layouts and actual-engine payload/cost checks pass.
    Published as 860a47f; a newly released lucide-react dependency fails the
    freshness check and is corrected separately in 58d35f8. All nine correction-
    head workflows pass, including native Windows, Linux and macOS. Exact runs
    and timestamps are preserved in journal-chunks-ci-after.txt.
    SEGMENTED-JOURNAL-PLAN.md records selected costs and open chunk
    reclamation work. Every wider unmet R1–R12 criterion remains active.

56. Extend the separately selected epoch collector to the chunk reference graph.
    Independent finite models first reproduce live chunk loss when old-epoch
    keys are reused or epoch CAS is omitted. Current-epoch staging passes all
    modeled interleavings, including two writers/two collectors, under finite
    state and memory limits. A separate JS API control first reproduces ignored
    chunk selection (blob2 instead of chunks2); native Rust/C# API controls also
    fail before production changes. The selected chunks2 codecs, complete graph
    protection, current-epoch staging and whole-group refresh pass 46 focused
    Rust tests, 60 C# tests, all 640 Node tests with enforced 100% production
    coverage and fourteen new controls in Bun/Deno. Every four-chunk cache mask,
    ten staging/publication cuts, overlapping collectors and following edits
    pass. Full sequential checks finish 07:27:05Z: 1,159 native plus six
    doctests, 719 actual-WASM, 2,442 C# per configuration, five real compositions,
    150 complete-store handoffs and both real three-member etcd layouts pass.
    All children are reaped and the isolated original three-language controls
    reproduce again. Full production denominators remain included below 100%;
    exact commands, costs and limits are in reclaiming-chunks{,-before}.txt.
    Publish and inspect fresh exact-head CI under CHUNK-RECLAMATION-PLAN.md.
    Every wider unmet R1-R12 criterion stays active; append-only chunks1's safe
    refusal alone did not establish this selected reclamation behavior.

    Published c1abe42 passes eight workflows, including all Rust test platforms;
    C# Windows alone fails the finite progress supervisor despite phase5/exit0.
    reclaiming-chunks-ci-before.txt preserves all exact-head runs. The following
    controlled caller-context reproduction must be corrected and verified on
    fresh Windows CI; this failed head is not reported as fully passing.

57. Reproduce the C# progress supervisor's captured-context dependency before
    modifying its awaits. A real finite child and the durable regression both
    report phase5/exit0 with a false timeout. Replay unchanged c1abe42 helper
    files from an immutable archive to preserve the failure after correction.
    Keep 500ms/15-second idle limits, finite consecutive phases, bounded streams
    and unconditional reaping. All 13 focused controls and the linked-helper probe
    pass with no caller callbacks after ConfigureAwait(false). All 2,447 tests
    pass in each C# configuration, and all 221 repository checks pass after
    relocating ignored CI logs to the permitted case-study tree. Type checks,
    source limits, changeset, 126 dependency declarations and workflow lint pass.
    Preserve phase-context.txt, publish an atomic correction and
    inspect fresh exact-head CI under PHASE-CONTEXT-PLAN.md. This test helper
    change supplies no new production persistence guarantee.

58. Exercise public reclaiming-store admission and provider-failure paths in
    both whole-object and chunked layouts. Short/long arrays reject before I/O;
    generation-zero payloads, uppercase epoch/hash keys and otherwise eligible
    orphans fail without publication/deletion. A generation-only catalog change
    cannot hide a missing unchanged object behind a second data retry. Following
    reads/writes and all three language surfaces preserve the existing contract.
    Full native/actual-WASM suites pass 1,163/723; both C# configurations pass
    2,447; all 644 Node tests enforce full production coverage. Four new controls
    also pass in Bun/Deno. After strengthening the eligible-orphan fixture, all
    50 native maintenance controls, all actual WASM and all C# Debug tests rerun
    successfully, finishing 08:23:27Z. Complete coverage retains every production
    denominator; native and linked-WASM each gain four covered branches. All
    repository checks pass. Preserve reclamation-admission.txt, publish the
    atomic tests/docs step and inspect fresh exact-head CI. These controls add
    no production protocol and do not complete the wider R1-R12 inventory.

    Both steps 57–58 now pass all nine workflows on exact published head
    7e9799a, created after its 08:24:39Z commit. C# passes all 2,447 tests on
    Windows/Linux/macOS, including the previously failing progress control and
    the deferred-context regression. phase-context-ci-after.txt records every
    exact SHA, timestamp and run link. This establishes the helper correction's
    platform verification; every wider unmet criterion remains active.

59. Retain a lifetime-checked flush owner for Rust's legacy mapping. Primary
    Windows documentation requires mapped-view flush before file-buffer flush;
    the prior adapter and upstream Drop synchronized only the file. New public
    API controls first fail against immutable 7e9799a. Independent syscall mocks
    verify ordering, both failures and retry; actual allocations verify the
    complete range after growth/shrink. The independent range assertion first
    catches a tail-only draft descriptor. Closed/failed-remap handles reject
    before revision publication, owner Drop releases retained files, and generic
    adopted stores retain explicitly attached ownership through decorators.
    Twelve new controls, six default-feature controls and a runnable adoption
    example pass. Full sequential validation finishes 09:13:25Z: 1,175 native
    plus six doctests, 723 actual-WASM, all 221 repository checks and strict local
    checks pass. Full coverage retains every production denominator below 100%.
    mapped-flush{,-before}.txt and MAPPED-VIEW-FLUSH-PLAN.md preserve evidence,
    cost and limits. Publish this atomic step and verify fresh exact-head native
    Windows/Linux/macOS CI. Upstream failed-resize safety, physical legacy repair,
    hardware power-loss verification and every wider unmet requirement remain
    unfinished; this barrier must not be presented as their completion.

60. Retain the actual Rust mapping through rejected plain-data growth. A finite
    Linux 4 KiB quota probe first returns EFBIG and then faults through the safe
    retained allocation API; its within-quota control passes. Five durable
    regressions first fail. Prepare an overlapping replacement owner before
    retiring the old view, and remember actual initialization bounds before
    page padding or failed extension. On Windows extend through a temporary
    mapping object, preserving the live old view. Quota, overflow, panicking
    fill and injected mapping failures now retain pointer, prefix, flush owner
    and successful retry; generic destructor behavior keeps its existing path.
    Complete sequential local verification passes 1,184 native tests, six
    doctests, 723 actual-WASM tests and all 221 repository checks at 10:22:42Z.
    Full production coverage denominators remain in mapped-grow.txt. Publish
    and inspect fresh exact-head platform CI under RUST-MAPPED-GROWTH-PLAN.md.
    Failed shrink, arbitrary destructor-growth errors
    and physical torn-page repair remain separate unfinished work.

61. Investigate the Linux finite-progress failure on exact published 89fbfad.
    Download the full failing C# log and establish the actual phase 5 / exit 0
    timeout rather than increasing limits. A bounded occupied-worker child
    independently reproduces it; all three durable healthy/stalled/invalid
    controls first fail because supervision needs that worker. Dedicated
    process/pipe tasks and a monotonic idle clock remove this dependency; all
    17 focused controls pass without widening any bound. The deferred-context
    regression and unconditional process reaping remain checked. All 2,451 C#
    tests pass in each Debug/Release configuration; complete coverage, all 221
    repository checks and strict local checks pass in phase-pool.txt. Correction
    950bcb3 is published; fresh exact-head CI remains active under
    PHASE-POOL-PLAN.md. Record both the successful Rust Windows
    view-flush checks and the independent C# failure in mapped-flush-ci-after.txt.

62. Audit C# native read-only discovery without production behavior changes.
    Nine new contracts exercise owned reads, every denied mutation, malformed
    parts, bounds, nonregular handles, exact-once cleanup, acquisition plus
    cleanup failures, deferred capability forwarding and borrowed WAL ownership.
    All 2,460 tests pass in both configurations; full production denominators
    remain unchanged and each gains 17 covered lines/four branches. All 221
    repository checks and strict local checks pass at 10:37:00Z after correcting
    the newly tracked probe manifest's Dependabot watch. discovery-guards.txt,
    mapped-grow-manifest-policy.txt and DISCOVERY-GUARDS-PLAN.md preserve the
    evidence. Publish this independent step and inspect fresh exact-head CI.
    R8 complete coverage and every wider unmet requirement remain unfinished.

63. Match the fresh 950bcb3 C# failure to its actual Windows log. All eight
    other workflows pass; Linux/macOS each pass 2,451 C# tests. The original
    finite-progress and occupied-worker controls also pass Windows. Its one
    failure is name-exit/success/True, phase6/version-open, killed after the
    unchanged 15-second idle bound. The following compound CreateAndUpdate
    combines two automatic version transactions with no intermediate progress.
    Preserve phase-pool-ci-after.txt, add an actual operation-boundary regression
    before correction, observe returned creation through a borrowing forwarding
    decorator without changing helper callbacks, ownership or persistence, run
    full sequential verification and inspect fresh platform CI.
    VERSION-SETUP-PROGRESS-PLAN.md records the limits: the log alone does not
    identify which inner flush was slow or establish a physical storage hang.
    All 31 focused controls and 2,462 Debug/Release tests each now pass;
    complete production coverage denominators remain unchanged. All 221
    repository and strict checks pass at 11:22:31Z. version-setup.txt retains
    the immutable baseline boundary failure and independent duplicate-row
    behavior control. All nine prior dec6cb7 workflows pass, including 2,460
    C# tests per platform before this visibility change; its Windows timeout
    was intermittent. mapped-grow-ci-after.txt preserves that exact-head
    evidence. Publish the progress correction and inspect its fresh CI.

64. Preserve zero-removal and rejected-shrink ownership boundaries in Rust.
    Six finite tests against immutable 2478d59 first reproduce four failures:
    zero removal truncates fresh/live files, erases unadopted persisted items,
    and excessive removal invalidates a still-valid flush descriptor. Complete
    shrink, empty reopening and destructor retirement already pass. Return
    immediately for zero removal and validate excessive removal before retiring
    the descriptor. Both focused feature configurations pass all six cases;
    complete validation finishes 11:01:26Z with 1,190 native tests, six doctests,
    723 actual-WASM tests and all 221 repository checks passing. Full production
    coverage denominators remain included. Publish the atomic correction and
    inspect fresh exact-head platform CI under
    RUST-EMPTY-MAPPING-PLAN.md. Genuine failed partial shrink, failed generic
    destructor growth, physical repair and every wider unmet criterion remain
    unfinished.

65. Preserve generic mapped growth admission without retiring a valid owner.
    Three finite regressions first fail against immutable 9bb985f: arithmetic
    overflow, layout overflow and zero-growth callback panic invalidate a
    still-valid flush barrier. Ordinary zero growth already passes. Preflight
    capacity/layout and invoke zero-growth callbacks directly against the old
    prefix. Both feature configurations verify all four controls, following
    growth and forty exact destructor retirements. Complete sequential checks
    finish 11:50:17Z with 1,194 native tests, six doctests, 723 actual-WASM tests
    and 221 repository checks passing. Complete coverage denominators remain
    included. Publish this atomic correction and inspect fresh platform CI
    under RUST-GROW-ADMISSION-PLAN.md. Real resize I/O failure, nonzero callback
    panic, failed shrink, representation safety and all wider unmet criteria
    remain unfinished.

66. Retain each C# platform's existing complete coverage evidence. Successful
    dec6cb7 collects three reports but publishes only Linux. The real artifact
    verifier rejects the missing Windows/macOS reports before correction.
    Remove the Linux-only gates from summary/retention, preserve the stable
    Linux artifact name and Codecov behavior, and give each other platform a
    distinct artifact. All 221 repository and strict workflow checks pass
    locally. Fresh exact-head 3cd89e9 C# run 40500064469 passes 2,462 tests per
    platform. All three actual archives download, verify their sizes/SHA256
    against GitHub and report complete metrics in csharp-platform-coverage.txt.
    CSHARP-PLATFORM-COVERAGE-PLAN.md retains platform differences. This does not
    complete the 100% requirement or infer a gain from retaining evidence.

67. Correct logical initialization for destructor-bearing Rust mappings.
    Three finite controls first fail against immutable 9bb985f and current
    pre-fix 40d850f, using only valid transparent-u64 representations. Forward
    the known initialized tail rather than upstream's minimum-page count.
    Both feature configurations preserve original values, within/cross-page
    growth, following edits, checked flush and exact destructor retirement.
    Complete sequential checks finish 12:19:05Z with 1,197 native tests, six
    doctests, 723 actual-WASM tests and all 221 repository checks passing.
    All production coverage denominators remain included; actual metrics and
    controls are retained in mapped-generic-initialization{,-before}.txt.
    Publish this atomic correction and inspect fresh exact-head platform CI
    under RUST-GENERIC-INITIALIZATION-PLAN.md. Genuine generic resize/partial
    shrink failures, callback/destructor panic, representation safety and all
    wider unmet criteria remain unfinished.
