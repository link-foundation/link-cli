# Independently selected immutable sorted runs

Requirement mapping: R5/R6/R7/R8/R9/R11/R12. Every wider criterion remains active.

- [x] Read the existing row representation, immutable sharing, journal adaptation
      and native/WASM/JS configuration seams before choosing an implementation.
- [x] Study primary RocksDB architecture and universal-compaction documentation
      and pinned source; distinguish an in-memory strategy from physical SSTables.
- [x] Preserve a failing public selection contract before adding the option.
- [x] Implement immutable sorted runs in Rust and C#, with identical selection
      through WASM/JS/TS and the existing links/session API.
- [x] Verify newest-write precedence, tombstones, resurrection, sorted scans,
      immutable snapshots, cheap clones, no-op writes and all representation
      conversions against independent finite maps.
- [x] Verify independent admission/isolation/validation/history/encoding choices,
      group and branch behavior, failed durability and physical journal reopening.
- [x] Document exact costs, compaction boundaries and runnable usage; preserve
      default flat/shared behavior and optional dependency guarantees.
- [x] Run complete native/C#/actual-WASM/JS suites, coverage, lint/types/format,
      interoperation and repository checks.
- [ ] Commit, push and obtain all fresh exact-head workflows.

The selected design is binary carry over immutable sorted runs. A point edit
starts a one-entry run. Occupied event-count tiers merge with newer values
winning equal keys. A tombstone hides older runs and can disappear only when
every older run participates. Clones share run arrays; historical snapshots
retain their original arrays. Lookups search newest runs first. Scans lazily
materialize an ordered view; that selected cost must be stated explicitly.

Flat stays the default. Sorted runs are an in-memory revision representation,
independent of the existing logical journal and physical page provider. They
do not supply RocksDB SSTables, Bloom filters, background disk compaction,
physical repair of legacy mappings or physical power-loss certification.
Those broader requirements remain unfinished.

Primary references:

- [RocksDB architecture](https://github.com/facebook/rocksdb/wiki/RocksDB-Overview)
  distinguishes its memtable, sorted on-disk tables and WAL and describes
  pluggable in-memory representations and wrapper layers.
- [Universal compaction](https://github.com/facebook/rocksdb/wiki/Universal-Compaction)
  describes merging adjacent time ranges and the write/read/space tradeoff.
- [Pinned compaction picker](https://github.com/facebook/rocksdb/blob/014e1c43c12947c2efe15275b7dd371bce13e6f1/db/compaction/compaction_picker_universal.cc)
  separately enforces size amplification, run-count and merge constraints.
  The downloaded 1,874-line source is retained in experiments/issue-138;
  reviewed sections 150–260 and 1040–1190 establish those distinctions.

Existing rpds/.NET immutable maps remain the shared-tree option. A small native
run component needs no additional package, native ABI or SQL dependency. Its
binary event-count policy is simpler than RocksDB's picker and must not be
presented as configuration equivalence to that complete storage engine.

The actual compiled public WASM binding rejects sorted-runs before implementation
with "unknown state structure". The retained finite contract is
experiments/issue-138/sorted-runs-contract.mjs; sorted-runs-before.log records
its nonzero exit against the retained pre-feature web package. That package
predates this feature; its source SHA is not established by this execution.
This is a missing-feature baseline, not a claim of an existing behavior defect.

Focused runs pass 48 Rust component/engine contracts and 52 C# cases. Both full
C# configurations pass 2,164 tests, with their complete reports retained. The
first C# wrapper exits 127 after both suites succeed: its executing file was
edited while the case body ran, so Bash resumed at an obsolete file offset.
The original wrapper log is preserved; this is an investigation-script error,
not a test success or a production failure. Do not modify executing scripts.
Subsequent syntax/repository checks must verify the resulting validator.

Complete local validation passes 935 native tests plus six doctests, 546 actual-
WASM tests, 2,164 C# tests in each configuration, 209 JS tests on Node 26 and
24, strict types and 221 repository checks. All 48 physical/policy configurations
pass 288 Rust/Node/Bun/C#/Deno/Rust file writers, including damaged-copy repair.
See sorted-runs.txt and sorted-runs-before.txt under dev/log/issues/138/pulls/139.
Native sorted-run coverage is 88/88 lines, 18/18 branches and 10/10 functions.
Actual-WASM sorted-run coverage is 87/88 lines, 17/18 branches and 10/10 functions.
The three wrapper modules reach 100%; complete linked/native/C# totals remain
below target. No source-coordinate union is substituted for official totals.
Fresh exact-head CI remains required before this feature batch is verified.
