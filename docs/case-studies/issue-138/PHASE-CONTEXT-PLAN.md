# Child-supervision synchronization context

The exact c1abe42 C# run 40467581526, created 2026-10-27T07:30:58Z,
fails only on Windows. Its log at lines 29273–29327 reports a cancelled
`DistinctFiniteProgressSurvivesAnAggregateDeadline` despite five consecutive
completed phases and child exit 0. The complete log is preserved locally at
`experiments/issue-138/ci-logs/csharp-40467581526.log`. No filesystem-latency cause is established.

- [x] Verify the run timestamp and SHA, download the complete failing log and
      inspect the specific exception, phase timings, actual exit and stack.
- [x] Trace capture, idle cancellation, process exit, reaping and cleanup.
- [x] Reproduce with an isolated finite child under a deliberately deferred
      caller synchronization context before changing the helper.
- [x] Correct the proven dependency without extending the idle or test bounds;
      keep strict phase validation and unconditional child reaping.
- [x] Add a durable automated regression, rerun all C# tests in both
      configurations and the actual process controls, and preserve evidence.
- [x] Publish the correction and inspect fresh Windows and other exact-head CI.

This concerns test-process supervision. It does not strengthen production
persistence guarantees or finish the wider R1–R12 inventory.

The actual linked-helper probe and durable regression both fail first with
phase5/exit0. `ConfigureAwait(false)` on the capture/process awaits makes the
probe pass with zero caller-context callbacks and all thirteen focused process
controls pass. This proves the helper dependency; it does not directly observe
the Windows runner's scheduling. `phase-context-before.txt` preserves exact
commands, bounded inputs and log lines. All 2,447 C# tests pass in each of
Debug and Release; the repository's 221 checks, type checks, source limits,
changeset, 126 dependency declarations and workflow lint pass. The initial
repository-layout check rejects root-level `ci-logs`; moving the ignored logs
under `experiments/issue-138/ci-logs` resolves it without changing the check.
`phase-context.txt` records the local after evidence. All nine workflows pass
on exact published head 7e9799a. C# passes 2,447 tests on Windows/Linux/macOS;
both finite progress controls pass on Windows. `phase-context-ci-after.txt`
preserves commit/run timestamps, run links and the successful log line numbers.
Subsequent source changes still require fresh exact-head CI.
