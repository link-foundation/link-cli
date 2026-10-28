# Destructor-bearing mapped growth admission

R1/R6/R7/R8 continue after the empty/rejected-shrink correction. Plain-data
growth validates capacity before touching its old owner, but the generic
destructor-bearing path invalidates its checked barrier before upstream
rejects arithmetic/layout overflow. Zero growth also unnecessarily remaps;
an otherwise harmless callback panic leaves a valid view's barrier invalid.

- [x] Add finite public API regressions before changing production code:
      arithmetic overflow, layout overflow, zero-growth callback panic and a
      successful zero-growth control. Independently check callback admission,
      pointer/prefix, unchanged file length, flush, following growth and exact
      destructor counts. Impossible capacities must be rejected before any
      allocation or mapping; these are finite boundary checks, not stress.
- [x] Retain the actual failures against immutable 9bb985f: three fail and
      the ordinary zero-growth control passes; mapped-growth-admission-before.txt
      preserves the actual log lines and the isolated reproduction command.
- [x] Validate generic growth arithmetic/layout before retiring its descriptor,
      and invoke zero-growth callbacks directly against the existing prefix.
      Preserve the current callback/return contract and one typed owner.
- [x] Run both feature configurations, complete sequential native/actual-WASM
      suites, all local strict checks and complete coverage denominators.
- [x] Commit, push only the prepared branch and inspect fresh exact-head CI.

This does not repair genuine resize I/O failures, arbitrary nonzero callback
panic states, destructor panic during shrink, torn pages or hardware power
loss. Do not weaken existing public generic support to avoid those hazards.
All wider unmet R1-R12 criteria remain unfinished; PR139 stays draft/unmerged.

Complete validation finishes 2026-10-27T11:50:17Z: 1,194 native tests, six
doctests, 723 actual-WASM tests and 221 repository checks pass. The before/after
receipts preserve the actual failures, complete coverage totals and remaining
generic resize/panic hazards. All nine workflows pass on exact published
3cd89e9, including Rust Windows/Linux/macOS, full coverage, lint, merge and
packaging. growth-admission-ci-after.txt preserves actual platform counts,
complete log/result line references, run URLs and timestamps. Later logical
initialization and causal changes require their own fresh CI.
