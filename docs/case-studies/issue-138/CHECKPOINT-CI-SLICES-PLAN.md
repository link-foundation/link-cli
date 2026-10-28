# Windows checkpoint process-exit matrix timeout

Objective: fix the exact-head C# Windows failure on 9a9a95c without increasing
its deadlines or omitting any native durability cut point.

- [x] List fresh runs, verify timestamps and head SHA, preserve complete C# log.
- [x] Read the failure context: run40416245668 log26794-26810 reports xUnit's
      120000ms case deadline for mirrored banks+root replacement. All eight
      other workflows and both other C# native platforms pass.
- [x] Inspect the finite probe. One case starts a process for every observed
      before/after write/truncate/sync cut and performs multiple synchronized
      reopen/following-write/replacement controls per cut. Accumulated process
      and durable-file costs under runner contention are a plausible cause of
      the whole-case timeout; the log alone does not establish the source of
      slowness. It reports no child timeout or persistence assertion mismatch.
- [x] Divide each of the nine configurations into four disjoint ordinal slices.
      Keep the 120s case and 10s child deadlines, all setup/recovery controls,
      child tree cleanup, ownership fences and every original cut point.
- [x] Run all 36 real process cases and check the emitted trace counts/union:
      all 258 original cutpoints occur exactly once across the four slices.
- [x] Run complete C# Debug/Release suites, formatting and repository checks.
      The combined workspace includes the separately implemented table feature:
      2,349 tests per configuration and all 221 repository checks pass.
- [x] Commit this useful test correction independently.
- [x] Push only the prepared branch.
- [x] Verify fresh exact-head native Windows/macOS/Linux CI on `523888a`;
      all nine workflows and all six Rust/C# platform jobs pass. Evidence:
      `dev/log/issues/138/pulls/139/sorted-table-ci.txt`.

Preserved full log: `experiments/issue-138/ci-logs/csharp-40416245668.log`. A public concise extract
and validation report will accompany the test correction. This source change
does not presume or claim a production persistence bug. Physical power-loss
certification and all wider unmet requirements remain open.
