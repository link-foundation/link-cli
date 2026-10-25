# Row effects before observer journaling

R1/R6/R7/R8 remain open for an existing ordinary-transaction crash window:
the storage operation changes a row before delivering its observer callback,
and the transaction wrapper records observed rows afterward. An adapter can
persist the changed row before the undo record exists. Explicit name writes
have their own pre-effect barrier; that does not establish a row-write barrier.

- [ ] Reproduce with a minimal automated test before changing production code.
      Use independent durable images and real bounded child processes which
      flush data and exit before forwarding the first observer. Cover allocation,
      update, deletion and cascades, including names and explicit/automatic owners.
      Reopen twice, compare complete rows/names, and perform a following edit.
      Include inner name reassignment affecting an unobserved owner: per-row
      resulting names alone cannot describe every custom adapter side effect.
- [ ] Trace built-in map/mapped stores, custom adapters, callbacks, pending
      ownership, journal decisions, recovery order, retention and version intents.
      Inventory which operations can mutate without reporting an observation.
- [ ] Evaluate pre-effect adapter hooks, staged writes and a durable pre-write
      undo image. A post-effect callback cannot alone provide write-ahead undo.
      Preserve the public API and ordinary callback delivery/coalescing. State
      exact costs; do not impose selected durability work on bare stores.
- [ ] Implement matching Rust/C# behavior, with portable tests on actual WASM.
      Preserve failed preparation, uncertain append/flush and cleanup ownership.
      Validate complete metadata before mutation. Include unknown allocations,
      name side effects, noncontiguous checkpoints and repeated recovery.
- [ ] Verify undo and version attribution across failures before/after the
      preparation barrier, effect, decision, checkpoint and publication. Preserve
      existing journal compatibility explicitly; old readers must not silently
      accept a new mandatory undo protocol they cannot interpret.
- [ ] Run focused then complete local suites/coverage sequentially within the
      memory budget, document compatibility and costs, review, commit, push the
      prepared branch, and verify every exact-head workflow.

This is the next investigation after NAMED-HISTORY-PLAN.md. No row barrier or
power-loss guarantee is claimed from this plan. Keep all unverified criteria
open until the implementation and its failure/reopening evidence exist.
