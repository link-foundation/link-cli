# Finite supervision under shared worker starvation

Objective: investigate the actual Linux C# failure on 89fbfad without widening
any idle or suite deadline. Preserve malformed-progress rejection, diagnostics,
actual exit codes, unconditional reaping and directory ownership.

- [x] Match run 40479397432 to commit 89fbfad and its timestamp. Preserve the
      complete log under experiments/issue-138/ci-logs. Lines 26217–26283 show
      the finite progress control timing out despite phase 5 and exit 0.
- [x] Trace every capture, timer, process-exit and cleanup continuation. The
      caller-context correction is present; shared worker admission remains.
- [x] Reproduce the observed symptom in an isolated process with a finite
      occupied worker pool. Bound child work, memory, waits and diagnostics;
      reap every child. Establish evidence before changing the helper.
- [x] Add a durable regression that leaves the test runner's pool untouched.
- [x] Correct the proven scheduling dependency, retain strict finite progress
      validation and add independent stalled/invalid-output controls.
- [x] Run all C# tests in Debug and Release, repository checks and coverage;
      preserve exact commands, denominators, before/after logs and limits.
- [ ] Publish the atomic correction and verify all fresh exact-head workflows.

The CI log does not establish the runner's precise scheduler state. Any isolated
control proves a helper dependency only. This work does not certify database
persistence or close the outstanding R1–R12 requirements.

The isolated linked-helper control first reproduces phase 5 / exit 0 /
TimeoutException against unchanged 89fbfad. Calling WaitForExit from the
control's main thread initially hid the race by synchronously raising the exit
event; the actual reproducer waits for a finite completion marker instead.
All three durable occupied-worker cases (progress, stall, repeated progress)
first fail because supervision requires that worker. The correction gives
process observation and each pipe its own dedicated finite task, checks idle
admission against a monotonic clock, and caps diagnostic capture at 64 KiB per
stream. All 17 focused controls pass, including a manually advanced clock and
caller-context deferral. No idle or suite bound was increased. Three dedicated
threads per active probe are a test-only resource cost, with every task joined
before the helper returns.

The complete Debug and Release suites each pass 2,451 tests with no failures or
skips. All 221 repository checks, formatting, declarations, dependency freshness
and workflow checks pass. The complete production coverage denominators remain
visible in dev/log/issues/138/pulls/139/phase-pool.txt; they remain below the
requested 100 percent. Fresh published-head CI is still required.

Published 950bcb3 passes eight workflows. Linux/macOS pass all 2,451 C# tests;
the actual Windows log also passes the original finite-progress case and all
three occupied-worker controls. Its one failure is a different compound setup
operation in VersionProcessExitTests. phase-pool-ci-after.txt preserves the
full SHA, timestamps, every workflow and actual error lines. Continue under
VERSION-SETUP-PROGRESS-PLAN.md without widening any idle/suite bound.
