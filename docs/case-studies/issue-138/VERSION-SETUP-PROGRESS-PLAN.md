# Versioning child setup progress

Continue the actual exact-head CI investigation without widening idle or suite
bounds. Published 950bcb3's Linux/macOS C# suites pass all 2,451 tests, including
the occupied-pool correction. Windows passes 2,450 and fails one versioning case.

- [x] Preserve complete run 40486274057 logs, SHA and timestamps. Its failure at
      lines 25689–25702 reports name-exit/success/True, phase 6 version-open,
      child elapsed 19.8139 seconds; version-open was emitted at 4,295 ms.
      The child is killed/reaped at the unchanged 15-second idle boundary.
- [x] Trace the next operation: CreateAndUpdate combines two automatic version
      transactions, with multiple data/journal/branch flushes in each. It reports
      no completed phase between creation and update. The log cannot identify
      which inner write or flush was slow, nor establish a storage deadlock.
- [x] Add a regression that observes real Create/Update call boundaries and
      requires progress after creation returns, before update starts. Retain
      its failure against the unchanged combined implementation.
- [x] Report returned creation through a borrowing forwarding decorator, before
      the combined helper starts its update. Keep its existing callbacks and
      duplicate-row return/effects, actual data, automatic transaction ownership,
      later crash points, idle limits and the finite 64-phase cap unchanged.
- [x] Run all 31 focused supervision/versioning controls and complete sequential
      C# Debug/Release suites: 2,462 tests each, zero failed/skipped. Full
      production coverage denominators are unchanged; all 221 repository and
      strict checks pass at 2026-10-27T11:22:31Z. version-setup.txt retains
      exact metrics and commands.
- [ ] Inspect fresh exact-head platform CI after publication.

This is test-process visibility, not a persistence guarantee. The new boundary
will also distinguish a slow Create from a slow Update if the platform still
fails. Do not claim the exact host scheduling or physical I/O cause is known.
All wider unmet R1-R12 criteria remain required; PR139 stays draft/unmerged.

Fresh dec6cb7 C# run 40491222901 passes all 2,460 tests on Windows/Linux/macOS
before this progress change. The original Windows timeout is therefore
intermittent; successful CI does not establish the missing boundary's cause.
The new call-boundary regression independently fails against the original
combined setup. Its duplicate-row control must use the unchanged public helper
on a separate real store, without assuming which row uniqueness preserves or
that the old helper always returns a still-existing address. Keep that behavior
through the existing decorator seam and enable the child's existing tracing.

All nine workflows now pass for prior published dec6cb7, including all three
native Rust/C# platforms. mapped-grow-ci-after.txt preserves exact SHA, commit
and run timestamps and the successful per-platform totals. This validates
retained plain-data growth and discovery controls, before the later empty
shrink/setup-progress changes; those changes require new exact-head CI.
