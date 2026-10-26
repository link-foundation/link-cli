# Optional SQLite and native storage

Requirement mapping: latest PR feedback and R1/R5/R6/R7/R9/R10/R11.

- [x] Inspect resolved default native Rust and actual-WASM dependency trees.
- [x] Inspect the core C# library's resolved transitive packages, separately
      from its optional provider examples and test project.
- [x] Exercise the JS root import with SQLite module loading rejected, then
      exercise memory/native-file configurations without a SQLite connection.
- [x] Preserve automated dependency guards in normal CI and record local runs.
- [ ] Verify the guards in fresh exact-head CI.
- [x] Map each optional SQL guarantee to a native doublets implementation or
      an explicitly unfinished native requirement.
- [ ] Implement and verify the remaining native strategies in all supported
      surfaces; optional SQL component reuse does not fulfill this item.

Current architecture uses SQLite only at an optional ordered-log seam. Bare
stores, native mapped doublets, native file journals, staging, logical undo,
history, replication and catalog/object adapters do not require it. A selected
SQL log supplies physical page WAL/checkpointing through its provider. Native
logical undo and checksummed redo alone do not repair a torn mapped page.
Native physical repair, page undo, LSM, retention/replay scalability and the
remaining distributed strategies stay open in REQUIREMENTS.md.

The inventory must distinguish dependency optionality from native feature
completeness. Successful default builds establish the former, not the latter.

Local guards pass in Rust, C# and JS; the JS guard also passes under the CI's
Node 24 major. Native mapped-store/file-journal contracts pass with optional
providers disabled: 26 tests, zero failures, two ignored child entries exercised
by their bounded parents. See sqlite-optional.txt. All 221 repository checks,
actionlint and ShellCheck also pass. There was no pre-existing dependency leak
to reproduce: these are regression guards for the maintainer's explicit
optionality requirement, not evidence that native feature gaps are complete.
