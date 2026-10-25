# Durable legacy version publication

Requirement mapping: R1/R3/R7/R8. Retaining an in-memory version owner establishes
graceful cleanup retry. It does not recover the branch/sequence attribution after
a real process exit between a durable journal decision and sidecar publication.
Rust successful commits also need an explicit branch-sidecar flush before their
acknowledgement can promise durable metadata.

- [x] Establish finite child-process regressions for successful automatic and
      explicit commits, plus a durable decision followed by a real checkpoint
      error. Exit without destructors; restore injected filesystem path changes
      before exit. Verify exact rows, branch heads, checkout and following edits.
- [x] Add uncommitted and rolled-back child controls that preserve the old head
      and reject the aborted sequence. Test actual C# mapped storage too.
- [x] Persist a versioned publication intent containing the inner transaction
      identifier, selected branch and exact before/after sequence range before
      the journal decision. Flush branch topology with the intent.
- [x] Publish committed transitions and branch/applied metadata idempotently,
      flush it, then remove and flush the intent. Retain admission on every
      fallible phase until cleanup succeeds.
- [x] On startup resolve the intent using the recovered transaction decision:
      publish committed work; remove aborted/pending work only after durable undo.
      Reject corrupt/contradictory intents without guessing or destroying data.
- [x] Cover intent write/flush, publication flush and intent removal/flush errors
      before and after effects; repeat cleanup and restart during recovery.
- [x] Preserve the existing C# constructor's binary signature through a separate
      explicit-flush overload. Reproduce a missing signature before correcting it;
      test all adapter-capability combinations and caller-flush ordering.
- [x] Preserve ordinary no-ops, async policy, existing marker compatibility,
      and same behavior in portable actual-WASM semantic tests.
- [x] Document flush capability, added costs, compatibility and power-loss limits;
      run full local suites, coverage and strict checks.
- [ ] Verify fresh exact-head CI after publication.

The real exit regressions first fail: C# loses attribution in both checkpoint
error cases; Rust also loses it after both acknowledged commits. Six cases per
language now pass, including pending and rollback controls. Children terminate
without destructors, with finite ten/fifteen-second kill-and-wait deadlines.
The complete behavioral before logs are retained under the PR evidence.

The publication matrix covers nine phases, both sides of each effect and every
cleanup entry point: 90 mapped C# cases and 108 portable Rust schedules. Fourteen
recovery interruptions per language verify reopening after failed publication,
removal and flushing. Shared valid/invalid vectors cover exact identities,
boundaries and accepted delimiter/Unicode/NUL branch names. Nine contradictory
metadata schedules per language verify rejection without rewriting sidecars.
Native restart checks also verify checkout, a following edit and a second reopen.
Full native/C#/actual-WASM validation passes; see version-publication.txt for
measured coverage separately from test counts. Referenced truncated history
remains a separate unfinished audit. Power-loss and other remaining legacy audits are
not implied by this process-exit protocol.
