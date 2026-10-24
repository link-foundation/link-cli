# Recovery after a noncontiguous applied checkpoint

Requirement mapping: R1/R7/R8. This audit follows the checkpoint-owner fixes.
C# asynchronous commits intentionally release admission after journal durability.
A failed older background checkpoint can leave an applied-marker gap before a
later, successfully applied commit. Replaying only the gap can overwrite or
resurrect data from that later commit. Older Rust journals or external log
implementations may contain the same valid marker pattern.

- [ ] Reproduce with a real failed asynchronous C# checkpoint followed by a
      synchronous update, delete or recreation; verify two persistent reopens.
- [ ] Reproduce the same logical journal in portable Rust/native/WASM contracts.
- [ ] Retain sequence ordering across the whole committed suffix after a gap.
- [ ] Inject recovery failure before/after redo, final data flush, applied markers
      and log flush; preserve a durable recovery anchor until the final state
      is flushed so a second interrupted recovery cannot lose later edits.
- [ ] Preserve pending-transaction undo, rollback markers and legacy sidecars.
- [ ] Run full suites, coverage, strict checks and fresh exact-SHA CI.

The first-failure evidence and observed limits must be recorded before changing
replay. Tests use finite schedules, bounded worker gates and distinct durable
images where faults model a restart. Passing in-process cleanup does not prove
power-loss persistence outside the storage adapter's flush contract.
