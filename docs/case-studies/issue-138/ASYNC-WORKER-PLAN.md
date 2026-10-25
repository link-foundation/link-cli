# Async worker scheduling audit

Requirement mapping: R1/R6/R8. C# run 40144628358 on
`1c1ef38b321877d7bc0021944ea729efb03c7e07` failed two Windows waits:
`AsyncShutdownTests.EveryShutdownCallerWaitsForInFlightStorageUse(callers: 1)`
(TimeoutException at line 61) and
`TransactionsDecoratorTests.AsyncCommitEventuallyMarksApplied` (expected applied
2, actual 0). Complete downloaded logs are retained locally; relevant trace
lines are 8490–8503 and 9872–9896. Eight other workflows passed on that SHA.

The current transaction constructor schedules a synchronous blocking consumer on
the shared ThreadPool for every owner, including idle synchronous owners. Async
commits then enqueue another Task.Run from that consumer and block on it. These
workers can occupy the pool while checkpoint tasks and test continuations wait.
This is a hypothesis to establish with a finite isolated-process reproduction,
not a reason to increase test timeouts.

- [ ] Reproduce idle-owner starvation in a child with a two-worker pool and four
      tiny stores; bound waits and join all workers before releasing mappings.
- [ ] Use an asynchronous queue wait, with ordered single-consumer checkpoint
      execution and no extra pool dispatch per queued batch.
- [ ] Preserve cancellation, in-flight joining, repeated/concurrent shutdown,
      callback self-join rejection and failed-checkpoint recovery.
- [ ] Retain a portable automated child regression in the ordinary test suite.
- [ ] Run full C# validation and exact-head Windows/Linux/macOS CI; preserve
      failed logs and specific before/after evidence.
