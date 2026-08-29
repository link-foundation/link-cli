---
bump: patch
---

Fixed `--changes` reporting in the Rust CLI: an auto-created reference is now reported as an update of the placeholder it started from, a delete reports the whole cascade of removed usages, and the reported changes are emitted in a reproducible order instead of one derived from the hash seed of the process.
