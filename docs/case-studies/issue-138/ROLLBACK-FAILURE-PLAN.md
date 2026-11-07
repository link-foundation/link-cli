# Failed manual rollback audit

Continue R1 with ordinary explicit transactions whose journal writes succeed,
then inject a single error during rollback. A rollback has already chosen abort;
it must not allow committing a partially restored state or adding more writes.

- [x] Reproduce unsafe Rust commit/write admission after rollback failure.
- [x] Reproduce the same boundaries in C# (all 16 cases fail before the fix).
- [x] Fence the transaction before undo or rollback persistence can fail.
- [x] Verify retry restores the exact baseline and survives reopening.
- [x] Run full native suites, coverage, formatting and actual WASM checks.
- [x] Publish and inspect exact-SHA CI; update PR evidence.

The fixture starts with two rows, updates both in one explicit transaction,
and injects one failure before or after undo, data flush, rollback marker append,
or journal flush. Commit and write rejection are separate tests so one cannot
mask the other. Every successful case retries rollback, checks admission, commits
a following write and reopens the data and journal. No process crashes, memory
stress or unbounded inputs are involved.

Commit `38eb3fb4cfa7e6c6e06b53159e1268d5115609c5` passes all CI checks: 37
successful, 10 skipped and one neutral. Runs created 2026-10-24 20:02:53 UTC
match that SHA. Rust run 40128031801 and C# run 40128031799 pass Windows, Linux
and macOS; WASM run 40128031827 passes both coverage and integration jobs.
