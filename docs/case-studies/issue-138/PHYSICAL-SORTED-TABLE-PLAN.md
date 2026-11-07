# Native physical sorted tables and LSM composition

Objective: address the physical sorted-table gap in R1/R5/R6/R7/R8/R9/R11/R12,
then continue WAL/memtable/manifest/compaction composition under the existing
atomic journal API. Every wider unmet criterion remains active in PR139.

- [x] Recheck RocksDB primary architecture, WAL and MANIFEST documentation;
      distinguish immutable physical tables from the existing in-memory runs.
- [x] Specify an independently versioned sorted-table record format over the
      existing native immutable doublets pages, with bounded complete validation,
      exact generations, tombstones and indexed physical point reads.
- [x] Run missing-API controls before production implementation, preserving
      their actual failures. Generate independent valid/corrupt binary vectors
      without importing the production codec.
- [x] Implement matching Rust/C#/JS tables, with Rust executing in actual WASM.
      Existing page/device APIs and optional SQLite paths stay available.
- [x] Verify bounded construction/failure schedules, immutable page identities,
      point I/O bounds, complete scans, corruption after open and source limits.
- [x] Demonstrate physical native-file interchange across Rust/C#/Node/Bun/Deno;
      retain before/after failures, resource bounds and runnable examples.
- [x] Run complete suites/coverage and relevant local CI checks; review the
      source, keep complete denominators and preserve the initial layout failure.
- [x] Commit the physical table feature as a useful atomic step.
- [x] Push only the prepared branch.
- [x] Verify fresh exact-head CI on `523888a`, including every native platform:
      all nine workflows and six Rust/C# platform jobs pass. Evidence:
      `dev/log/issues/138/pulls/139/sorted-table-ci.txt`.
- [ ] Compose physical tables with independently selected WAL, bounded memtable,
      manifest publication, recovery, compaction and obsolete-file retirement.
      A table alone does not complete the on-disk LSM requirement.

Design: a sealed table uses one native-page flush containing a version marker
and strictly ASCII-key-sorted records. Each record carries a positive canonical
u64 modification generation and either owned string records or a tombstone.
The existing page hashes and database identity bind every physical page.
Open validates the complete bounded file before exposing a key/record-address
index and page hashes; payloads are discarded after validation. Point lookup
binary-searches this index and reads only the selected record's doublets pages,
checking them against their validated hashes. Complete scans return owned values.
Caller-owned devices remain exclusive and must not change during use. Native
files use the existing SQL-free file page device. No block-cache or SQL cost is
introduced to users who do not select this table.

The chosen initial format trades full bounded startup validation and a key/page
index for avoiding a retained full payload map. Complete scans and compaction
still incur physical I/O. There is no current claim of background compaction,
Bloom filters, arbitrary database size or physical power-loss certification.
