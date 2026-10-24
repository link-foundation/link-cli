# Observed cascade journal failure audit

Continue R1 with actual native uniqueness/usages decorators, three baseline
rows, a two-row cascading delete, and a one-shot journal fault before or after
its first or second transition append. At most four rows are ever live.

- [x] Reproduce loss of rollback information after a partial journal append.
- [x] Verify all observed undo records survive first/second append failures.
- [x] Verify incomplete explicit transactions reject commit and further writes.
- [x] Roll back automatic operations on journal failure when cleanup succeeds.
- [x] Verify admission, subsequent commits and persistent reopening.
- [x] Verify failed rollback markers keep the transaction pending until retried.
- [x] Maintain Rust/C# parity and add release/documentation evidence.
- [x] Run focused/full suites, coverage and formatting.
- [ ] Publish and inspect exact-SHA CI.

An append error can have an unknown physical outcome. Tests cover both outcomes;
never retry an unknown append as a fresh logical write. Failed undo or rollback
markers must remain errors rather than claiming a completed rollback. These
in-process tests do not prove crash atomicity for unlogged in-place mutations.

The initial Rust fixture also exposed missing mapped-store cascade observations:
its second-append fault did not fire. Two isolated no-fault tests reproduce this
for deletion and uniqueness merging. Forwarding upstream callbacks fixes the
merge, and reveals that restoring a higher address allocates lower empty slots;
delete undo must restore contents even when such a slot already exists. After
both corrections, the journal suite genuinely reaches either append and has
10 failing cases and two passing cases before journal error handling changes.

The original C# fixture observes both affected addresses and has four failing
commit-rejection cases, with eight passing rollback cases. Its journal recorder
now stages the entire observed batch before writing, just as Rust does.
An append error makes the transaction rollback-only; automatic work attempts
rollback immediately. C# additionally exposes Rollback() on the decorator so
callers can retry failed cleanup of an implicit transaction without a handle.

Fixtures cover before/after physical append outcomes, single-row creates and
updates, Rust explicit-address allocation, a two-row cascade, manual rollback,
and rollback-marker failure. Subsequent writes and reopening are checked where
applicable. None of these fixtures exercises process crashes or silently mutated
storage that violates the observer contract.
