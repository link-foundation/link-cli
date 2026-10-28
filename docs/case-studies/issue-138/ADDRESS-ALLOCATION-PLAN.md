# Coordinated fresh address reservations

Continue R4/R6/R7/R9/R11 after the exact legacy query allocator fixes. The
ordinary causal workspace accepts caller-reserved ranges; it cannot prevent
independent writers from choosing the same fresh identities. Add a separately
selected reservation service over the existing atomic journal-store contract.

- [x] Recheck existing atomic store, causal owner and session APIs and costs.
- [x] Compare PostgreSQL sequence caching/non-reuse and etcd linearizable CAS.
- [x] Define one canonical bounded journal with immutable inclusive domain,
      contiguous non-recycled grants, author-bound idempotency tokens and
      unsigned 64-bit admission/exhaustion.
- [x] Write independent finite vectors and allocation schedules before the
      implementation; first demonstrate the missing selected API.
- [x] Implement Rust, C# and portable JS/TS against the existing provider API.
      Every attempt rereads state and attempts at most one CAS. A lost reply
      retries the exact author/token/count and never invents a second grant.
- [x] Verify differing authors/counts cannot alias an old token; malformed,
      oversized, exhausted or conflicting requests preserve provider bytes.
- [x] Verify race schedules, exact retries after following grants, high u64
      ranges and finite admission with owned observations.
- [ ] Link portable Rust contracts to actual WASM and run runtime parity in CI.
      Add focused local scripts; reserve complete suites for CI.
- [x] Supply usable physical/etcd examples and docs that disclose the single
      coordinator boundary, retained receipts, range gaps and migration costs.
- [ ] Review compatibility and lazy imports, update release fragments, run
      focused checks, commit and push the prepared branch; inspect exact-head CI.

A domain starts above every identity ever issued outside this allocator,
including tombstones. Every fresh writer in that domain must use the same
coordinator key. Grants are permanent and are never recycled after abort,
process exit, failover or lost replies. Distinct requests receive disjoint
ranges. Author lineage fencing, foreign/uncoordinated writes, membership,
receipt reclamation, multi-catalog publication and hardware power-loss proof
remain separate acceptance work. This allocator must not imply those guarantees.

Primary comparison sources, checked 2026-10-28:

- https://www.postgresql.org/docs/16/sql-createsequence.html describes cached
  blocks, distinct concurrent values, abandoned gaps and non-cycling exhaustion.
- https://www.postgresql.org/docs/16/functions-sequence.html explains why
  transaction abort does not recycle values and why externally visible values
  need committed persistence.
- https://etcd.io/docs/v3.1/learning/api_guarantees/ specifies default linearizable
  operations and the weaker selectable serializable read mode. The allocator
  requires the existing provider's linearizable read/CAS contract.

The design is an explicit optional range cache, with O(retained journal) reads,
replay and writes per new grant. It amortizes coordination across ordinary local
creates but has one shared allocation bottleneck and a finite receipt budget.
Existing bare stores and caller-reserved sessions select no new work.

Implementation and local evidence: [ADDRESS-ALLOCATION-VERIFICATION.md](ADDRESS-ALLOCATION-VERIFICATION.md). Actual WASM, full five-runtime physical/exit matrices and real-etcd execution remain CI checks.
