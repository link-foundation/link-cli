---
bump: minor
---

### Added

- `Engine::compact_journal_after` compacts once a selected number of commits have landed since the last compaction, and the optional `BackgroundCompaction` calls it periodically on its own thread and reports the first failure. The WebAssembly engine exposes `compactJournalAfter`.
