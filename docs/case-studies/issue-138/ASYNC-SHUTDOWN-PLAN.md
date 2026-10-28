# C# asynchronous worker ownership

Continue R1/R8 with a finite reproducer for disposal returning while borrowed
mapped stores remain in use.

- [x] Read the worker, flush, disposal and ownership paths.
- [x] Reproduce early return with one and two shutdown callers before fixing it.
- [x] Reproduce direct/descendant callback attempts to join their own worker.
- [x] Test synchronous callbacks, detached descendants and checkpoint recovery.
- [x] Join the worker before shutdown completion, serialize callers and reject self-join.
- [x] Verify focused tests, corrupt-construction disposal, full Debug/Release coverage, formatting and release metadata.
- [x] Document ownership and callback completion requirements; preserve evidence.
- [x] Verify fresh exact-SHA CI after publication (b37c4c5: all nine workflows pass).

The lifetime test holds a callback behind a fifteen-second finite gate and checks
shutdown after six seconds, beyond its existing five-second join. Its finally
always releases the gate and joins the actual worker before closing stores, even
when the public-contract assertion fails. Reflection is used solely for failing
test cleanup. Other gates and test deadlines are finite. Committed records whose
asynchronous checkpoint fails or is cancelled remain recoverable on reopen;
shutdown does not promise to drain cancelled queued checkpoint work.
