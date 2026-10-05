---
'Foundation.Data.Doublets.Cli': patch
---

Fixed the `--changes` report of creations:

- Every link a query creates is reported, named point links and leaves created on the way included. Each one is reported once, as `() ((name: name name))`, not as an empty link filled in later.
- Creating a link under an existing name redefines that link in place, and a new name for an existing doublet names it instead of copying it.
- Reference validation predicts the addresses new links get by asking the store. Stores reuse the address freed last first, so the lowest free address is not always next. A reference to a freed address the query does not refill is now reported missing instead of being dropped.
- `EnsureCreated` no longer throws when the store hands out a freed address above the target before reaching it.
