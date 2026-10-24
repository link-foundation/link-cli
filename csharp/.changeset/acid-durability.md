---
'Foundation.Data.Doublets.Cli': minor
---

Reject complete corrupt journal entries and report replay failures. Add explicit
data/journal flush callbacks and checkpoint only after data reaches storage.

Add an optional staged links engine with configurable concurrency, isolation,
immutable revision retention, constraints and replaceable file/replicated logs.

Support independent journal branches and atomic three-way merge, with saved
merge bases for reopen/retry after uncertain I/O outcomes.
