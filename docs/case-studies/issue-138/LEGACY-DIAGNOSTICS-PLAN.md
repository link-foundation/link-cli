# Diagnostic failures through existing decorators

Continue the ACID fault audit after `adb93d0`. Optional diagnostics in existing
storage/transaction/version layers use the same infallible console primitives.
The staged-engine fix does not establish their behavior.

- [x] Reproduce Rust raw-store, named/query, transaction and version diagnostics
  failing against a bounded Linux `/dev/full` stderr destination.
- [x] Reproduce C# transaction, version and persistent-trigger operations failing
  through a throwing console writer; isolate global console replacement from
  parallel tests and always restore the previous writer.
- [x] Keep diagnostic failures separate from journal, checkpoint and user output
  failures; preserve the existing diagnostic destination and opt-in switches.
- [x] Verify commits, rollbacks, retention and subsequent operations still work;
  reopen persistent state where the operation guarantees persistence.
- [x] Reproduce and correct retention replaying rolled-back writes, in both
  languages, including create/update/delete, following commits and archive data.
- [x] Reproduce C# background retention marking pending changes applied for both
  policies; wait for commit/rollback before replay or reclamation.
- [x] Verify the remaining C# named-store constructors/operations, query writes
  and server request/reply traces against unavailable diagnostic destinations.
- [x] Verify a real Rust TCP request and subsequent read while stderr fails.
- [x] Run focused regressions, complete native suites, actual WASM, formatting
  and coverage.
- [x] Publish and inspect exact-SHA CI.

Exact-SHA CI for `e652c2c` completes with 35 successful checks, 11 skipped,
one neutral and one Windows Rust failure. The failed job is an existing etcd
socket fixture, not a diagnostic/retention regression. The complete workflow
and failed-job logs are downloaded; the specific errors and portable fixture
corrections are preserved in `dev/log/issues/138/pulls/139/windows-peer.txt`.
Fresh Windows verification of that correction remains required.

All inputs, subprocess counts and stored rows are finite. A diagnostic fault
does not simulate storage failure or power loss. Normal query result output
retains its existing semantics.

The Rust stdout regression uses a custom test harness: the standard harness
itself refuses to start tests when stdout returns ENOSPC. Its child emits no
framework output before calling the persistent-trigger API. C# uses a collection
with parallelization disabled and a throwing writer scoped by `try/finally`.
Additional C# cases cover both named-store variants, the simple-store constructor,
three query mutations, and both server trace boundaries. The server writer allows
zero or one successful diagnostic writes, so failure is tested before and after
the request mutates state. Standard query output retains its existing behavior.

Removing the diagnostic interruption exposed retention replaying rolled-back
creates. Independent native regressions also reproduce changed updates and
deletes for sized and chunked retention. Rollback records are now reclaimable
without replay; committed archives omit those aborted writes. The journal's
durable rollback markers remain intact for reopening.

The C# pending-transaction cases inject a second explicit transaction through
the older asynchronous commit's data-flush callback. Both policies previously
advanced applied markers into the pending transaction. A bounded event and
worker shutdown establish completion without sleeps. The fixed policy leaves
pending records in the live log and unmarked; rollback then restores the single
committed row. The experiment temporarily removes only the two pending guards,
reproduces both failures, and restores the guards in `finally`.

The Rust server child uses a real TCP connection with finite read/write deadlines
and explicitly joins its worker before checking replies. Restoring only the old
server trace calls reproduces exit 101; the fixed child creates a row and reads
it back while stderr continues to return ENOSPC.
