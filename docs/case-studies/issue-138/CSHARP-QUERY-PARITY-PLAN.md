# C# standard query path over private sessions

Continue CAUSAL-PROTOCOL-PLAN.md. The existing C# processor, options, pattern
matching and mutation helpers use uint. CausalSession implements the ordinary
ulong links interface, whose wildcard aliases ulong.MaxValue. A checked wider
adapter is required to preserve that real stored identity. The raw session
contracts and passing Rust/actual-WASM/JS protocol tests do not verify this work.

- [x] First reproduce the existing monotonic allocation failure through
      AdvancedMixedQueryProcessor and EngineSession<uint>. Keep the smallest
      anonymous forward-reference case and independent explicit/pair-reuse
      controls; assert private and published rows separately.
- [ ] Add missing generic-query and checked-session API contracts before
      production changes. Cover all u64 identities, wildcard separation,
      lower caller reservations, quoted/encoded numeric and special names,
      owned observations and explicit-token publication with exact retries.
- [ ] Select an unsigned generic-math address in the existing partial query
      implementation, retaining its public uint Options and Pattern facade,
      implicit string conversion and existing query semantics. Reuse the
      matching/mutation algorithms rather than copying them for causal use.
- [ ] Preserve reference kinds in QueryParser for the selected numeric type;
      reject malformed, negative and overflowing numeric input before checked
      private writes. Retain the established legacy uint interpretation.
- [ ] Add optional query preparation and direct explicit-address capabilities
      to the ordinary named-store interface with compatible defaults. A
      monotonic session must not use the current create/delete prediction or
      walk every intervening address through LinksExtensions.EnsureCreated.
- [ ] Under the allocator owner lock, execute the same query against owned
      preview rows/names with validation disabled, record its exact allocation
      order and validate final references and reply budgets. Reserve that
      sequence without creating probe rows; consume it in real creation and
      retire only a matching explicit reservation. Input and preview admission
      failures preserve rows and the allocator cursor. A later missing-reference
      rejection or rollback may retain already reserved gaps, matching Rust.
- [ ] Bound input bytes, parser depth, document nodes, result/binding expansion
      and decoded named reply bytes before checked writes. Use finite bounded
      probes, ordinary positive controls and no host-exhaustion experiments.
- [ ] Extend standard protocol operations and server dispatch with the same
      selected address type, preserving all uint entry points. Carry original
      query text in the matching explicit envelope through text and binary;
      apply checked admission before recursive canonical formatting.
- [ ] Verify ordinary and causal allocation/concurrency/freeze/disposal cases,
      existing uint query/protocol suites and complete Debug/Release tests.
      Measure complete production coverage without removing denominators;
      run the existing source, changeset and release checks before committing.
- [ ] Add matching shared vectors and physical plain/mirrored query recipes,
      verify cross-language whole-store interchange and finite child cleanup,
      document costs and preserve before/after receipts. Commit useful steps,
      push only issue-138-adc2aa56c4cb and inspect exact-head platform CI.

Current source audit: AdvancedMixedQueryProcessor{,.Matching,.Mutations,
.References}.cs, QueryParser.cs, INamedTypes.cs, LinksExtensions.cs,
Engine/EngineSession.cs, Engine/CausalSession.cs and Protocol/LinksServer.cs.
The standard extension currently loops Create until an explicit target and
the query reference planner creates/deletes temporary links. Neither path
provides a reservation for a monotonic allocator. EngineSession already has
an owner gate and explicit EnsureCreated; preserve its lock ownership and
existing concurrency policies when adding query preparation.

`bash experiments/issue-138/run-csharp-query-allocation.sh` reproduces the
anonymous point as `(2: 1 1)`, with an unpublished engine snapshot and missing
identity 1. The independent explicit point and explicit-high/pair-reuse
controls pass. `csharp-query-allocation-before.log` preserves the three finite
results; the failure remains unfixed. Each case creates at most four rows.

All other unmet R1–R12 criteria remain active. PR139 stays draft and unmerged.
