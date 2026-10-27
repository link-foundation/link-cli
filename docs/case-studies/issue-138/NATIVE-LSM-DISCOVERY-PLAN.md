# Native LSM discovery and ownership

Objective: open a physical file namespace without repairing, truncating,
initializing or retiring files before the complete selected LSM decision has
been validated. This continues the unfinished native portion of
[the physical composition plan](PHYSICAL-LSM-PLAN.md).

- [ ] Reproduce the missing read-only checkpoint inspection API in Rust, C#
      and JavaScript before implementing it. Keep the finite independent
      inspection experiment as an additional oracle.
- [ ] Add matching bounded inspection APIs: Rust
      `CheckpointPagedLog::inspect(&mut root, &mut banks, max_pages)`, C#
      `CheckpointPagedCommitLog.Inspect(root, first, second, maxPages)` and
      JavaScript `CheckpointPagedCommitLog.inspect(root, banks, options)`.
      Return owned records without allocating a database identity or completing
      any repair. Do not change the existing opening constructors.
- [ ] Verify empty/partially initialized devices, selected bank identity,
      conflicting metadata, complete page chains, ignored uncommitted suffixes,
      finite limits and staged mirror repairs. Run shared Rust controls in
      actual WASM, and preserve complete coverage denominators.
- [ ] Implement exclusive outer directory ownership before acquiring any WAL
      or table handle. Reject stale fences and foreign, malformed or symbolic
      names; never steal another owner's fence.
- [ ] Bound directory enumeration before collecting it. Check that every
      selected physical data/metadata file already exists and is regular before
      calling a lower-level device constructor that can create missing files.
- [ ] Validate the inspected semantic WAL and all selected tables through a
      read-only WAL and namespace view. Suppress repair completion and orphan
      deletion in this validation pass. Close its temporary table leases before
      opening the ordinary checkpoint WAL and actual physical store.
- [ ] Handle a completely uninitialized WAL only with an empty namespace.
      Initialize after discovery succeeds; a nonempty unrooted bank is corrupt.
- [ ] Synchronize successful native file creation and retirement according to
      each platform's supported contract. State the exact guarantee separately
      from file-content flushing and from physical power-loss certification.
- [ ] Close all child handles before releasing the outer fence. Retain the
      fence on uncertain cleanup; do not retry an unsafe raw descriptor close.
- [ ] Verify finite before/after mutation cuts, actual process exits, missing
      selected files and preservation of corrupt evidence before failure.
- [ ] Run complete native Rust/C#/Node/Bun/Deno whole-store file interchange,
      verify read-only file hashes and demonstrate real cluster consumers.
- [ ] Run local gates, review the complete diff, publish an atomic verified
      step and inspect the resulting exact-head CI before updating evidence.

The extra validation pass performs bounded reads twice under the same exclusive
outer owner. It prevents lower-level WAL recovery effects from preceding LSM
semantic validation. It does not promise protection against an unrelated actor
modifying files outside the ownership contract. The existing generic physical
store and lower-level file providers alone do not implement this facade.
