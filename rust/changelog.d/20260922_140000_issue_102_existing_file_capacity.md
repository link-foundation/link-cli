---
bump: patch
---

Added `PersistentFileMapped::open_existing`, a safe constructor that adopts
the full element capacity represented by an existing file instead of starting
with a logical capacity of zero. This lets embedding applications reopen a
persisted `doublets::unit::Store` without an `unsafe` `grow_assumed` call.
