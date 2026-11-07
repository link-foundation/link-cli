---
bump: minor
---

Add an optional native checkpoint journal factory owning the raw root and both
single or mirrored banks. Acquire all writer fences before recovery, attempt
every cleanup in reverse order, preserve all constructor/cleanup errors and
retain uncertain native fences without flushing pending work on close.
