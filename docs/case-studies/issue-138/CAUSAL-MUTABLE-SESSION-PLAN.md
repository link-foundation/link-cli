# Ordinary mutable causal sessions

Continue the unchecked ordinary-session work in CAUSAL-SESSIONS-PLAN.md.
The objective remains every R1–R12 requirement in PR139, with no requested
criterion represented as complete without implementation and verification.

- [x] Trace the standard engine mutation, validation, allocation, callback and
      cleanup paths. Reuse the existing private mutation engine. Keep its
      journal allocator above every visible row; independently enforce a fresh
      caller-reserved inclusive allocation range, including u64::MAX.
- [x] Independently model 1029 finite three-operation schedules and 686 exact
      before/after lost-reply retries under finite memory/stack limits. This
      is design evidence, not production verification.
- [x] Prepare matching eight Rust/C# contracts for ordinary rows/names and
      callbacks; independent reservation; empty final differences; frozen
      unknown retries; all losing siblings/deletions; selected constraints;
      stale or oversized final batches; owned results and repeated cleanup;
      malformed token and payload preflight.
- [x] Install the contracts and preserve missing-public-API failures before
      adding each production port. Add actual-WASM portable execution.
- [x] Add the separately selected Rust workspace/publication adapter, sharing
      existing batch normalization/token preflight without weakening guards.
      Seed an owned revision-zero memory engine, validate its selected rows,
      and expose only the sealed standard links/names API.
- [x] Add the matching C# standard INamedTypesLinks surface with owned rows,
      explicit prepared/completed/disposed states and checked callbacks.
- [x] Add the actual WASM workspace binding and JS/TS publication adapter.
      Preserve full u64 addresses, synchronously reserve preparation before
      queued work drains, reject later operations, and always free handles.
      Verify the adapter with mocks first and actual Node/Bun/Deno bindings.
- [ ] Address ordinary query/protocol parity where its existing u32 surface
      cannot represent a supplied u64 snapshot; never truncate addresses.
- [x] Verify no-op receipts, all retained observations, stale/conflicting bases,
      publication errors before/after CAS, exact retries after later writes,
      constraints across all state/index choices, finite counter/address limits,
      callback behavior, ownership and cleanup. Preserve every rejection's
      unchanged real-store evidence.
- [x] Compose the ordinary session in plain/mirrored SQL-free physical stores
      across all five runtimes, with complete-store handoffs, finite reaped
      process exits, unchanged-byte stale refusals and exact following edits.
      All 50 handoffs and 100 recovery/following sequences pass after twenty
      reaped exit-73 children; 100 blocked opens preserve all physical hashes.
- [x] Add runnable use cases and exact CPU/space/wire/durability costs. Run
      complete unfiltered suites/coverage, formats/lint/types and release checks
      sequentially; preserve every denominator and immutable report identity.
- [ ] Review, commit useful steps, push only issue-138-adc2aa56c4cb, inspect all
      fresh exact-head workflows/platform artifacts and update PR139.

Preparation freezes the original owner generation, token and complete final
edits before publication. One coherent local batch is the only real write.
An unknown reply retains its original exact retry request; it cannot rebase or
accept further mutation. Repeated success returns an owned cached result.
Cleanup releases private resources and cannot undo a completed publication.
Per-row remote exchange does not preserve group atomicity; independent writers
still need disjoint reserved addresses and uniquely fenced author lineages.

All wider requirements remain active, including complete project coverage,
generic mapped failures, competitor algorithms, catalog scaling, global
allocation and safe membership/history reclamation. PR139 remains draft.
