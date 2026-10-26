# Native inline-name persistence

Requirement mapping: R1/R3/R6/R7/R8/R9/R12. The structure audit exposed a
separate round-trip defect in Rust's legacy `LinkStorage` text format.

- [x] Trace the reader/writer separately from the normal Unicode names sidecar.
- [x] Fail an automated minimal reopen test before changing production code.
- [x] Exercise whitespace, quote edges, raw backslashes, literal-looking names,
      Unicode, controls and line breaks, including text resembling another row.
- [x] Audit names attached to absent/external addresses without creating rows.
- [x] Reuse the existing versioned UTF-8 literal codec in namespaced comment
      metadata for line-unsafe text and names without rows; preserve legacy
      readable names, unquoted literal-looking text and exact raw backslashes.
- [x] Test malformed/unsupported encoded metadata without rewriting the file,
      repeated reopen, subsequent rename/removal and allocator reuse.
- [x] Reproduce failed reload after a valid row followed by invalid UTF-8;
      preserve live rows, names, indexes and allocation on read/parse failure.
- [x] Verify existing native/C#/actual-WASM name/query/output behavior and the
      complete local suites, coverage, formatting and release fragments.
- [x] Publish the correction and verify fresh exact-head CI.
- [x] Record file compatibility, chosen costs and limitations in the case study
      and PR; preserve the wider native-storage and coverage criteria as open.

Observed code: `parse_link_line` splits the entire record on whitespace and
uses only its fourth token as a name, trimming every quote at its edges. `save`
inserts names directly into a single-line record without encoding line breaks.
The name table also permits addresses that do not have a stored row, while the
old writer enumerates only rows. Seven automated before regressions all fail on
`faf191d`, including absent-address names and partial live-state loss during a
failed reload. All seven now pass. Four additional native contracts cover
independent metadata bytes, eighteen invalid/future metadata cases with plain
and indexed reload, 64 finite mixed-Unicode names, legacy spellings and following
rename/removal/address reuse. Existing storage/index tests also pass. A portable
memory-reload control runs in native and actual WASM. Complete local validation
passes: 894 native, 521 actual WASM, 2,027 C# per configuration, 177 JS and 221
repository checks, plus all three generated-WASM client runtimes. The native
text-format component measures 100% lines/branches/functions. Whole-project
totals remain below target. The runnable native document-label example passes
without optional providers. See inline-names.txt and TEXT-STORAGE.md;
all nine workflows pass on published head `b125e6d`; inline-names-ci.txt
preserves the exact head, timestamps and links.

Compatibility choice: encoded text belongs only to a versioned comment
namespace, not an ambiguous new meaning for legacy unquoted `~1{31}` names.
Rows with control-character names can be written unnamed, followed by their
exact encoded name metadata in the same replacement file. Older readers skip
that metadata and cannot recover those names; updated readers are required for
the extension. Safe readable names retain the old quoted row format. The
existing Unicode doublets names sidecar uses a separate representation.
