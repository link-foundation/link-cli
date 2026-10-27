# Logical initialization for destructor-bearing mappings

Continue R1/R6/R7/R8 after generic growth admission. The wrapper captures the
original file length before upstream adds a minimum page. Plain-data growth
uses that logical length, but generic growth still forwards upstream's physical
padding count to callers. The safe filled helper consequently may skip values
that should be initialized. Establish the failure before changing that path.

- [x] Add three finite public regressions for fresh padding, an original
      sixteen-item prefix followed by growth across a page, and an independently
      observed raw callback count within a page. Use transparent u64 values with
      separate destructor counters: all bit patterns, including baseline zero
      padding, are valid. Do not persist pointer-bearing representations or
      panic inside an upstream initialization callback in the negative control.
- [x] Run them against immutable 9bb985f and preserve the actual failures.
- [x] Forward the known logical initialized-item count on the generic path;
      preserve the original file prefix, typed single ownership, callback API,
      following growth, checked flush and exact destructor retirement.
- [x] Run both feature configurations, complete native and actual-WASM suites,
      strict local checks and complete production coverage reports sequentially.
- [ ] Commit and push only the prepared branch, then inspect exact-head CI.

Complete sequential validation finishes 2026-10-27T12:19:05Z: 1,197 native
tests, six doctests, 723 actual-WASM tests and all 221 repository checks pass.
Both feature configurations verify all three new controls plus prior admission,
growth, flush and empty/shrink tests. Complete native LLVM reports
17,168/18,051 lines and 3,294/3,782 branches; complete linked WASM reports
13,144/15,760 lines and 2,663/3,171 branches. All production denominators remain
included. mapped-generic-initialization{,-before}.txt preserves actual failures,
commands, component metrics, exact destructor counts and remaining limits.

No additional allocation, copying, lock or file operation should be needed to
compute the count. Generic real resize failures, nonzero callback panic, failed
shrink/destructor panic, safe adoption of arbitrary persisted representations,
physical repair and hardware power-loss verification remain separate audits.
Every wider unmet criterion remains active; PR139 stays draft and unmerged.
