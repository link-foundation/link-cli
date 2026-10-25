# Row effects before observer journaling

R1/R6/R7/R8 remain open for an existing ordinary-transaction crash window:
the storage operation changes a row before delivering its observer callback,
and the transaction wrapper records observed rows afterward. An adapter can
persist the changed row before the undo record exists. Explicit name writes
have their own pre-effect barrier; that does not establish a row-write barrier.

- [x] Reproduce with a minimal automated test before changing production code.
      Use independent durable images and real bounded child processes which
      flush data and exit before forwarding the first observer. Cover allocation,
      update, deletion and cascades, including names and explicit/automatic owners.
      Reopen twice, compare complete rows/names, and perform a following edit.
      Include inner name reassignment affecting an unobserved owner: per-row
      resulting names alone cannot describe every custom adapter side effect.
- [x] Trace built-in map/mapped stores, custom adapters, callbacks, pending
      ownership, journal decisions, recovery order, retention and version intents.
      Inventory which operations can mutate without reporting an observation.
- [x] Evaluate pre-effect adapter hooks, staged writes and a durable pre-write
      undo image. A post-effect callback cannot alone provide write-ahead undo.
      Preserve the public API and ordinary callback delivery/coalescing. State
      exact costs; do not impose selected durability work on bare stores.
- [x] Implement matching Rust/C# behavior, with portable tests on actual WASM.
      Preserve failed preparation, uncertain append/flush and cleanup ownership.
      Validate complete metadata before mutation. Include unknown allocations,
      name side effects, noncontiguous checkpoints and repeated recovery.
- [x] Verify undo and version attribution across failures before/after the
      preparation barrier, effect, decision, checkpoint and publication. Preserve
      existing journal compatibility explicitly; old readers must not silently
      accept a new mandatory undo protocol they cannot interpret.
- [x] Run focused then complete local suites/coverage sequentially within the
      memory budget; document compatibility, costs and measured coverage.
- [x] Review, commit and push the prepared branch.
- [x] Verify every exact-head workflow after publication.

The selected `WriteAheadImage` policy now prepares a full logical image before
an owner's first effect. The default `Observed` policy retains its existing
cost and pre-callback window. Reconciliation also records unreported rows and
names before successful return, commit or version intent. Identical-name calls
have their own failing before cases: custom setters can change data even when
the predicted name delta is empty. Genuine no-ops allocate no history sequence.

Complete local validation passes: 820 native, 478 actual WASM, 1,901 C# tests
per Debug/Release configuration, 111 JS and 217 repository checks. Formatting,
lints, types, release metadata, source sizes, runtime conformance and persistent
examples pass. See row-wal.txt for official coverage; complete-project totals
remain below 100%. Published as fa03b5f. C# CI tests and fresh-merge checks pass; SDK 10.0.402
rejects one else/foreach layout at log lines 1997–1999 in run 40205855501.
The explicit-block correction f95f59d passes local formatting and 108 focused
C# cases. All nine exact-head workflows pass on f95f59d; see row-wal-ci.txt.
See TRANSACTION-UNDO.md for replay/flush assumptions, full-image costs and wire
compatibility. Process exits leave the OS alive; no physical
power-loss simulation or page-level atomicity is claimed.
