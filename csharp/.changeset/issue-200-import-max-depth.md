---
'Foundation.Data.Doublets.Cli': minor
---

Added `--in-max-depth <N>` (1 to 128, default 64) to set the nesting limit of
LiNo imports through `--in`, `--import` and `--lino-input`, and
`LinoDatabaseInput.ImportText`/`ReadFromFile` overloads that take `maxDepth`.
LiNo import now accepts nested pairs as link references, as the Rust and
JavaScript implementations do. The 64-level default is now documented (#200).
