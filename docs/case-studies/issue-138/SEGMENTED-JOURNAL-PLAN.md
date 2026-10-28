# Bounded immutable journal chunks

Objective: reduce rewriting of unchanged journal prefixes in independently
routed data stores, without changing the atomic catalog decision or the common
journal API. This is further R5/R6/R7/R8/R9/R10/R11/R12 work; it does not finish
the remaining horizontal scaling requirements.

The current partitioned store writes one content-addressed complete journal.
An append changes its address and writes all records again. Its optional cache
avoids repeated downloads of an unchanged object, but cannot reuse the prefix
of a newly addressed object. Logical checkpoints bound retained replay at a
separate explicit history cost. Preserve all those existing choices and bytes.

- [x] Trace the current Rust/C#/JS partitioned store, verified object cache,
      reclamation epochs, catalog CAS and cluster replay boundaries.
- [x] Re-read current primary immutable-file publication research. Iceberg
      reuses manifests across snapshots and publishes through checked metadata;
      RocksDB documents the write/space/reclamation tradeoffs of value separation.
      Record the pinned source and distinguish this inference from either DBMS.
- [x] Run a finite independent model before production code: complete old/new
      publication after each staging/catalog cut, stale writers, lost replies,
      collisions, empty journals and finite bounds. Measure actual payload
      bytes written separately from reads, catalog metadata and replay work.
- [x] Specify canonical bounded chunk manifests and shared byte/address vectors.
      Whole-journal pointers remain readable and remain the default write mode.
      Reject unsupported versions, malformed references and excessive counts
      before fetching data. Validate all updates before any staging effect.
- [x] Reproduce missing selected chunk APIs in Rust/C#/JS before implementation.
      Reuse existing immutable-object validation and caching. Preserve full
      content comparison on staging; a cache hit must not silently weaken it.
- [x] Implement matching independently selected chunk policies in all languages.
      Stage and verify every referenced object before one atomic catalog CAS;
      retain unknown-outcome discovery and caller-owned values. A lost response
      must not automatically retry a mutation. Never overwrite hash collisions.
- [x] Exercise the actual engine, physical files and independent-runtime writers.
      Verify following edits, Unicode and owned snapshots, stale groups,
      missing/corrupt chunks, optional SQL and finite admission before I/O.
- [ ] Extend reclamation deliberately for the new reference graph. Existing
      blob reclamation must reject an unsupported manifest, without deleting
      referenced chunks. Reader epochs and exact-generation deletion remain
      necessary if chunk deletion is selected.
- [x] Run complete local checks and coverage, review and publish an atomic
      useful step, then inspect all fresh exact-head CI and preserve evidence.

Proposed first policy: deterministic record/UTF-8-byte bounded chunks, a finite
ordered manifest of immutable references in the catalog, and the existing
full-content collision check. Stable full chunks can be reused when appending;
the last partial chunk may be rewritten. Admit all requested journals before
I/O. Manifest size grows with chunk count and therefore requires its own finite
bound. A nonzero generation with an empty manifest remains a live empty journal;
an absent catalog generation remains distinct.

Fresh staging still reads existing objects to verify their complete contents.
Without a new verified strategy, chunking reduces bytes written and allows
read-cache reuse, while staging reads and full engine replay can remain linear
in journal length. Do not claim constant-time commits, improved measured
throughput, automatic balancing, multi-catalog transactions or bounded startup
replay from this policy alone. Those required criteria remain active.

Primary sources: [Iceberg specification at e5f8ab1](https://github.com/apache/iceberg/blob/e5f8ab19c82fcf24be430cc6c0cecf9a23f5afe4/format/spec.md#optimistic-concurrency)
and [RocksDB BlobDB design](https://github.com/facebook/rocksdb/wiki/BlobDB).
The locally preserved Iceberg source has SHA-256
`b8c9607bad6ac2bd42289a41d44befc56db79e9faa534f59bd120b0d7c0c1b89`;
lines 90, 94-100, 130-135 and 1539 were inspected. This is design research;
neither source format nor implementation is copied.

The independent `experiments/issue-138/model-chunked-journal.py` passes ten
before/after publication cuts and stale/empty/admission/collision controls.
In its finite JSON-payload example, appending one record writes 75 new chunk
bytes versus 293 whole-journal bytes, while fresh staging still reads 296
chunk bytes. These are model payload counts, excluding keys, metadata and
protocol framing; they are not production measurements or throughput claims.


The independent six-case UTF-8/address vectors and canonical chunks1 protocol
are implemented in Rust, C# and JS. Missing selected APIs first fail in both
native languages; JS first produces blob1 where the new test expects chunks1.
Seven Rust and seven C# chunk controls pass, including ten finite publication
cuts, malformed metadata before data access, modified chunks, caller-owned cache
values, complete staging comparison and prefix reuse. A JS sparse-array control
first reproduces skipped validation, then verifies rejection before staging.

All 626 Node tests pass at enforced 100% production line/branch/function coverage;
all eight chunk controls also pass in Bun and Deno, with strict TypeScript.
Actual Rust and C# chunk engines pass checkpoint, stale group and file-reopening
checks. All fifty directed plain/mirrored Rust/C#/Node/Bun/Deno chunk-store
handoffs pass, including physical hash preservation across read actions and
following edits with independently selected chunk sizes. All 221 repository
checks, source-size, changeset, workflow and shell checks pass locally.

The existing epoch-fenced collector now has a negative compatibility control in
every language: it rejects valid unsupported chunks1 metadata before deleting
anything. This establishes safe refusal, rather than completing chunk reclamation.
That compatibility control does not establish safe chunk deletion; the separate
CHUNK-RECLAMATION-PLAN.md tracks that implementation and its verification.


Complete local validation now passes 1,146 native tests plus six doctests,
706 actual WASM, 2,429 C# tests per configuration and 626 Node tests at enforced
full production coverage. Both complete-store layouts pass fifty five-runtime
handoffs. A consumer-only rerun verifies the generated WASM binding's explicit
256MiB memory cap and repeats all consumers, costs, handoffs and conformance.
journal-chunks.txt preserves every whole-project coverage denominator and cost.
Published as 860a47f with the dependency correction in 58d35f8, this step now
passes all nine exact-head workflows, including native Windows, Linux and macOS.
journal-chunks-ci-after.txt preserves exact SHA, runs and timestamps. The
chunk-reclamation item remains independently unfinished until its own full
validation and publication; every wider unmet criterion remains required.
