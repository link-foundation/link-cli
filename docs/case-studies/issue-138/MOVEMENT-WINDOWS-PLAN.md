# Windows movement child deadline investigation

Requirement mapping: R1/R3/R7/R8/R12. Preserve the complete issue inventory.

- [x] Check the failed run's SHA and creation time; download its complete log.
- [x] Read the actual failure and the complete child/parent/fault implementation.
- [x] Preserve the exact error and line numbers in mapped-growth-ci-before.txt.
- [x] Retain child stdout/stderr in a file and add elapsed setup/metadata progress.
      Keep the existing ten-second deadline and kill/join behavior.
- [x] Run focused tests, complete native tests, strict lint and repository checks.
- [ ] Obtain exact-head platform diagnostics; establish the root cause before
      claiming a fix. Do not weaken crash/recovery assertions or extend deadlines.
- [x] Verify the diagnostic change in fresh CI; all nine workflows pass on
      3c22e69, including all native platform matrices. See movement-diagnostics-ci.txt.
      Continue all wider criteria; the earlier timeout's cause remains unknown.

Run 40299771183 at 1aafc31 fails only Windows's
switch_survives_exit_at_every_persisted_movement_phase. Lines 32629–32635
report "Finite movement child did not stop." The parent suppresses stdout,
waits without draining stderr and discards stderr on its deadline path. The
requested point and setup progress therefore cannot be recovered from this
failure. Unread-pipe backpressure and slow durable setup are hypotheses, not
established causes. The other two movement parents pass in that run.

The diagnostic change sends both streams to a per-child file and reports its
contents on deadline or unsuccessful exit. Elapsed markers bracket existing
setup operations; the existing version trace is enabled only in this test
child, and the metadata callback reports each checked phase. This makes a
future failure actionable without adding default production output. The input
and deadline remain finite. No database behavior correction is claimed.

The unchanged crash/recovery schedule passes locally: three parents exercise
all child exit points, then the full native suite passes. Journal-only head
e39ecce also passes all nine workflows, including Windows, before this
instrumentation. The original timeout is not reproducible here; no database
fix or definite pipe/setup cause is claimed. All nine workflows also pass on
diagnostic head 3c22e69. A recurrence must be investigated using the retained phase.
