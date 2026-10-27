# Causal session query and protocol parity

Continue the ordinary mutable session work under R6/R7/R8/R9. The raw session
preserves every u64 address, but the existing Rust query processor, named-store
trait, reference validator and protocol operations specialize their addresses
to u32. C#'s advanced mixed-query processor also specializes its options,
matching and mutation paths to uint despite the generic links interface.
Supplying an ordinary u64 workspace therefore needs a real address-width
extension in both native implementations. The raw API alone does not complete
this criterion.

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
- [ ] Reuse the existing query implementation with an explicit address type,
      retaining the existing public u32 behavior by default. Evaluate a wider
      checked intermediate for the u64 adapter so its wildcard marker lies
      outside the complete stored address range. Do not truncate, remap real
      identities to temporary u32 slots, or fork the query algorithms.
- [ ] Replace the reference planner's create/delete allocation probe with an
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
`query-allocation-matrix-before.log`; their fixes remain unfinished.
