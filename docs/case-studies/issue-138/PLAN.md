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
- [ ] List recent CI runs with timestamps/SHAs; save failed logs under ci-logs;
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
   outside mapped lifetimes and verify new exact-head CI; see
   VERSION-PUBLICATION-PLAN.md and PUBLICATION-WINDOWS-PLAN.md.
7. Truncated history checkout now rejects missing changed records before any
   undo or redo. Three reproductions per native language and actual-WASM shared
   contracts pass; native reopen and available suffix/no-op controls pass. See
   RETAINED-HISTORY-PLAN.md. Atomic replay failures remain a separate audit.
   Reproduce ordinary legacy named writes across rollback/history/reopen. Their
   admission fence is distinct from logging names atomically; staged sessions
   already keep names in revisions.
8. Continue the source-backed algorithm/component comparison, efficient journal
   and state structures, finer decorator composition and full native/linked-WASM
   coverage. Every unsupported strategy and unverified guarantee stays open.

This sequence tracks remaining work; passing one audit or CI batch does not
complete the overall issue or authorize marking the draft ready early.
