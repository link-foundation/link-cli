# Unicode sequence graph integrity

Requirement mapping: R1/R7/R8/R12. A bounded one-node native probe did not
terminate normally; it was stopped under a two-second CPU/256 MiB address-space
limit. Both published baseline walkers lack active-path cycle detection. Keep this
audit separate from the published optional object cache.

- [x] Trace Rust and the exact installed C# converter/walker package source.
- [x] Preserve bounded automated failures before changing production code.
- [x] Add iterative active-path validation that rejects cycles while retaining
      repeated valid shared subtrees and existing UTF-16 behavior.
- [x] Preserve Rust's public Vec-returning walker and add a checked Result path;
      propagate decoding errors through both strings and raw code units.
- [x] Reuse the C# package converter through its existing walker interface.
- [x] Test both cycle directions, self/two-node cycles, repeated failures,
      healthy following reads, missing nodes, valid sharing, Unicode order,
      deep finite sequences, names and persistent reopening.
- [x] Execute portable contracts in actual WASM and verify public boundaries.
- [x] Run complete local checks/coverage and document costs and limits.
- [ ] Publish an atomic step and verify fresh exact-head CI. Keep wider criteria open.

Before-fix executions use finite children, memory/CPU limits and parent deadlines.
The active path must be local to each traversal; a global visited set would
reject valid DAG reuse. This audit does not promise an output-size limit or
change existing converter cache or UTF-16 compatibility policies.
