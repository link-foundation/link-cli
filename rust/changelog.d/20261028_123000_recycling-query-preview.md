---
bump: patch
---

- Validate legacy recycling queries on a detached copy of the actual allocator,
  including named allocations and duplicate merges. Reject missing references
  before writing data, names or journals, and keep transaction/history behavior.
