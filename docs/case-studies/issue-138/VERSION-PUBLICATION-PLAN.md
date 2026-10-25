# Durable legacy version publication

Requirement mapping: R1/R3/R7/R8. Retaining an in-memory version owner establishes
graceful cleanup retry. It does not recover the branch/sequence attribution after
a real process exit between a durable journal decision and sidecar publication.
Rust successful commits also need an explicit branch-sidecar flush before their
acknowledgement can promise durable metadata.

- [ ] Establish finite child-process regressions for successful automatic and
      explicit commits, plus a durable decision followed by a real checkpoint
      error. Exit without destructors; restore injected filesystem path changes
      before exit. Verify exact rows, branch heads, checkout and following edits.
- [ ] Add uncommitted and rolled-back child controls that preserve the old head
      and reject the aborted sequence. Test actual C# mapped storage too.
- [ ] Persist a versioned publication intent containing the inner transaction
      identifier, selected branch and exact before/after sequence range before
      the journal decision. Flush branch topology with the intent.
- [ ] Publish committed transitions and branch/applied metadata idempotently,
      flush it, then remove and flush the intent. Retain admission on every
      fallible phase until cleanup succeeds.
- [ ] On startup resolve the intent using the recovered transaction decision:
      publish committed work; remove aborted/pending work only after durable undo.
      Reject corrupt/contradictory intents without guessing or destroying data.
- [ ] Cover intent write/flush, publication flush and intent removal/flush errors
      before and after effects; repeat cleanup and restart during recovery.
- [ ] Preserve ordinary no-ops, async policy, existing marker compatibility,
      retained history and same behavior in portable actual-WASM semantic tests.
- [ ] Document flush capability, added costs, compatibility and power-loss limits;
      run full local suites, coverage, strict checks and fresh exact-head CI.

A prepared Rust fixture is kept under experiments until the current full-suite
run completes, so test discovery cannot change during validation. Its child has
a ten-second deadline with kill-and-wait cleanup and only a finite three-state
history. No process-exit claim is based on ordinary drop/dispose reopening.
