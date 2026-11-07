---
bump: minor
---

Publish retained journal checkpoints through optional replacement logs while
holding the engine commit mutex. Preserve live sessions, history, branch bases
and committed allocator positions; fence uncertain outcomes until reopening.
Compose physical delta compression and expose compaction through WASM bindings.
