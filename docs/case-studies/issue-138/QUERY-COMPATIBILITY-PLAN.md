# Public query compatibility and unchanged state

Requirement mapping: R1/R7/R8/R9/R12. Whole-project coverage and all wider
inventory items remain active throughout this audit.

- [x] Rank uncovered complete-run native/C# query paths and read the existing
      basic, mixed and advanced public processors and fixtures.
- [x] Install minimum public API contracts before correcting any behavior.
- [x] Compare empty, leaf and incomplete query shapes across the three retained
      C# processors; preserve exact rows, names, notifications and following edits.
- [x] Reopen the same native files after each no-op shape and verify exact state.
- [x] If a behavior defect reproduces, correct its root cause while retaining
      the established accepted-query semantics and diagnostic policy.
- [x] Run focused and complete native checks with unchanged coverage denominators,
      format/release/source checks, preserve evidence and commit an atomic batch.
- [x] Obtain fresh exact-head CI and continue every wider unfinished criterion.

Source tracing finds a candidate null-list hazard in the legacy mixed processor:
`outerLinkValues?.Count < 2` does not reject a null child list, but the following
indexing assumes one exists. The basic processor explicitly returns for null and
the advanced processor checks null before its child count. A candidate is not
yet a verified bug; use leaf-shaped public input before changing production.

The first finite run passes all leaf controls, including the mixed processor.
The installed maintained parser supplies empty child lists for these inputs,
so no null-list crash is reproduced and no production correction is justified.
One proposed comment control used unsupported `//` instead of the established
`#` syntax and observed a read notification. Correct that test input; do not
change accepted query/notification semantics to satisfy an incorrect oracle.

All 33 focused contracts pass. Both complete C# configurations pass 2,124 tests
with no failures or skips. Debug covers 7,938/8,259 lines and 4,259/4,632 branches;
Release covers 6,094/6,296 lines and 4,214/4,605 branches. No production source
changed, and no measured file or denominator was removed. Argument rejection,
mixed wildcard errors, existing-pair reuse and basic update-count diagnostics
also preserve exact rows, names and subsequent successful edits. The no-op
shape matrix additionally reopens the real native files and checks another edit.
See query-compatibility.txt for the complete report paths and reproducible run.
All nine workflows pass on 580dcb2, including the native platform matrices;
query-compatibility-ci.txt preserves the exact SHA, creation timestamps and links.
