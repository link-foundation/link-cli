---
bump: minor
---

Add optional `IndexedLinksDecorator` lookup indexes, composed through
`LinkStorage::with_indexed_lookups`, `NamedTypesDecorator::with_indexed_lookups`
and the Rust CLI's `--indexed-lookups` flag. Named LiNo imports and exports use
persistent exact-pair/source/target indexes instead of repeated full-store
scans when enabled. Keep the original scan algorithms as the default for
comparison. Preserve Unicode names, duplicate resolution, cascading deletion,
address reuse and persisted readback. Add bounded scaling and parity tests.
