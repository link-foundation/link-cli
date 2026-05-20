---
'Foundation.Data.Doublets.Cli': minor
---

Added optional transactions and version-control layers (issue #94). The
new `TransactionsDecorator` records each Create/Update/Delete as a
reversible transition in a sidecar doublets store and exposes
`BeginTransaction()` / `Commit()` / `Rollback()` plus three retention
policies (`infinite`, `sized:<n>`, `chunked:<n>:<dir>`) and two commit
modes (`sync`, `async`). The new `VersionControlDecorator` adds
branching, tagging, and time-travel checkout over that log. The CLI
surfaces both layers through `--transactions`, `--transactions-file`,
`--commit-mode`, `--retention`, `--log`, `--vc`, `--vc-file`,
`--branch`, `--branch-from`, `--checkout`, `--tag`, `--list-branches`,
and `--list-tags`. When no flag is passed, behaviour is byte-identical
to the existing CLI — no sidecar is written and no extra cost is paid.
