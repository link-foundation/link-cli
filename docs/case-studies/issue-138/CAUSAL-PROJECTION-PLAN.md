# Owned causal row and name projection

Continue R4/R6/R7/R8/R9/R10 after local batches on published 80c41ad.
Keep mutable sessions and every wider unmet requirement active.

- [x] Independently model complete retained siblings, explicit rejection or
      maximum-dot selection, and visibility after selection. Check 128 finite
      scenarios, 3914 permutations and duplicate replays under 192 MiB address
      space and 8 MiB stack limits. Generate 132 semantic vectors, including
      dominated and cross-row dot contradictions and equal-clock contradictions.
- [x] Add production contracts before implementing APIs. Node first reports
      ERR_MODULE_NOT_FOUND, native Rust E0432 and C# CS0246/CS0103. Preserve
      complete diagnostics with byte counts and hashes in the before receipt.
- [x] Add separately selected production ports: Rust causal-projection feature,
      C# CausalProjection and JS/TS engine-causal-projection export. Reuse causal
      merge and ordinary row/name representations. Select from all retained
      siblings including deletions before deciding visibility; retain losing
      siblings independently of the chosen policy.
- [x] Verify five portable contracts in native Rust, C#, Node/Bun/Deno and
      actual WASM. Include every independent vector in both orders, unchanged
      inputs, owned results, explicit policies, null/empty/Unicode/BOM/NUL names,
      full u64 values and numeric row ordering. Admit the full supplied input
      before pruning: 4096 versions/8 MiB, 64 supplied versions per row, and
      exact valid maxima followed by rejected next inputs.
- [x] Run complete suites and unfiltered production coverage sequentially;
      retain every denominator, existing parent-driven entries and raw hashes.
      Verify formats, strict native/WASM Clippy, types, workflows, repository
      checks, selected dependency boundaries, fresh dependencies and releases.
      Complete validation finishes 19:07:39Z: 1231 native plus six doctests,
      757 actual-WASM, 2496 C# per configuration, 686 Node and 221 repository
      checks pass. causal-projection.txt retains every denominator and hash.
- [ ] Document costs/guarantees and remaining gaps; review all changes, commit
      and push only the prepared branch. Inspect fresh exact-head platform CI,
      download failed logs and verify actual platform coverage archives.

This component performs no I/O or publication and owns its snapshot results.
It does not allocate addresses, fence authors, reclaim history, or supply a
mutable ordinary links session. The next session work reuses existing raw links
and names APIs over a private engine and publishes one CausalBatch. Explicit
address reservations, lifetime/retry state, physical composition, complete
project 100% and every wider criterion remain unfinished. PR139 stays draft.
