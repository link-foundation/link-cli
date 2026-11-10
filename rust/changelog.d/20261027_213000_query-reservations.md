---
bump: minor
---

Reuse the standard query processor with a selected address type while retaining
the u32 facade. Add a checked full-u64 causal named/query adapter with a wider
wildcard marker, finite grammar/result/binding admission and lossless name output.

Reserve exact query allocations through the shared execution path on private
rows. Preserve owned identities across reference replanning and explicit ensures,
without staging allocation probes or publishing before explicit session commit.
