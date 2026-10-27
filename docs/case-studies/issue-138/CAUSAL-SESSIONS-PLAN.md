# Ordinary links over selected causal ownership

Continue R4/R6/R7/R8/R9/R10 after local ownership and bounded Merkle transfer.
Those components preserve versions and publish individual writes, but they do
not yet implement an ordinary mutable links session. This is an unfinished
implementation criterion rather than an external dependency.

- [x] Establish an independent finite model before production changes. Model
      one local atomic batch, counter assignment, retry receipts, replay,
      rejection of a stale base and errors before/after publication. A failed
      later mutation must not publish an earlier mutation. Thirty finite
      scenarios pass in causal-batch-model.py with two authors, three rows,
      explicit before/after errors and exact replay. The process has 192 MiB
      address-space and 8 MiB stack limits. This is design evidence only.
- [x] Specify a separately selected batch-receipt protocol and shared canonical
      vectors. First evaluate reuse of the existing owner's single-write
      records in an isolated staging store, followed by one coherent two-key
      CAS publishing both the complete owner journal and batch receipt. This
      can preserve all existing owner parsers and keep the batch implementation
      absent unless selected. Bound requests, receipts and replay before
      allocation/publication; validate receipt ownership against actual owner
      records. Child retry-token collisions must reject without effects.
- [x] Add absent-API tests, then implement batch publication in Rust/C#/JS and
      actual WASM. A successful retry returns the original entire batch even
      after later edits; a changed request with the same token rejects. The
      original base generation is part of the retry request, and a receipt is
      checked before rejecting a now-stale base. No blind CAS retry is allowed.
      Seven native Rust/C# and actual-WASM contracts, fifteen Node/Bun/Deno
      contracts and 53 independent wire cases pass. JS owns caller input before
      asynchronous reads. Full validation remains a separate item below.
      A real physical-LSM contract also guards opaque owner generations:
      receipt-only publication need not change that key, and a following
      change can jump over another key's generation.
- [ ] Add an owned snapshot projection with explicit conflict rejection or a
      deterministic dot order. Choose from complete causal siblings, including
      retained deletion markers, before deciding whether a row is visible.
      Projection must not mutate or discard the retained causal evidence.
- [ ] Implement the standard links and names session APIs over a private
      write set and one local batch publication. Keep conflict policy and
      reserved address allocation explicit; independent writers require
      disjoint caller-assigned ranges or an external allocator.
- [ ] Verify multiple-row atomicity, read/name behavior, duplicate pairs,
      non-cascading deletion, no-op commits, conflicts and cleanup/retries.
      Cover owned snapshot lifetime, disposed sessions, counter/address limits,
      constraints actually selected, and unchanged bytes on rejected requests.
- [x] Compose the batch API in all five runtimes with native SQL-free plain/mirrored directories,
      exact reopen/retry/following edits and finite reaped process exits.
      Fifty complete-store handoffs and 100 recovery sequences pass after
      twenty reaped exit-73 children. One hundred stale-marker opens preserve
      every file hash before explicit operator recovery. Ordinary mutable
      links sessions need their own corresponding physical verification.
- [x] Document batch costs and guarantees and run full unfiltered suites and
      coverage sequentially. Complete local validation finishes 18:15:46Z:
      1226 native tests plus six doctests, 752 actual WASM, 2491 C# per
      configuration, 681 Node and 221 repository checks pass. The causal-batch
      receipts retain all denominators, admission controls and remaining gaps.
- [ ] Commit/push the prepared branch and inspect all
      exact-head platform checks and actual coverage artifacts.

Local atomic publication does not make separately transferred remote leaves
atomic as a group. Causal convergence does not establish global uniqueness,
cascades, foreign-key integrity or distributed serializability. Such guarantees
still require the existing coordinated strategy. Safe membership/history
reclamation, generic mapped failures, complete project coverage, all other
unmet requirements and the draft status of PR139 remain unchanged.
