---
bump: patch
---

### Fixed

- Rollback, undo-image restore and recovery replay no longer delete a row that shares its source and target with another row. They now restore the recorded row through the new `LinksStorage::restore_link`, which skips duplicate resolution on direct stores. A layered `NamedLinksStorage` view keeps its own checks, so a duplicate below such a view still merges.
