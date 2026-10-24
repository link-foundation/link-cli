---
bump: patch
---

Preserve quoted and encoded query references as names throughout validation,
creation, matching and mutation, including numeric names such as "0" and "1".
Bare numbers remain link addresses. Applies to the CLI, library and WASM wrapper.
Keep quoted names intact in stored triggers, and quote numeric names in LiNo
output so exported names round-trip through queries.
