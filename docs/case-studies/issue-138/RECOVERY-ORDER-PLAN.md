# Recovery after a noncontiguous applied checkpoint

Requirement mapping: R1/R7/R8. This audit follows the checkpoint-owner fixes.
C# asynchronous commits intentionally release admission after journal durability.
A failed older background checkpoint can leave an applied-marker gap before a
later, successfully applied commit. Replaying only the gap can overwrite or
resurrect data from that later commit. Older Rust journals or external log
implementations may contain the same valid marker pattern.

- [x] Reproduce with a real failed asynchronous C# checkpoint followed by a
      synchronous update, delete or recreation; verify two persistent reopens.
- [x] Reproduce the same logical journal in portable Rust contracts.
- [x] Execute the shared new contracts in actual WASM.
- [x] Retain sequence ordering across the whole committed suffix after a gap.
- [x] Inject recovery failure before/after redo, final data flush, applied markers
      and log flush; preserve a durable recovery anchor until the final state
      is flushed so a second interrupted recovery cannot lose later edits.
- [x] Preserve pending-transaction undo and rollback markers in restart tests.
- [x] Reproduce sized/chunked retention over the same older checkpoint gap;
      checkpoint current inline effects without replaying stale state.
- [x] Preserve legacy version/sidecar contracts in the complete suites.
- [x] Run full local suites, coverage and strict checks.
- [ ] Verify fresh exact-SHA CI after publishing this batch.

The first-failure evidence and observed limits must be recorded before changing
replay. Tests use finite schedules, bounded worker gates and distinct durable
images where faults model a restart. Passing in-process cleanup does not prove
power-loss persistence outside the storage adapter's flush contract.

Before: 39 C# cases fail (three real async histories, 36 mapped recovery
interruptions); four Rust contracts fail and two delete controls pass. After
ordered recovery, the same 39 C# and six native Rust contracts pass. Rust
interruption tests execute 32 finite schedules from distinct durable images.
No applied marker is published until the complete final state has been flushed.
Six additional C# retention cases fail before the live checkpoint change while
the 39 ordered recovery controls pass. Live retention now flushes the current
inline state before marking older records, avoiding stale redo. All 45 focused C# cases and six portable Rust contracts pass. Full C#
Debug/Release (1,151 each), native Rust (607 plus stdout harness), and actual
WASM (321) pass, along with strict lint, formatting, metadata and file sizes.
Exact measured coverage is retained in recovery-order.txt under the PR evidence;
project-wide 100% coverage and fresh CI remain pending.
