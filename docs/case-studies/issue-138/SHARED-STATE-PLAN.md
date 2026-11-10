# Next acceptance batch: shared immutable state

R6/R11 still document full row/name copies on scans and commits. Read paths
currently clone the full map, even a GetByName result on the first row. Snapshot
retention adds O(N) unique row/name allocations for every single-row commit.
The physical DeltaLog decorator reduces bytes only; it still receives complete
encoded revisions. These are separate costs and must be measured separately.

1. [x] Publish the complete image-replay batch (`71cad1c`); all nine workflows pass.
2. [x] Audit ordered persistent map components and license/target/Send+Sync support;
   prefer a maintained reusable library to a new balancing algorithm.
3. [x] Select flat versus structurally shared immutable rows independently of
   concurrency/history/log/replication. Existing bare stores and default options
   preserve their behavior. Keep native public snapshot interfaces compatible.
4. [x] Add finite cost and independent semantic tests before implementation:
   unchanged node/value identities shared only when selected, old snapshots
   unchanged, insert/update/delete/no-op/name paths, zero/maximum addresses,
   history retention, read committed/snapshot scans, failed commit, restart,
   group commits and branches with different state choices. Use counters or
   allocation identities rather than flaky wall-clock thresholds.
5. [x] Add shared vectors and matching C#/Rust-backed JS/WASM options; reject
   invalid configuration and maintain strict TS definitions.
6. [x] Implement without making a materialized public map a hidden engine read
   dependency. Document first compatibility-map materialization O(N), complete
   revision encoding O(N), and selected constraints/scans remaining O(N).
7. [x] Add bounded examples/measurement scripts under examples/experiments,
   run focused semantic/cost probes, then complete local checks sequentially.
8. [ ] Continue row/page WAL and physical persistence separately: structural sharing
   alone does not implement page WAL or make full-revision CPU incremental.

Component audit (2026-10-25, pinned downloads and SHA-256 in
the local state-component-audit.json): rpds 1.2.1 at
50a486ec8e4fade33a286af4bc35c2728b1f268f supports ordered Arc-based structural
sharing, logarithmic updates and constant-time clones; values live in their own
shared entries, without requiring Clone. Its Rust 1.85 floor fits the project's
1.89 floor. Select RedBlackTreeMapSync, never default Rc map, to preserve native
Send/Sync. Verify actual wasm32, all-features and supported compiler targets.
Microsoft System.Collections.ImmutableSortedDictionary is available in the
existing .NET target and path-copies frozen AVL nodes. No new C# package needed.
The im-rs alternative is archived (last source push 2024-08-19), so prefer rpds's
maintained MIT component. Do not copy either library's implementation.

Implementation sketch: internal RowMap Flat/Shared selected by independent
StateStructure option, Flat default. Rust Snapshot still exposes
rows() -> &BTreeMap for source compatibility, backed by a lazy OnceLock only for
Shared snapshots; internal reads, validation and encoding must never invoke that
compatibility materialization. Arc<RowMap> snapshot roots let clone retain the
same cached materialization. A mutable transaction candidate clones only the
persistent root and resets any materialization cache. All values/entries of
unchanged rows remain shared. Flat still clones rows exactly as before.
C# snapshot Rows remains IReadOnlyDictionary, backed by read-only flat map or
immutable sorted dictionary; candidate helper overlays selected structure.
Reopening/forking converts to the receiving engine's choice. Wire unchanged.
Views avoid mutating snapshot maps; scans can share/merge rather than clone all
rows. Preserve sorted iteration, closed-session reads and conservative scan
read-set tracking. Whole engine1 record serialization remains O(N) and must stay
explicit in docs and cost claims.

Design details to preserve cost boundaries:
- An enum iterator avoids boxing (and avoids a new allocation on flat scans).
  Shared iterators may keep O(log N) traversal state provided by the component.
- rpds entries already hold values behind shared ownership: do not wrap Row in
  another Arc. Pointer-identity tests on unchanged rows can verify sharing.
- RowMap clone must reset Shared's lazy flat cache, while Snapshot clone shares
  its Arc root/cache unchanged. Mutating candidates cannot mutate old snapshots.
- Rows::into_structure moves flat row values into the shared map once; converting
  back materializes/moves if possible. Reopening converts every retained revision
  according to selected history; state publish converts mismatched fork roots.
- Keep the existing engine1 wire and its full O(N) encoding. Do not route the
  internal encoder, validator, fork equality, or WASM exporter through rows().
- C# IReadOnlyDictionary can expose ImmutableSortedDictionary directly, while
  internal candidate helpers preserve old read-only flat wrappers. Shared callers
  may invoke SetItem, which returns a separate map and cannot mutate a snapshot.
- Add genuine no-op/read-only/rollback/copy materialization checks, both structure
  choices in conflict/constraints/group/branch and independently persisted reopen
  tests. Keep any allocation probe finite, non-timing and separately documented.

Baseline measured before implementation: 256 rows, sixteen single-row commits,
zero of 4,080 unchanged row references shared. The finite reusable probe is in
experiments/issue-138/state-copy-probe; full output is state-copy-before.log.
This observes allocation identity, not a timing or power-loss claim.

Focused verification passes: eight portable native cases, two private native
cost/cache checks, eighteen C# cases and the same eight portable cases in actual
WASM. The public WASM option-validation contract, strict native/WASM Clippy,
formatting, TS types, source sizes and release fragments pass. The same finite
probe now measures Flat 0/4,080 and Shared 4,080/4,080 unchanged row identities.
Feature tests were written before the option existed; their missing-API compiler
failure is preserved separately from the runnable flat allocation baseline.
Complete compatibility/coverage suites and persistent examples pass. Exact totals
and official coverage are recorded in shared-state.txt; complete-project coverage
remains below the requested target and no timing claim is made.

Complete local suites, coverage, all three runtime checks and cross-language
examples pass; see shared-state.txt for exact official totals and limits.

9. [x] Review, commit, push and verify every fresh exact-head workflow.
   All nine workflows pass on `3960c25`; see shared-state-ci.txt.
