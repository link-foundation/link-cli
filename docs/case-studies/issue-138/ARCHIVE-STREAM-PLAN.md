# Portable archive and stream contracts

Requirement mapping: R1/R7/R8/R9/R12. Every wider criterion stays active.

- [x] Read the native archive contracts, codec boundary and C# counterpart.
- [x] Share the ten existing pure archive contracts between native Rust and
      actual WASM; preserve the six native remote/CLI/filesystem contracts.
- [x] Retain the independently authored 30-byte cross-language golden vector.
      Native fixture owners must outlive their mapped stores.
- [x] Add finite stream contracts: 31 read-failure boundaries, 30 truncated
      prefixes, three trailing bytes, 30 partial-write failures and successful
      one-byte reads/writes. Compare existing rows and names after rejection.
- [x] Run focused native tests before any further production change; report passing
      controls honestly if no defect reproduces.
- [x] Execute all 967 native tests and six doctests, with strict Clippy and the
      complete official coverage report. Preserve all original native contracts.
- [x] Verify WASM file helpers report the unsupported filesystem with path
      context, preserve the store and permit following in-memory archive use.
- [x] Execute the shared contracts inside actual WASM, verify bounded memory,
      strict lint/format and complete official coverage denominators.
- [ ] Preserve complete local evidence, review the atomic diff, publish on the
      prepared branch and inspect fresh CI at its exact SHA.

Input parsing/validation must finish before import mutation. This contract does
not assert atomic undo after storage allocation, update, name or save failures.
An export writer can retain a valid byte prefix after an I/O error; only a
successful complete export is an archive. No flush durability is inferred from
an in-memory stream. Read/export failures must permit following valid use.

The preceding import-allocation production fix and its measurements are tracked
separately in IMPORT-ALLOCATION-PLAN.md. Complete-project 100% coverage and every
other unfinished issue requirement remain active.

All twenty focused native contracts pass: ten shared compatibility cases, four
stream groups and six retained native cases. These are passing controls, with no
new production defect reproduced or production change proposed by this batch.

The first WASM lint run rejects duplicate helper modules. The actual-WASM test
root now loads that helper once and each shared test imports it; strict Clippy
passes before the complete execution. This changes test composition only.

Complete native validation passes 967 tests plus six doctests. Actual WASM passes
571 tests with the verified 256 MiB cap. All 221 repository checks, three optional
SQL guards, formatting, strict native/WASM Clippy, source/release checks and
ShellCheck pass. archive-stream.txt preserves complete official denominators:
94.37% native lines and 81.45% complete-linked WASM lines. The archive component
measures 100% native lines/branches/functions and 98.47% WASM lines, 83.33% WASM
branches, 100% WASM functions; filesystem success is unavailable on that target.
The three wrapper modules remain 100%, separately from complete linked totals.
C#/JS production is unchanged; the preceding import-allocation batch's complete
validation remains the evidence for those surfaces, not a newly executed run.
