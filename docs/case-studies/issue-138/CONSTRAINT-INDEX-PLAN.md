# Independently selected constraint validation

Requirement mapping: R1/R6/R7/R8/R9/R11/R12. Shared rows and changed-row
encoding still pay a full candidate scan for every selected constraint.

- [x] Add before-implementation portable/native and C# contracts for the new
      choice; preserve compile-failure evidence before adding the option.
- [x] Keep scan validation as the default. Add optional immutable indexes only
      for selected pair, name and reference constraints, independently of map,
      encoding, admission, serializable validation, history and physical log.
- [x] Validate a private index candidate before I/O. Remove every changed old
      value before adding new values, so atomic swaps work. Publish indexes only
      with their committed rows; group preparation keeps its own roots.
- [x] Verify bounded independent constraint oracles, absent/zero/self references,
      incoming references to deleted rows, duplicate/empty/Unicode names, swaps,
      rejected/no-op/following writes, groups, branches, replay and I/O outcomes.
- [x] Verify no indexes for default or unconstrained engines, per-constraint
      allocation, retained root sharing, and changed-row work using finite
      deterministic counters rather than timing thresholds.
- [x] Expose the choice in actual WASM and JS/TS; test invalid options and run
      real Node/Bun/Deno conformance and persistent examples.
- [x] Document algorithm, selected costs and limits; update case-study evidence
      and release notes. Run complete local checks and official coverage.
- [x] Commit/push the prepared branch, update PR 139 and verify fresh exact-SHA
      CI, preserving all remaining wider acceptance gaps.

The indexes are local validation structures, not distributed locks or a new
wire format. Recovery must validate every historical record under selected
constraints, and build indexes for the final recovered state. Cluster workspaces
must retain existing whole-shard validation and CAS semantics.
