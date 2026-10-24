# Shared query and storage contracts in WASM

Continue R7/R8 with existing native semantic assertions and supported backends.

- [x] Inspect query, naming, Unicode, index, import and storage test fixtures.
- [x] Share fixture owners: native temporary files, WASM in-memory stores.
- [x] Register portable tests in the actual WASM runtime.
- [x] Keep filesystem persistence and revision checks native and unchanged.
- [x] Verify actual WASM tests and official complete LLVM coverage.
- [x] Verify focused/full native tests, Clippy and formatting.
- [ ] Preserve evidence and verify exact-SHA CI after publication.

The native constructors and temporary-file ownership stay equivalent to their
existing fixtures. WASM chooses the already-supported in-memory implementation.
Only semantic contracts shared by those backends execute on both targets;
filesystem contract assertions continue to run on native platforms. No production
code or coverage source exclusions change.
