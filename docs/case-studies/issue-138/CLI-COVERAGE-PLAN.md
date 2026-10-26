# Public command recovery and version names

Requirement mapping: R1/R3/R7/R8/R9/R12. The complete measured denominator
remains the target. Focused success or a higher percentage does not close R8.

- [x] Rank complete-run Rust/C# coverage without excluding production files.
- [x] Read the uncovered C# command paths and existing integration fixtures.
- [ ] Preserve finite before tests for any discovered behavior defect.
- [ ] Verify delimiter-containing version names through both native commands,
      exact listing, checkout, repeated reopening and following commits.
- [ ] Exercise real command dispatch in the coverage process, with serialized
      console capture, automatic restoration and exclusive-handle release checks.
- [ ] Check invalid configuration diagnostics, version errors, import/export
      failures, trigger removal and subsequent successful database commands.
- [ ] Compare complete Debug/Release and native coverage, preserving the full
      denominator and all remaining gaps. Run appropriate full local checks.
- [ ] Preserve atomic changes, update public examples/documentation and obtain
      fresh exact-head CI before treating the command batch as verified.

The initial complete C# Release mirror report leaves 108 uncovered command
lines. Native Rust also has CLI and transaction/recovery gaps. The audit script
in experiments/issue-138/coverage-audit.py distinguishes LLVM zero-hit region
starts from Cobertura wholly unhit lines; it never changes a report denominator.

Initial source tracing found both command parsers split --tag at the first
equals sign, while the version libraries preserve delimiter-containing names.
This is a candidate defect, not yet a reproduced or corrected result. Test the
public name=sequence grammar with a name containing equals and a numeric/tag
point after its final delimiter. Preserve ordinary tag syntax and errors.
In particular, an existing delimiter-containing tag must remain a valid point
in alias=existing-tag. Splitting only at the final delimiter would regress that
previously accepted command. Prefer the earliest delimiter whose complete suffix
resolves, retaining the original interpretation whenever it succeeds; test this
ambiguity and preserve the original unknown-point diagnostic when none resolves.

Existing subprocess tests remain necessary for actual executable integration.
Additional direct entry-point contracts should exercise distinct recovery and
configuration guarantees, rather than replace those process checks or merely
assert implementation branches. Test input and output must remain finite;
network rejection cases must avoid unbounded external connections.
