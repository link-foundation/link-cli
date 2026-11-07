---
bump: minor
---

### Added

- `Engine::backup_incremental_to` appends to an earlier backup only the revisions it lacks, each as the rows that changed, and JavaScript gets `WasmEngine.backupIncremental`. It refuses a destination whose last revision is no longer retained or that is not a copy of this engine, and leaves it unchanged.
