# Allocation failures during imports

Requirement mapping: R1/R6/R7/R8/R9/R12. All wider criteria stay active.

- [x] Trace text and binary import allocation separately from query planning.
      `resolve_index`, nested anonymous imports, `update_link` and the archive's
      first allocation pass still enter infallible named-links methods.
- [ ] Reproduce the faults before modifying those production call sites, using
      finite adapter faults and dedicated real-filesystem journals.
- [ ] Replace the infallible calls with the corresponding fallible bridge and
      preserve valid text/binary formats, names and public import signatures.
- [ ] Verify explicit, named and nested imports, pre-existing rows, both legacy
      owners, trigger composition, cleanup, retries and exact native reopening.
- [ ] Execute portable contracts in actual WASM. Verify C# exception controls
      without claiming an unobserved C# allocation-bridge defect.
- [ ] Keep every address small in pre-fix probes. Never exercise a potentially
      huge explicit allocation without finite process memory limits.
- [ ] Run required local checks, publish an atomic commit and inspect fresh CI.

This follows the independently verified fallible query/trigger bridge batch.
Import failures remain unfinished until their own reproductions and checks pass.
An error does not imply atomic undo of earlier imported rows or known durability;
the selected owner's cleanup and recovery protocol remains authoritative.
