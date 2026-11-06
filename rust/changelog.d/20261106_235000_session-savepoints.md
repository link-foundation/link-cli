---
bump: minor
---

### Added

- Nested savepoints on engine sessions and cluster shards with `savepoint`,
  `rollback_to_savepoint` and `release_savepoint`. Rolling back restores staged
  rows only; reads stay validated, point-write ownership stays and allocated
  addresses stay retired. The WebAssembly session exposes them as `savepoint`,
  `rollbackTo` and `releaseSavepoint`.
