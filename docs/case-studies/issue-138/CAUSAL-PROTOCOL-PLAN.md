# Full-width staged protocol requests

Continue CAUSAL-QUERY-PARITY-PLAN.md after the ordinary Rust query reservation
step. No completed Rust-only step substitutes for matching C#/actual-WASM/JS
behavior or for the wider issue's remaining requirements.

- [x] Add failing contracts for standard protocol count/each/create/update/
      delete/name operations over a checked full-u64 causal workspace. Include
      u64::MAX, wildcard restrictions, malformed/overflow addresses, owned
      results, private saves, freezes, explicit publication and exact retries.
- [x] Reuse the existing protocol operation implementation with a selected
      address type and preserve every existing u32 facade. Keep protocol
      wildcards separate from addresses with the existing optional parts.
- [x] Preserve numeric/name/wildcard/variable reference kinds by carrying an
      original query string as an explicit query request argument. Test its
      text and binary round trips before using it in a client. Keep legacy
      direct document requests available; their string-only model cannot
      recover a reference kind that was already erased by canonicalization.
- [x] Check finite request/depth/node/result admission before recursive
      formatting or staging, using the checked workspace's existing policy.
      Reject malformed envelopes and unsupported input without real writes.
- [x] Route actual WASM causal-session requests through these same operations
      and the shared query processor. Supply the standard LinksClient API in
      JS/TS through an owned queued transport, preserving full-width BigInts,
      cancellation/deadline outcomes, synchronous preparation transfer and
      explicit-token publication. Avoid duplicating query algorithms in JS.
- [x] Reproduce token-boundary comments and oversized raw TCP comments before
      correcting parser/framing admission. Reproduce named query/export reply
      expansion with finite inputs, then admit decoded reference bytes in the
      shared owned query preview before real edits and in reply construction.
      A finite instrumented JS control also requires oversized strings to reject
      before allocating their UTF-8 duplicate; multibyte admission still applies.
- [ ] Extend the C# standard query/protocol implementation with a selected
      address type while preserving its uint facade. Use a checked wider
      address type for causal sessions. Replace its monotonic allocation probe
      with a shared-path owned preview, with independent before/after tests.
      CSHARP-QUERY-PARITY-PLAN.md records the source audit and ordered contracts.
- [ ] Consume matching behavioral vectors in native Rust, C#, actual WASM and
      actual Node/Bun/Deno. Extend the plain/mirrored physical session recipe
      with queries and protocol calls; verify whole-store interchange, exact
      retries, following edits and finite reaped process exits.
      CAUSAL-PHYSICAL-QUERY-PLAN.md records the unchanged expected store history.
- [x] Run focused old/new contracts, then complete suites/coverage, strict
      formats/lints/types and release checks sequentially. Retain every
      production source and denominator, including uncovered code.
- [ ] Document exact costs/limits and reproducible evidence. Review and commit
      useful steps, push only issue-138-adc2aa56c4cb and inspect exact-head CI.
      Leave PR139 draft/unmerged while any acceptance criterion remains unmet.

The complete Rust/actual-WASM/JS driver passes at 2026-10-27T23:15:47Z;
`dev/log/issues/138/pulls/139/causal-protocol.txt` retains full denominators,
source/report hashes and before/after controls. C# and physical query work
remain unchecked above; the completed suites do not verify those criteria.

The finite reference-kind probe is
`experiments/issue-138/query-document-kinds.mjs`; its output is design evidence,
not a passing production fix. A new query envelope must preserve old clients'
raw operation documents and must be implemented before a client advertises it.
