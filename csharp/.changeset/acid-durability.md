---
'Foundation.Data.Doublets.Cli': patch
---

Reject complete corrupt journal entries and report replay failures. Add explicit
data/journal flush callbacks and checkpoint only after data reaches storage.
