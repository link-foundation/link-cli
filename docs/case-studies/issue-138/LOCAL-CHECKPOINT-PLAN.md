# Optional atomic replacement of local page journals

Requirement mapping: R1/R3/R5/R6/R7/R8/R9/R10/R11/R12. The entire acceptance
inventory remains active; persisted checkpoint imports alone do not implement
replacement of an acknowledged journal.

- [x] Trace native devices, ownership, page metadata and logical export. Keep
      ordinary log/page formats and default costs unchanged.
- [x] Define an independent two-bank root protocol and finite shared vectors.
- [x] Capture missing-feature regressions in Rust, C# and JS before adding
      production replacement code.
- [x] Add an optional ordered-log replacement capability and two fixed page
      banks. Prepare the inactive bank completely before synchronizing an
      alternating root decision. Bind each decision to its exact bank epoch.
- [x] Independently model failures before and after every reset, page, metadata
      and root operation. Reopen actual durable images; retain complete old or
      complete new records and fence uncertain owners.
- [x] Verify malformed roots, epoch exhaustion, pending input, no-op replacement,
      stale bank identities, repeated swaps, resource ownership and finite costs.
- [x] Add engine replacement under the commit lock, optional physical delta
      composition and matching WASM/JS binding. Preserve history, pins, active
      reservations and sessions; reject unsupported capabilities before mutation.
- [ ] Add native file ownership/cleanup, bounded process exits and persistent
      cross-provider handoffs with following groups in normal CI.
- [x] Run complete portable suites and measured coverage sequentially within
      2.92 GiB; preserve before failures, ownership rules, selected costs and
      component/whole-project coverage in local-checkpoint{,-before}.txt.
- [x] Commit/push this portable batch only to the prepared branch and inspect
      every fresh exact-head workflow and native platform. Engine/file changes
      need their own following complete validation.

All nine workflows and both native language platform matrices pass on c66e75f;
`local-checkpoint-ci.txt` preserves exact heads, creation timestamps and links.
The intervening dependency update and SDK-matching C# formatting correction
retain their original failed logs and separate forward commits.

Engine integration and complete local verification are recorded in
[ENGINE-COMPACTION-PLAN.md](ENGINE-COMPACTION-PLAN.md) and engine-compaction.txt.
That following batch needs its own exact-head CI. Native owning factories and
process/persistent replacement checks remain in the unchecked step above.

The root uses two fixed metadata files and two complete page banks. No file
name changes during replacement. All devices stay under one exclusive owner;
internal bank paths must not be used independently. Retained memory snapshots
do not hold old file readers. A new bank identity is deterministically bound to
the root identity and monotonically increasing epoch, so an older decision
cannot silently open a bank reused by an interrupted later replacement.

Successful synchronization is an explicit device assumption. Process exits
cannot certify hardware power loss. Alternating decisions provide interrupted
publication recovery; they do not promise repair of arbitrary post-acknowledgement
media corruption. Root redundancy/repair, legacy mapped repair/page undo,
on-disk LSM, old distributed object reclamation, whole-project coverage and all
wider criteria remain unfinished until independently implemented and verified.
