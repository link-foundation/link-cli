# Facade dependency-order cleanup

The explicit transaction owner now joins its worker before callers may close
borrowed stores. LinksFacadeDisposer claims to release an entire facade graph,
but visits dependencies before disposing borrowers. A version cleanup may need
its transaction, and a background transaction may still use its data mapping.

- [x] Read complete disposer, ownership cycles/Root forwarding and existing tests.
- [x] Build a bounded failing regression for an outer facade that needs an open
   dependency during disposal. Prefer mocks first; then finite actual async
   checkpoint/mapped-store integration. Never release a worker to native code
   after its mapping has closed; restrict failing cleanup to a managed ownership
   check and always await actual work.
- [x] Establish cleanup dependency order without breaking duplicate/cycle handling,
   borrowed-vs-owned construction, nested disposer calls or retry semantics.
   Consider stopping worker/transaction borrowers in a separate first pass before
   native stores if simple traversal inversion cannot handle Root cycles.
- [x] Test failed/corrupt construction, version rollback, shared dependencies and
   idempotence. Run full Debug/Release, format and release metadata checks.
- [ ] Document ownership/evidence and verify all exact-SHA CI after an atomic commit.

Three initial regressions fail: mock borrowers receive a disposed dependency,
failed borrower cleanup closes its dependency before a retry, and graph traversal
closes mapped data while its asynchronous checkpoint is still running. The last
probe always throws after managed Flush before native applied-marker writes,
regardless of traversal timing; all gates and worker joins are finite. Five
initial controls pass after borrower-first cleanup and shared nested traversal.
Version rollback and constructor-failure controls additionally verify real owners.

Capture inner references before owner cleanup can detach them. Share a visited
set across synchronous nested calls on the same thread; always clear the root
scope on success or exception. A borrower failure stops dependency cleanup so
callers can retry. Direct transaction Dispose continues to borrow its stores;
the whole-graph helper explicitly releases every reachable facade.

## Windows CI follow-up

Commit `421dd39fa42576d35fc43373bad309d7fba1fc65` passes eight workflows,
but C# run `40134580109` fails the new integration test on Windows. Its worker
join reaches the five-second deadline (`ci-logs/csharp-facade-40134580109.log`,
lines 7587–7591). Linux and macOS pass all 903 tests. The stack identifies the
join deadline, not the worker's instruction at that instant.

The original probe called real mapped-file Flush after opening its release gate,
then deliberately aborted the checkpoint. That filesystem operation adds an
unrelated latency source to this ownership assertion. The updated probe checks
the managed disposal flag and always aborts before native checkpoint markers;
ordinary construction still uses real Flush. This removes that dependency
without increasing any deadline or changing production shutdown. Platform I/O
latency is an inference, not a measured worker stack. Reverify the changed probe
against the old traversal, repeat it finitely, and require fresh Windows CI.

The changed probe fails against `421dd39^` with the expected closed-dependency
assertion, then passes ten finite runs with borrower-first cleanup. Full logs
remain under experiments; committed excerpts and the exact CI failure are in
`dev/log/issues/138/pulls/139/facade-lifetime-ci.txt`. Fresh CI remains open.
