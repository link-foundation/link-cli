# Compiled causal branch coverage

Continue R8 after the owned projection step. Keep every whole-project
denominator and every wider unmet requirement active.

- [x] Inspect the actual Release Coverlet records, including generated methods
      without source lines. Preserve the pre-change report and its hash.
      CausalBatch.ParseEdits has one unobserved edge: a scalar individual edit.
      The original aggregate is 187/188 compiled branches, while all reported
      source-line branches are covered.
- [x] Add a raw branch assertion before changing the shared input vectors.
      Preserve its failure at generated lambda IL offset 8, path 1, line 61.
      Inspect the pinned Coverlet reporter: class totals include all methods,
      while the source listing omits methods with no source lines.
- [x] Add an independently generated scalar-edit rejection case. Preserve all
      existing vectors, producing 54 cases. Do not change production code or
      relax a coverage threshold.
- [x] Reproduce missing raw-report retention with the existing workflow-policy
      contract before changing CI. Collect JSON and Cobertura together and
      retain both reports on Linux, macOS and Windows.
- [x] Run complete C# Debug and Release suites with both raw and XML collectors;
      assert every compiled branch of all five causal components. Preserve
      complete project counts and report byte counts/SHA256.
- [x] Run shared-vector contracts in native Rust, actual WASM, Node, Bun and
      Deno; retain complete Node coverage, repository and workflow checks.
- [ ] Review the precise test/CI changes, commit and push only the prepared
      branch. Inspect fresh exact-head checks and actual platform archives.

Reproduce with `bash experiments/issue-138/validate-causal-raw-coverage.sh`.
Raw branch completeness for these five files does not establish complete
project coverage or complete issue 138. PR139 remains draft and unmerged.

The reporter implementation is pinned at
[Coverlet 10.1.0 CoberturaReporter](https://github.com/coverlet-coverage/coverlet/blob/v10.1.0/src/coverlet.core/Reporters/CoberturaReporter.cs#L74).
The actual raw records provide the before/after evidence; the reporter source
explains why a source-line sum missed the edge.
