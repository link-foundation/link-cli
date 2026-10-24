# Version history at the signed sequence boundary

Requirement mapping: R1/R3/R8. The legacy journal already rejects exhausted
sequence space, but its C# version decorator increments an inclusive signed
range after the final number. Initial history also creates markers for every
integer up to the last logged sequence, including gaps in valid sparse journals.

- [x] Trace attribution, initial history, replay and native exhaustion controls.
- [x] Reproduce final-number publication through automatic and explicit writes,
  plus first version construction over a tiny sparse journal. Stop probes after
  eight marker writes or the first nonpositive sequence; never traverse an
  unbounded numeric range or allocate an unbounded number of mapped rows.
- [x] Publish actual recorded transitions without signed wraparound or phantom
  gap markers. Preserve branch/head/tag, checkout, recovery and terminal admission.
- [x] Add public version API edge contracts for cancellation, rejected metadata
  and transaction state, using existing public entry points rather than reflection.
- [x] Run focused and full Debug/Release, coverage, format and release checks.
- [ ] Commit/push, review fresh exact-SHA CI, and preserve any failures.

The fixtures add one valid committed/applied no-op transition at long.MaxValue
or one less, rather than executing an impractical number of operations.

All three C# cases fail against the published implementation at the finite
marker guard. The corrected creation fixture captures the callback address:
primitive Create returns a handler status, which was initially mistaken for
the allocated address. Rerunning that corrected fixture against the old loops
again fails all three cases. The loop correction passes all 78 focused version
contracts, including cancellation, metadata rejection and named forwarding.

Rust's inclusive final-number iteration already terminates, but its initial
history also invents gap versions. A finite sparse sequence 100 fixture first
fails because Checkout(3) succeeds despite there being no third journal record.
After iterating actual retained records, the sparse regression and automatic/
explicit final-number controls pass, along with the existing version/sequence
targets. These are legacy native APIs; the staged JS/WASM history is unchanged.

Full C# Debug/Release each pass 981 tests. Native Rust passes 561 standard
tests plus the custom stdout harness, with four child entry points ignored.
Official coverage and checks are recorded in
`dev/log/issues/138/pulls/139/version-sequence-boundaries.txt`.
