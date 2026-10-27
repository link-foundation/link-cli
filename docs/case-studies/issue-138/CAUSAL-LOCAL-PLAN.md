# Local causal publication and retry ownership

This continues the [anti-entropy plan](ANTI-ENTROPY-PLAN.md). The separately
selected version codec is in production; the local-owner implementation below
is still an experiment. Do not describe it as a durable replicated engine.

- [x] Create eight JS contracts before the new production API; the absent
      module fails independently. Exercise coherent local CAS, two handles,
      stale observations, before/after publication errors, durable retry
      identifiers, restart, concurrent deletion and historical contradictions.
- [x] Independently generate 26 bounded local-journal cases without importing
      a production codec. Include owner binding, exact canonical bytes, absent
      versus erased state, global author-counter progression, received context,
      historical equivocation, duplicate tokens and admission limits.
- [x] Run all eight contracts against a JS draft outside production source.
      This is experiment evidence. Rust/C# drafts also pass the 26 shared
      cases, retry/reopening and deletion controls. Real native durability and
      production parity remain separate unchecked steps. JS module coverage
      reaches 100% lines, branches and functions without reduced thresholds.
- [ ] Port the contracts to native Rust/C# and actual WASM; run them against
      missing production APIs before implementation. Validate all shared cases.
- [ ] Add the selected production local-owner layer over AtomicJournalStore.
      Reserve one key for one author lineage; read every operation afresh and
      publish counter, mutation and retry receipt at one local atomic boundary.
      Two handles on the same coherent store must never reuse an author dot.
- [ ] Verify all five runtime writers/readers against the same native SQL-free
      physical LSM directory, with explicit close/reopen and owned snapshots.
      Use real process exits and before/after publication faults; preserve
      existing directory ownership and unresolved-outcome cleanup guarantees.
- [ ] Add the separately selected ordinary-links projection/session facade.
      Retain concurrent versions beneath explicit rejection or deterministic
      projection; do not imply global uniqueness, cascades or serializability.
- [ ] Run complete suites/coverage and fresh exact-head platform CI, preserving
      every production denominator and actual coverage archives. Record full
      replay/publication cost and each finite admission bound.

The draft retains an append-only local journal: an author header, local writes
with caller-supplied retry identifiers and observed versions, and canonical
received-version batches. Full replay reconstructs both the global local-author
counter and retained historical identities. Tokens return the original result
after an uncertain reply and later edits; changing a token's request rejects.
CAS errors never trigger a blind mutation retry. Duplicate known deliveries
perform no publication. Retained deletions prevent stale-state resurrection.

This initial design has explicit finite capacity (up to 4096 entries and 8 MiB
of canonical ASCII records). A local write copies and publishes the selected
journal; replay retains historical identities and retry receipts. Capacity
exhaustion rejects before CAS. Compaction that discards those receipts or author
evidence would invalidate the retry/contradiction guarantees and needs its own
verified design. These costs belong only to the selected component.

Author uniqueness remains a deployment assumption across independent stores.
The local CAS fences publication on one coherent lineage; it cannot fence an
active offline copy. Foreign unknown dots from the local author, or causal
contexts beyond its durable counter, reject. Reusing an author after restoring
an older backup is unsafe without an external fence or a new identity. External
deletion/replacement of the reserved key is outside the store contract.

Bounded Merkle snapshots/range transfer, automatic safe reclamation, global
constraints, unbounded replay/throughput and all wider unmet criteria remain
unfinished. Native SQL-free composition must be verified before this experiment
can establish any durability result. PR139 remains draft and unmerged.
