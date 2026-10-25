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
    official coverage. Fresh exact-head CI is required after publication. See
    SQLITE-JOURNAL-PLAN.md. Legacy page repair, LSM, complete
    coverage and wider cluster/competitor criteria stay open.

This sequence tracks remaining work; passing one audit or CI batch does not
complete the overall issue or authorize marking the draft ready early.
