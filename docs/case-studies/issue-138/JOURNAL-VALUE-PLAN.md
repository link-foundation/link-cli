# Journal value ownership and codec contracts

Requirement mapping: R1/R3/R7/R8/R12. Keep complete-project coverage and every
wider unfinished criterion active.

- [x] Read complete journal/transition definitions and shared name-wire vectors.
- [x] Verify captured names survive caller collection changes and expose no
      mutable list; verify equality/hash collections and complete header getters.
- [x] Check the complete UTF-16 unit domain and malformed four-digit name units
      through the public codec, preserving narrow/wide and legacy compatibility.
- [x] Preserve production validation: no defect reproduced and no speculative
      simplification is needed.
- [x] Run focused and full Debug/Release coverage with all production files,
      formatting/release/source checks and repository tests; retain evidence.
- [ ] Obtain fresh exact-head CI, review source and continue the wider inventory.

JournalRecord copies the caller's enumeration into a read-only list. Its public
equality includes the complete transition, name-only marker and ordered names.
Unequal values need not have different hash codes. Codec timestamps represent
milliseconds; tests must not invent submillisecond round-trip guarantees.

All 17 focused controls pass before any production change, including a finite
65,536-unit name. No ownership/equality/codec defect reproduced, so production
code remains unchanged. This batch preserves semantic tests and their measured
effect; it does not change codec validation merely to remove coverage branches.

Both complete configurations pass 2,143 tests with no failures/skips. Complete
coverage measures 96.25% lines / 92.02% branches in Debug and 96.92% / 91.59%
in Release. JournalRecord.cs reaches 83/84 and 67/68 unique lines respectively.
See journal-values.txt; production denominators remain unchanged. Fresh CI and
all wider acceptance criteria remain active.
