---
'Foundation.Data.Doublets.Cli': patch
---

Hardened the C# build and the pipelines around it (issue #96).
`Directory.Build.props` now turns warnings into errors and enables the
.NET analyzers, and `TransactionsDecorator` / `VersionControlDecorator`
implement `IDisposable` so the memory-mapped databases they own are
released deterministically — the leak that made the Windows test job
fail while the pipeline still reported success. The C# workflow no
longer masks those Windows failures with `continue-on-error`, verifies
formatting and file sizes, and finally implements the `changeset-pr`
release mode it had been advertising without handling.
