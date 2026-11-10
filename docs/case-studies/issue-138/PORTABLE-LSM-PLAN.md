# Portable physical journal store

Objective: promote the isolated physical composition into independently selected
production Rust, C# and JavaScript stores. This step contributes to R1, R5, R6,
R7, R8, R9, R11 and R12. It does not complete native directory ownership,
physical process-exit interchange, whole-project coverage or the wider issue.

- [x] Preserve missing-API failures against codec-only d12760f in all three
      languages and retain the independent publication oracle and prototype.
- [x] Implement public PhysicalLsmJournalStore, replaceable WAL and exclusive
      table-namespace contracts without SQLite or changing engine defaults.
- [x] Keep disk payloads in actual immutable doublets pages, retaining only
      point-lookup metadata plus the bounded write buffer in the live owner.
- [x] Verify 152 finite before/after mutation cuts in each language, covering
      group publication and compaction, acknowledged data, uncertain fencing,
      nonreused table reservations and following writes after recovery.
- [x] Reproduce early table-repair completion and failed-close ownership loss
      in JavaScript, then defer completion until all selected bindings validate
      and preserve explicit cleanup retry in every production port.
- [x] Exercise native Rust cluster links/names, read-only stale guards,
      partitioned catalogs and epoch-fenced collection on actual physical stores.
      JavaScript's cluster adapter uses a binding mock; partition/reclamation
      tests use the production consumers. The C# equivalents also pass.
- [x] Add native Rust and C# recovery controls for hidden historical conflicting
      generations, missing/excess generations, combined retained capacity and
      generation exhaustion. JavaScript has the corresponding selected controls.
- [x] Exercise actual mirrored table devices, failed later bindings, typed
      admission, owned scan results and repeated hot-key WAL rotation. All 18
      Rust and C# focused controls pass; JavaScript has 33 focused controls.
- [x] Complete native Rust, Debug/Release C#, actual-WASM and Node/Bun/Deno
      suites, strict formatting/lint/types, whole-project coverage denominators,
      existing physical-table interchange and independent vector regeneration.
- [x] Document the public storage contracts, ownership, finite capacities,
      amplification, constructor/recovery ordering and native-provider limits.
- [x] Recheck optional loading and SQLite independence, repository/release
      gates and latest feedback. Review the local diff; GitHub rejects the
      complete PR diff endpoint because the PR exceeds 300 files. The local
      git diff against origin/main supplies the complete review fallback.
- [x] Inspect fresh exact-head CI after the portable commit is published. All
      nine workflows pass on bc364d2; lsm-store-ci.txt records each URL and
      native platform, coverage and fresh-merge job timestamp.
- [x] Commit and publish the verified portable step; continue the unchecked
      native file, process-exit and wider acceptance work in PHYSICAL-LSM-PLAN.md.

Run experiments/issue-138/validate-lsm-store.sh with the repository's pinned
coverage/.NET tools. It runs compilers sequentially, limits .NET managed heap
to 640 MiB and linked WASM memory to 256 MiB, and saves full logs. The native
and actual-WASM store tests share source; neither native-only results nor a
binding mock substitute for executing the WASM controls.

Physical recovery inside this API starts after the supplied WAL is constructed.
The caller/provider must perform bounded discovery and semantic validation
before a native WAL constructor that can repair, truncate or initialize files.
The independent read-only inspection experiment prepares that unfinished facade;
it is not yet a production native file-store implementation.

Complete local validation finishes 2026-10-27T01:52:07Z. Native Rust verifies
1,096 distinct tests (1,091 in the full run plus five added controls), six
doctests, 692 actual-WASM tests, 2,369 C# tests per configuration, 568 JS tests,
209 selected Bun/Deno tests each and 221 repository checks. The existing
immutable-table matrix passes all 25 native writer/reader pairs.
`dev/log/issues/138/pulls/139/lsm-store.txt` preserves complete coverage
denominators and the distinction between portable images and native files.
