# Heap-doublets literal queries

This advances R1/R7/R8 through the existing public `DoubletsStorage` adapter.
The shared tests execute a real upstream heap store in native Rust and actual
WASM. They do not substitute a mock implementation or require a filesystem.

## Reproduction

An immutable `d3ba9bb` archive with the final test target fails exactly three
contracts while eight controls pass:

- `query_links(None, None, Some(0))` misses the live `(0, 0)` row.
- `search_link(0, 0)` returns `None`, allowing get-or-create to duplicate it.
- A concrete-pair query returns only one of two allowed bare-store duplicates.

```sh
bash experiments/issue-138/run-doublets-portable.sh d3ba9bba0b3ad5577264235a1245c926f4afcd14
bash experiments/issue-138/run-doublets-portable.sh
```

The baseline runner uses an isolated target directory. An earlier attempt
sharing fixed build outputs was rejected because its baseline unexpectedly
passed; the isolated rerun establishes the three actual assertion failures.

Upstream doublets 0.9.0 omits null endpoints from its endpoint indexes and uses
a single-result lookup when both endpoints are concrete. The adapter now uses
at most one usable endpoint index and applies every optional literal filter
to its candidates. `Some(value)` matches exactly, including zero and the value
of the upstream wildcard constant. `None` alone means unrestricted. Results
have no guaranteed order, and duplicate pairs remain allowed without a
uniqueness decorator. Null/wildcard-value search scans and stops at the first
exact match; get-or-create reuses it. Ordinary nonzero pair search retains its
upstream index. Scan cost is O(N); indexed filtering costs the chosen endpoint's
candidate traversal plus its output. No complete row copy is made for search.

## Verification

All eleven native contracts pass. The independent 125-pattern filter oracle,
typed missing-row errors, u64/usize endpoints, address recycling, caller-owned
store extraction, observed uniqueness/cascades and rollback remain covered.
An additional real-file test reopens persisted null/duplicate rows and verifies
exactly unchanged physical bytes after queries and get-or-create reuse.
All 24 related native storage/decorator/observer tests pass; the established
ignored child helper is executed by its parent cross-process lock test.

Twelve contracts pass in actual WASM, including typed capability errors from
the unavailable filesystem constructors. With cargo-llvm-cov 0.9.1,
wasm-bindgen 0.2.129 and the pinned Rust 1.98.0 coverage toolchain on `PATH`:

```sh
bash experiments/issue-138/run-doublets-portable-wasm-coverage.sh
python3 experiments/issue-138/summarize-doublets-portable-coverage.py experiments/issue-138/doublets-portable-wasm.json
```

The verified earlier complete WASM artifact records zero executed lines for
this file. The focused final execution records 255/294 lines, 28/38 branches,
52/53 functions, 365/447 regions and 76/142 instantiations. Complete linked
production totals are retained in the report and receipt. These focused
counters do not establish full-suite, filesystem or whole-project coverage.
Filesystem success paths and generic instantiation gaps remain visible.

Strict focused Clippy, formatting, ShellCheck, all 221 repository checks,
source-size and release-fragment checks pass. Full suites remain in CI per
the maintainer's instruction. Fresh CI for this change remains required.
[The receipt](../../../dev/log/issues/138/pulls/139/doublets-portable.txt)
preserves commands, immutable failures, report/source hashes and exact results.
Generic mapped resize/partial-shrink failure, hardware power loss and the
other unfinished R1–R12 requirements remain separate open criteria.
