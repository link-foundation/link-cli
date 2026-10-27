# Local causal publication and retry ownership

This continues the [anti-entropy plan](ANTI-ENTROPY-PLAN.md). The separately
selected version codec and local-owner ports are in production source. Portable
contracts and native filesystem composition pass; full coverage verification
is in progress. Do not
describe the local owner as a complete anti-entropy transport.

- [x] Create eight JS contracts before the new production API; the absent
      module fails independently. Exercise coherent local CAS, two handles,
      stale observations, before/after publication errors, durable retry
      identifiers, restart, concurrent deletion and historical contradictions.
- [x] Independently generate 26 bounded local-journal cases without importing
      a production codec. Include owner binding, exact canonical bytes, absent
      versus erased state, global author-counter progression, received context,
      historical equivocation, duplicate tokens and admission limits. Three
      later vectors cover retained dominated observations, rejection of old
      equivocation after reopening, and canonical evidence order (29 total).
- [x] Run all eight contracts against a JS draft outside production source.
      This is experiment evidence. Rust/C# drafts also pass the 26 shared
      cases, retry/reopening and deletion controls. Real native durability and
      production parity remain separate unchecked steps. JS module coverage
      reaches 100% lines, branches and functions without reduced thresholds.
- [x] Port the contracts to native Rust/C# and actual WASM. Missing production
      APIs fail before implementation (Rust E0432, C# CS0246 and JS missing
      module); all eight contracts and 29 independent shared cases now pass
      across native Rust, C#, Node, Bun, Deno and actual WASM.
- [x] Add the selected production local-owner layer over AtomicJournalStore.
      Reserve one key for one author lineage; read every operation afresh and
      publish counter, mutation and retry receipt at one local atomic boundary.
      Two handles on the same coherent store must never reuse an author dot.
- [x] Reproduce malformed framing reaching JSON.parse and a contradictory
      dominated observation disappearing before history validation. Validate
      complete evidence before normalization and retain it through replay.
      The eight latest JS contracts pass at 100% module coverage; native/C#/
      actual-WASM focused contracts verify the correction as well.
- [x] Verify all five runtime writers/readers against the same native SQL-free
      physical LSM directory, with explicit close/reopen and owned snapshots.
      Use real process exits and before/after publication faults; preserve
      existing directory ownership and unresolved-outcome cleanup guarantees.
      All 50 plain/mirrored handoffs and 20 reaped process exits pass, including
      100 stale-owner refusals with unchanged bytes and 100 recovered
      cross-runtime retry/reopen/following-edit checks. Recovery explicitly
      quarantines stale markers after reaping; it never steals a live owner.
- [ ] Add the separately selected ordinary-links projection/session facade.
      Retain concurrent versions beneath explicit rejection or deterministic
      projection; do not imply global uniqueness, cascades or serializability.
- [x] Run complete local suites/coverage, preserving
      every production denominator and actual coverage archives. Record full
      replay/publication cost and each finite admission bound. The sequential
      run finishes 2026-10-27T14:21:34Z: 1210 native tests plus six doctests,
      736 actual-WASM tests, 2475 C# tests in each configuration, 657 JS tests
      at enforced 100% production coverage and 221 repository checks pass.
      causal-replica.txt retains complete metrics, including uncovered native,
      linked-WASM and C# branches. No denominator was removed.
- [ ] Finish fresh exact-head platform CI. Published d00e022 passes all three
      C# platforms at 2475 tests each; actual archives verify size and SHA256 in
      causal-replica-csharp-platform-coverage.txt. Eight workflows pass; Rust's
      Windows job is pending when the local receipt is recorded.

The selected implementation retains an append-only local journal: an author header, local writes
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
unfinished. Application-crash probes establish the tested native SQL-free
composition, without certifying hardware power loss. PR139 remains draft and
unmerged.
