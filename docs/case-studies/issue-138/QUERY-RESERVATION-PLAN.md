# Exact staged query allocation

Continue CAUSAL-QUERY-PARITY-PLAN.md without claiming the wider R1–R12 scope
complete. The existing create/delete predictor consumes monotonic identities;
counting anonymous syntax nodes also mispredicts pair reuse, named leaves and
explicit high definitions. Preserve the existing recycling-store fallback.

- [x] Reproduce the dangling anonymous forward reference on the ordinary
      engine, plus the fifteen-case engine/recycling allocation comparison.
- [x] Preserve missing generic query/named-store API failures; extend the same
      Rust matching, mutation, validation and formatting implementation with a
      selected address type and the existing fixed-u32 facades.
- [x] Add checked u128-over-u64 workspace tests before the adapter. Admit
      numeric range, grammar depth, request bytes and node/result bounds.
      Reproduce numeric/control-name replay before correcting its encoding.
- [x] Run the actual shared query mutation path on owned private rows with
      reference validation disabled only for that internal preview. Collect
      actual allocations in order and actual final identities. Avoid a second
      implementation of query semantics.
- [x] Reserve those identities under the ordinary engine's allocator lock,
      without creating rows. Preserve an owned reservation prefix across
      validation replans and consume it on actual creation. Explicit ensure
      retires a matching reservation. Keep the private causal range independent
      of the engine journal allocator, including u64::MAX.
- [x] Verify anonymous/named/mixed queries, forward references, pair reuse,
      explicit high identities, independent concurrent owners, rejection,
      exhaustion, rollback and no unintended publication. Do not treat the
      recycling implementation as a correct oracle.
- [x] Bound map growth before Cartesian result allocation, then run focused
      existing u32 regressions, complete native and actual-WASM validation,
      strict lints/formats and repository/release checks sequentially.
- [x] Record exact before/after evidence, costs and remaining language/protocol
      work, including complete coverage denominators and source/report hashes.
- [ ] Review, commit and push only the prepared branch. Inspect fresh
      exact-head CI and retain complete coverage denominators in PR139.

Reservations may leave gaps on rejected or rolled-back transactions, as normal
engine allocation already does. They never publish rows or authorize an
address outside the caller's causal reservation/retained history. Preview is
planning evidence; ordinary transaction validation and explicit publication
remain authoritative.
