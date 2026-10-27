# Causal session query and protocol parity

Continue the ordinary mutable session work under R6/R7/R8/R9. The raw session
preserves every u64 address. The Rust query processor, named-store trait and
reference validator now select an address type while retaining their u32
facades. Its protocol operations now select the same address type; original
query-text envelopes preserve reference kinds, and the actual WASM/JS session
client uses that shared path. CAUSAL-PROTOCOL-PLAN.md tracks fresh validation
and the remaining cross-language/physical recipe work.
C#'s advanced mixed-query processor also specializes its options,
matching and mutation paths to uint despite the generic links interface.
The C# standard API still needs the matching address-width extension and
allocation correction. The raw API alone does not complete this criterion.

- [x] Trace the existing named-store adapters, query parsing, matching,
      substitution, reference validation, change simplification, structure
      formatting and protocol dispatch. Standard query saves must leave the
      workspace private until the caller supplies the publication token.
- [ ] Write missing-API contracts before changing production. Cover ordinary
      query creation, matching, mixed updates, named leaves and deletion over
      the same session; protocol count/each/create/update/delete/name calls;
      canonical text and binary documents; and unchanged real storage until
      explicit publication. Preserve the existing duplicate/non-cascading
      semantics of the raw engine adapter.
- [ ] Include addresses above u32::MAX, u64::MAX as a real identity/reference,
      independent lower reservations, malformed/out-of-range numeric input,
      wildcard restrictions and frozen/disposed mutation refusals. A wildcard
      marker must never alias a legitimate u64 address.
- [ ] Audit parser/matcher depth and result expansion before exposing the
      additional transport. Bound deliberate deep-input probes by finite
      input and process stack/memory limits. Preserve existing request-size
      admission and reject unsupported input before any real publication.
- [x] Reuse the existing Rust query implementation with an explicit address type,
      retaining the existing public u32 behavior by default. Evaluate a wider
      checked intermediate for the u64 adapter so its wildcard marker lies
      outside the complete stored address range. Do not truncate, remap real
      identities to temporary u32 slots, or fork the query algorithms.
- [x] Replace the Rust query reference planner's create/delete allocation probe with an
      optional checked reservation for monotonic staged allocators. Its
      existing assumption that deletion returns the last allocated address
      does not hold for engine transactions or caller-reserved causal ranges.
      Reproduce anonymous and forward-reference queries before implementing
      this extension; preserve the existing fallback for recycling stores.
      A read-only preview cannot fence concurrent allocators. Reserve an owned
      prefix without creating rows, retain that prefix across repeated planning,
      and consume it only on actual allocation. Explicit creation must retire
      a matching reserved identity without losing the remaining prefix.
      Rollback may leave address gaps, matching the existing engine contract.
      Verify independent transactions, explicit high/overlapping identities,
      rejected requests and exhaustion before changing the allocator.
- [ ] Add the checked named-store adapter to the sealed causal workspace and
      route actual WASM/JS requests through the shared query/protocol path.
      Keep decimal wire addresses and full-width owned results in JS/TS.
      Extend the C# standard query processor's uint-specialized implementation
      while preserving its existing facade. Exercise a checked wider adapter
      over the ulong links interface with matching shared behavioral vectors.
- [ ] Run focused existing u32 query/protocol regression suites before the
      complete native, C#, actual-WASM and Node/Bun/Deno suites. Preserve all
      production coverage denominators, strict lint/type/format checks and
      exact before/after evidence. Run heavy compilers sequentially within the
      container limit.
- [ ] Extend the runnable plain/mirrored physical session recipe to perform
      ordinary query/protocol work, then repeat complete-store interchange,
      finite reaped process exits, exact retry and following edits. Rejected
      requests must preserve all actual physical files.
- [ ] Document query costs and any remaining limits, review the full diff,
      commit and push only issue-138-adc2aa56c4cb, and inspect fresh exact-head
      platform checks and artifacts in PR139.

The query work does not supply distributed allocation, membership/history
reclamation or hardware power-loss certification. Every other unmet R1–R12
criterion remains active; PR139 remains draft and unmerged.

## Reproduced allocation failures

The missing wide query processor and workspace `named_links()` adapter fail
compilation before implementation (`causal-query-before.log`). The proposed
adapter name preserves the existing raw `LinksStorage::query_links` method.

`query-allocation-contract.rs` reproduces a separate existing bug against the
compiled ordinary u32 engine. `(() ((1 1)))` succeeds but produces identity 2
with references to missing identity 1. The reference planner allocates/deletes
identity 1 while predicting a recyclable allocation; the monotonic engine
correctly keeps its next address at 2. This is a real production failure,
independent of adding a wide adapter.

`run-query-allocation-experiment.py` compares fifteen finite queries against
the compiled monotonic engine and the recycling store under 512 MiB address
space and 8 MiB stack limits. All engine snapshots remain unpublished. Two
anonymous forward references and nested named definitions also leave missing
references in the monotonic engine. Explicit definitions followed by anonymous
or named allocations expose false rejections in both stores; a high explicit
definition followed by a lower forward reference exposes a bad prediction in
both stores. The matrix records actual rows and missing references, rather than
treating the recycling implementation as a correct oracle.

A fixed count of anonymous syntax nodes is therefore insufficient for exact
planning: pair reuse can avoid allocation, named leaves can allocate, and
explicit definitions can change the allocator before a later anonymous node.
The reservation work must model the actual creation path and validate these
cases. Reserving the current overestimate and claiming every reserved identity
will be created would retain the dangling-reference bug. These failures are
preserved in `query-allocation-before.log` and
`query-allocation-matrix-before.log`. The ordinary Rust engine and causal
workspace now preview the shared execution path and reserve its actual
allocations. Focused contracts verify the reproduced anonymous/forward,
named/pair-reuse and explicit-high cases. The recycling-store fallback and
C# planner still require their separate corrections. CSHARP-QUERY-PARITY-PLAN.md
records the independently reproduced C# anonymous-point failure and passing
explicit-address controls.

## Canonical protocol reference kinds

`node --max-old-space-size=64 --stack-size=1024
experiments/issue-138/query-document-kinds.mjs` proves that the existing
canonical string document model maps quoted or encoded numeric names, quoted
`*` and quoted `$x` to the same documents as bare addresses, wildcards and
variables. Formatting those documents cannot recover their original kinds.
`query-document-kinds.log` preserves the five finite controls. This is a
separate protocol-boundary requirement; the direct Rust query parser preserves
those distinctions. Extending only the operation address width would leave
this loss unresolved. Query transport needs an explicit lossless query-text
envelope, or another specified representation that retains reference kinds,
with matching native and actual-WASM/client tests before claiming parity.
