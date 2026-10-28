# Shared native and WASM contracts

Continue R7/R8 by running the existing portable contracts inside a real WASM
runtime. Native test results do not demonstrate that the WASM target executes
those contracts successfully.

- [x] Inspect existing engine/model/protocol tests and separate OS requirements.
- [x] Reuse portable test bodies with target-specific test registration.
- [x] Extract pure protocol option fixtures from the TCP server helper.
- [x] Run the added tests in an actual WASM runtime before making coverage claims.
- [x] Verify native tests preserve their existing behavior and counts.
- [x] Collect official complete LLVM coverage, including the linked core.
- [x] Run native/WASM Clippy and formatting.
- [x] Verify exact-SHA CI after publication.

The shared tests cover isolation/admission/retention, independent branches and
merge failures, finite serial-history oracles, allocator limits, corruption,
replica/delta/partitioned journals, protocol vectors and model/parser behavior.
They retain their original assertions and finite fixture sizes.

Filesystem/process, native-thread and panic-unwind tests remain on native
platforms. WASM aborts rather than unwinds a Rust panic, and the unknown-unknown
target does not supply native threads or host filesystem access. The depth-limit
protocol test keeps its finite input and semantic assertions in both targets;
its native Instant check is replaced only on WASM by the test runner's existing
30-second deadline. No production source is excluded from the coverage report.

The WASM test crate adds a direct links-notation development dependency at the
same version already used by the core, so shared assertions can name its LiNo
model. Production dependencies and public runtime APIs are unchanged.

All 142 tests pass in actual WASM: 59 shared engine, 66 shared model/protocol,
nine wrapper-engine and eight web contracts. Native assertions and test counts
are preserved: 536 standard tests plus the stdout harness, with four child
entry points ignored. Official complete LLVM totals, including the linked core,
are 3,786/9,657 lines (39.20%) and 597/1,469 branches (40.64%). Wrapper source
remains at 100%; project-wide coverage is still unfinished. Additional generic
instantiations increase the report denominator. No production sources were
excluded and coordinate unions do not replace official LLVM summary counts.
