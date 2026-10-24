# Version owner after durable commit and failed checkpoint

Requirement mapping: R1/R3/R8. Transactions mark a commit after its journal flush,
then checkpoint data and applied markers. Version Commit attributes branches only
after that inner call returns. Version transaction Dispose skips committed inner
transactions, so a post-commit checkpoint error may prevent owner cleanup from
completing version publication. Metadata publication itself clears the owner
before fallible branch writes, which needs a separate retry audit.

- [x] Trace the complete C# commit, checkpoint, version attribution and disposal
  paths, plus Rust's explicit commit/rollback retry behavior.
- [x] First reproduce post-commit cleanup and recovered history with real mapped
  faults before/after data Flush, applied marker and final journal Flush, through
  transaction disposal, owner disposal and whole-graph disposal.
- [x] Preserve the committed outcome and allow version attribution retry; keep
  admission through incomplete version metadata publication. Do not roll back a
  durably committed transaction or hide the original checkpoint error.
- [x] Verify metadata-write faults separately, including after-effect controls,
  exact branch/head recovery, rewind/forward checkout and subsequent transactions.
- [x] Run focused/full Debug/Release, coverage, formatting and release metadata.
- [ ] Commit/push and close fresh exact-SHA CI, preserving any failed logs.

The original 18 checkpoint cases all fail: transaction, version-owner and graph
disposal leave head 2 after durable commit 4. Adding six version-metadata phases
produces 36 failures; metadata cases admit Branch while publication is incomplete.
Two further finite regressions reproduce lost publication through Rollback and
accepted writes during pending publication. Both before logs are preserved.

The final matrix has 48 cases: twelve fault phases through transaction disposal,
owner disposal, graph disposal and rollback after a durable commit. Every case
checks write/branch/switch/tag/checkout/begin fencing, terminal state, exact
recovered head/data, unique transition markers, rewind/forward checkout, exclusive
file reopening and a following committed/tagged transaction. The matrix and all
31 existing rollback/lifetime controls pass. Commit/metadata errors still reach
the caller; cleanup preserves the known committed outcome and retries metadata.
