---
'Foundation.Data.Doublets.Cli': patch
---

`clink --transactions` writes no longer slow down as history grows (issue #195). The CLI journal is now an append-only `<db>.transitions.log` file (`FileTransitionLog`): a write appends its transitions and markers instead of rewriting the whole transitions store. A bounded `--retention` policy also compacts the journal on disk, so the file stays bounded. An existing `<db>.transitions.links` journal is imported once. CI runs a benchmark that fails when write N+100 is much slower than write N on a constant-size store.
