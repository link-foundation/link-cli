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
- [ ] Add a regression that observes real Create/Update call boundaries and
      requires progress after creation returns, before update starts. Retain
      its failure against the unchanged combined implementation.
- [ ] Spell out the same two setup operations with a completed-creation phase
      between them. Keep actual data, automatic transaction ownership, later
      crash points, idle limits and the finite 64-phase cap unchanged.
- [ ] Run focused supervision/versioning controls and complete sequential C#
      suites; retain full coverage denominators and inspect fresh platform CI.

This is test-process visibility, not a persistence guarantee. The new boundary
will also distinguish a slow Create from a slow Update if the platform still
fails. Do not claim the exact host scheduling or physical I/O cause is known.
All wider unmet R1-R12 criteria remain required; PR139 stays draft/unmerged.
