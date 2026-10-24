# Unambiguous persisted version names

Requirement mapping: R1/R3/R7/R8. Legacy version APIs accept branch and tag
names containing their sidecar serialization delimiters. Persisted names and
ancestry must round-trip exactly without changing the public API.

- [x] Reproduce delimiter/Unicode branch, tag and parent loss in both native
  languages, with mapped/file reopening and actual checkout data assertions.
- [x] Introduce a shared versioned encoding and wire fixtures, preserving legacy
  decoding and the existing fault-injection marker prefixes.
- [x] Cover punctuation, Unicode, empty parent, nested ancestry, malformed
  records, legacy reopening, and exact branch/tag/checkout behavior.
- [x] Run focused tests then complete native/C# suites, coverage, formatting,
  strict lint and release metadata checks; keep full logs in experiments.
- [x] Commit/push, review exact-SHA CI and preserve failed logs.

The staged engine already serializes complete names in checksummed revisions.
This audit concerns the existing native legacy version-control sidecar format.
Do not restrict accepted delimiter names as a substitute for correct storage.

Before implementation, all three independent C# and Rust cases fail: delimiter
branch, quoted parent and delimiter tag. C# reports a shortened branch and
changed parent, or a missing branch; the tag-only case cannot recover its name.
The two native before logs are preserved under experiments/issue-138.

Branch marker version two hex-encodes UTF-16 fields. Tags retain their existing
wire format and split at the final equals sign. Shared fixtures and legacy
reopening controls verify compatibility for unambiguous existing records.

Full native Rust passes 571 standard tests and the custom stdout harness;
C# Debug/Release each pass 990 tests. Actual WASM passes 279 tests and retains
complete wrapper coverage. Exact totals and before/after evidence are in
`dev/log/issues/138/pulls/139/version-metadata-names.txt`. Full coverage remains
below 100%; no exclusions or source-coordinate substitutes were added.

All nine workflows passed for exact head `7a91a70`, created 22:32:18 UTC.
Run URLs and timestamp checks are preserved in
`dev/log/issues/138/pulls/139/version-metadata-names-ci.txt`.
