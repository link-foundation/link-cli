---
bump: patch
---

Routed the CLI's `LinkStorage` through the upstream `doublets` uniqueness and cascade resolvers, so a delete now removes the links that referenced the deleted one and an update that would duplicate an existing link merges into it, matching the C# CLI. The transactions log records one transition per link a write actually touched — including the ones a cascade touched — so rollback and version-control branch switching no longer lose the cascaded changes, and the query processor restores links that a resolved write deleted as a side effect, mirroring `RestoreUnexpectedLinkDeletions` in the C# processor.
