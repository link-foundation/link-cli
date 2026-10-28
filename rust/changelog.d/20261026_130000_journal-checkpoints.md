---
bump: minor
---

Export portable retained-history checkpoints and optionally publish them through
coordinated generation CAS without rewriting local append-only journals.
Compose the initial checkpoint with optional physical delta encoding and either
replication order, preserving exact replay and uncertain-operation fencing.
