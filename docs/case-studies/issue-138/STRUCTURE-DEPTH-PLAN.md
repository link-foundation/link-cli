# Deep structure output

Requirement mapping: R1/R7/R8/R12. A finite 2,048-node structure on a 128 KiB
isolated native Rust thread reproduces a stack overflow before formatting
finishes. Both Rust output implementations and the C# formatter recursively
expand the source spine; targets remain references.

- [x] Trace concrete Rust, generic Rust and C# formatting entry points.
- [x] Preserve the bounded standalone native failure.
- [x] Reproduce through automated bounded public-API tests in both languages.
- [x] Replace recursive output construction while preserving reference order,
      cycle handling, quoting, names, public trait behavior and failures.
- [x] Verify independent shallow oracles, deep finite output, shared children,
      cycles, persistence and actual WASM.
- [x] Run complete local checks and coverage; record costs and limitations.
- [ ] Publish an atomic step, update PR 139 and verify fresh exact-head CI.

No probe may run without finite input and isolated stack/memory/CPU bounds.
Keep the wider issue and its 100% whole-project coverage criterion unfinished.

Focused native tests now pass: seven Rust public contracts plus the ignored
bounded child entry and 16 C# output/depth tests. The first broader C# fixture
merged an existing cyclic pair into another pair during setup and aborted in
the separate usages-repointing resolver. The corrected fixture creates a new
independent self-cycle; the formatter itself was not entered by that failure.
Full sequential validation passes: 882 native Rust, 520 actual WASM, 2,027 C#
in each configuration, 177 JS and 221 repository checks, plus all three generated
client runtimes. Complete coverage remains below 100%; see structure-depth.txt
and VERIFICATION.md. Publication and fresh exact-head CI are still required.
A separately observed Rust legacy inline
text-name truncation is recorded for the next atomic audit in this same PR.
