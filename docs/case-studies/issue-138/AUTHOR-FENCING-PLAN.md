# Selected author takeover fencing

Continue R4/R6/R7/R9 with an optional AtomicJournalStore decorator. Address
reservations alone cannot prevent a replaced owner from publishing causal dots.
A separate checked ownership key must participate in the same coherent data CAS.

- [x] Reproduce the missing API and use an independent bounded model/vector set.
- [x] Define canonical immutable-author header and permanent sequential claim
      receipts. Explicit takeover requires the observed predecessor epoch and a
      unique owner token; retrying a superseded token must never retake ownership.
- [x] Implement selected Rust, C#, JS/TS providers with no background work and
      no default-path imports. Bind owned claim descriptors; check the current
      claim during reads and atomically alongside every guarded data publication.
- [x] Verify takeover before publication, takeover inside the final CAS, same-base
      claim races, before/after lost replies, exact retries and stale receipt refusal.
- [x] Verify canonical/bounded admission, owned observations and no unguarded
      provider mutation after malformed input or a known fence failure.
- [x] Compose ordinary causal batches and permanent address reservations in
      focused portable tests; retain a real-etcd/actual-WASM session recipe.
- [x] Verify the new recipe and linked portable tests in fresh full CI.
- [x] Add reproducible recipes, cost/compatibility limits and release fragments.
- [x] Run focused checks locally, complete suites in CI, commit and push only the
      issue branch; inspect fresh exact-head CI before claiming verification.

All nine workflows pass on `9ece2d1`, including both real-etcd native/WASM
compositions and all fifty physical handoffs. Later process and native source
coverage audits have separate plans and still require fresh CI.

This is explicit application-authorized takeover, without TTL, leader election,
permissions, membership or automatic failure detection. All mutations of guarded
keys must pass through the selected provider; bypassing it is outside its contract.
A claim token identifies one allocating process lineage and must not be shared.
The coordinator retains every claim and refuses new claims when its finite budget
fills. One fence key reduces the selected provider's data-key limit to 31.

Primary sources checked 2026-10-28:

- https://zookeeper.apache.org/doc/r3.9.3/recipes.html describes recovery by a
  retained GUID when a successful lock creation loses its response.
- https://etcd.io/docs/v3.6/learning/api/ specifies atomic conjunctions of key
  comparisons and grouped requests. Design inference: checking a claim only in
  a preceding read leaves a race; add the current fence generation to the same
  data CAS, with no rewrite of that key.

No foreign implementation or wire format is copied. This does not complete safe
history reclamation, uncoordinated writer admission, or other open requirements.
