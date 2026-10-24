# Observed partial-write audit

Continue requirement R1's deterministic storage fault audit after explicit-address
writes. A storage adapter can emit callbacks and then return an error. Repeated
callbacks for a new address must also preserve its original absent state.

- [x] Reproduce failed update/delete after observed mutation in Rust.
- [x] Reproduce failed create/update/delete after observed mutation in C#.
- [x] Verify automatic rollback and admission release; preserve explicit ownership.
- [x] Verify successful repeated create/update callbacks rollback to absence.
- [x] Preserve the storage error and journal partial effects before cleanup.
- [x] Test following writes, persistent reopen and both normal/error outcomes.
- [x] Run native/WASM suites, coverage, Clippy and formatting.
- [ ] Publish and inspect exact-SHA CI.

Use at most three live data rows per fixture, no stress loops. Custom adapters
implement the existing callback contract; additionally inspect ordinary user
callback failures on the real C# store. Do not claim panic safety for Rust's
unwind path or recoverability for adapters that mutate without any observation.

Six Rust cases fail on baseline `b163326`: failed observed updates/deletes in
automatic and explicit transactions, a failed cascade, and successful repeated
create/update callbacks followed by rollback. Fifteen C# cases independently
fail for the same contract, including real user callback failures. A first C#
fixture incorrectly treated Create's continuation result as an address; it was
corrected to capture the created address from the callback before collecting
the 15 assertion failures. No invalid-address probe is retained as a test.

The fix journals observations even when the inner call fails, then rolls back
automatic work. Explicit transactions keep those observations for caller rollback.
A null initial before-state is retained as absence, never replaced by a later
intermediate row. Journal/checkpoint failures retain their own error semantics;
this does not establish crash atomicity for stores that mutate without callbacks.

The expanded 21-case C# focused suite passes after removing an unsupported
synchronous-test Timeout attribute. Six added cases check errors before mutation
for create/update/delete and both transaction ownership modes. CI job-level
deadlines remain in place.
