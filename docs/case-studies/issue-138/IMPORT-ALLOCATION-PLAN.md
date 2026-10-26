# Allocation failures during imports

Requirement mapping: R1/R6/R7/R8/R9/R12. All wider criteria stay active.

- [x] Trace text and binary import allocation separately from query planning.
      `resolve_index`, nested anonymous imports, `update_link` and the archive's
      first allocation pass entered infallible named-links methods before the fix.
- [x] Reproduce the faults before modifying those production call sites, using
      finite adapter faults and dedicated real-filesystem journals.
- [x] Replace the infallible calls with the corresponding fallible bridge and
      preserve valid text/binary formats, names and public import signatures.
- [x] Verify explicit, named and nested imports, pre-existing rows, both legacy
      owners, trigger composition, cleanup, retries and exact native reopening.
- [x] Execute portable contracts in actual WASM. Verify C# exception controls
      without claiming an unobserved C# allocation-bridge defect.
- [x] Keep every address small in pre-fix probes. Never exercise a potentially
      huge explicit allocation without finite process memory limits.
- [x] Run required local checks and retain complete coverage denominators.
- [ ] Publish an atomic commit and inspect fresh CI at its exact SHA.

Before production changes, six real-filesystem cases panic, four portable cases
ignore faults and one further portable case ignores reallocation failure after
a once trigger deletes the reserved row. Two native and one portable named
import controls pass before the fix. Complete original logs are preserved in
`dev/log/issues/138/pulls/139/import-allocation-before.txt` at baseline c530a6a.
All 28 focused Rust composition cases pass. Complete local suites pass: 963 native
plus six doctests, 556 actual-WASM, 2,172 C# per Debug/Release configuration,
209 JS and 221 repository checks. All 48 physical/policy configurations pass
288 cross-language writers with mirrored repair. See import-allocation.txt for
commands and complete official coverage. Fresh exact-head CI follows publication.

This follows the independently verified fallible query/trigger bridge batch.
Local import checks pass; fresh CI is required after publication.
An error does not imply atomic undo of earlier imported rows or known durability;
the selected owner's cleanup and recovery protocol remains authoritative.
