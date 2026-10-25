# Independent sessions with a shared durability boundary

R1/R2/R5/R6/R7/R8/R9/R11: the pinned InnoDB study distinguishes written and
durable positions. One flush may acknowledge several independent transactions.
The present engine holds its publication mutex through each append and flush,
so a log wrapper alone cannot combine those commits. Several edits in one
transaction do not supply this strategy.

- [ ] Reproduce the existing two-session/two-flush behavior with an instrumented
      log; add a contract requiring separate contiguous revisions and one flush.
- [ ] Add an opt-in group coordinator around existing sessions. Validate in input
      order against private tentative state; retain ordinary isolation, constraints,
      conflict generations, allocator state and pessimistic ownership rules.
- [ ] Append each accepted revision separately, flush once, then publish while
      holding the existing engine gate. An empty or entirely aborted group must
      perform no journal I/O. Ordinary commits must preserve their current path.
- [ ] Keep known aborts separate from uncertain append/flush outcomes. After an
      uncertain outcome, retain the old visible state, fence the engine and
      discover the valid committed prefix through reopening. Do not claim that
      the group is one atomic transaction or that an I/O error reverses a prefix.
- [ ] Verify constraints, conflicting/disjoint/read-only sessions, stale point
      reads and scans, history, names, allocator gaps, closed/foreign sessions,
      admission release, repeated cleanup and following commits. Use finite
      before/after append/flush faults and independently captured durable images.
- [ ] Implement matching C# and Rust-backed JS/WASM surfaces, with actual-WASM
      binding tests, shared portable contracts and precise TypeScript types.
- [ ] Add recipes for buffered append plus durable flush, delta encoding and
      fixed-primary acknowledgement; document selected coordinator memory costs
      and the single serialization point. This does not add an automatic timer
      or scheduler, page repair, anti-entropy or a leaderless consensus protocol.
- [ ] Run focused then complete local checks and coverage sequentially; retain
      exact evidence, update release fragments, self-review, commit, publish only
      the prepared branch and verify all exact-head CI and latest feedback.

Use bounded groups and fault matrices in tests. No unselected raw store or
ordinary session should acquire a batch queue, worker or timer. Keep all broader
acceptance and coverage gaps open until their own implementation is verified.
