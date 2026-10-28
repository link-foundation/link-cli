# Engine publication of retained checkpoints

Requirement mapping: R1/R3/R5/R6/R7/R8/R9/R10/R11/R12. This batch composes the
portable two-bank journal with the existing engine and optional physical delta
codec. Native file ownership and every wider acceptance gap remain active.

- [x] Preserve missing-feature controls in real Rust, C#, JS and actual-WASM
      targets before adding engine publication APIs. The baseline is 652ce12;
      these controls demonstrate an absent feature, not prior data loss.
- [x] Share retained-checkpoint construction with export. Hold the engine commit
      mutex continuously through construction, replacement and acknowledgement.
      Leave ordinary commits and default logs unchanged.
- [x] Preserve immutable history, pinned views, exact committed allocators, live
      private reservations, pending sessions, conflict generations, admission,
      point locks and saved branch merge-base descriptors.
- [x] Reject unsupported capabilities before replacement. Fence the engine and
      pending sessions after every replacement error; reopen complete old/new
      durable images and require following commits and replacement to work.
- [x] Delegate replacement through the physical delta codec only when supported.
      Validate and encode the complete input before inner I/O; adopt fresh codec
      state only after acknowledgement, including identical replacement.
- [x] Require synchronous boolean acknowledgements from JS/WASM adapters.
      Missing, throwing or non-boolean capability values do not opt in. Thrown,
      asynchronous and non-boolean replacement results fence the owner.
- [x] Run bounded native concurrency controls showing that a following commit
      cannot cross a blocked checkpoint publication. Always release the gate
      before checking the final result.
- [x] Verify all 72 bank/state/encoding/history/delta configurations with durable
      models in native Rust/C# and with actual WASM in Node, Bun and Deno.
      The runnable example uses finite volatile images and makes no physical
      durability claim; selected mirrored banks repair damaged closed copies.
- [x] Complete native/C#/actual-WASM suites and measured coverage, strict JS
      coverage and TypeScript, real-etcd modes, both existing native handoff
      controls and repository checks. Preserve complete logs and exact totals.
- [x] Review, commit and push only the prepared branch; inspect all fresh
      exact-SHA workflows and native platforms. Continue native owning factories
      and the complete R1–R12 inventory without treating this batch as completion.

Before evidence: `dev/log/issues/138/pulls/139/engine-compaction-before.txt`.
The full validator is `experiments/issue-138/validate-engine-compaction.sh`;
`summarize-engine-compaction.py` checks saved reports before publishing counts.
The [protocol](../../protocol/checkpoint-pages.md) documents lock duration,
selected costs, errors and resource ownership. The
[example](../../../examples/journal-checkpoints/compact.mjs) preserves history,
private reservations, groups and following allocation across independent images.

Complete local suites pass 1,002 native tests plus six doctests, 620 actual-WASM
tests, 2,210 C# tests in each configuration, 230 JS tests and 221 repository
checks. Coverage review registered eight pre-existing wrapper tests with the
actual WASM runner and added direct adapter controls. Wrapper lines, branches
and functions measure 100%; LLVM regions and instantiations remain below that
figure and are retained in the report. Whole-project totals remain below 100%.
Both existing native handoff controls pass 48 configurations/288 writers each;
all three real-etcd modes and identical 72-case runtime compositions pass.

Exact-head workflows and native platforms subsequently passed at 133c4ff;
see `dev/log/issues/138/pulls/139/engine-compaction-ci.txt`. The following
native-ownership work and wider requirements remain active.
