# Legacy journal sequence exhaustion

Continue R1/R8 with finite valid journals at the signed sequence boundary.

- [x] Prepare tiny journals containing one committed/applied no-op boundary record.
- [x] Reproduce native automatic/explicit creation and cascade failures first.
- [x] Reject already-exhausted writes before mutating data.
- [x] Reserve sequence capacity for the complete observed batch; retain all undo
      state if a cascade exceeds the remaining capacity.
- [x] Verify commit/write rejection, rollback, subsequent admission and reopening.
- [x] Run full native coverage, formatting and actual WASM checks.
- [x] Verify exact-SHA CI after publication.

Fixtures start with three data rows. The boundary is i64/long MAX for one
creation, MAX-1 for a two-row Rust cascade, or MAX-3 for C#: its resolver
first dispatches a nested update and delete, followed by the two-row callback
batch. This uses three boundary journal appends rather
than attempting an impractical number of writes. Ephemeral rollback state must
not become an invalid serialized transition. The legacy crash window before
an observed batch is logged remains a separate documented limit.

All four Rust cases first panic at unchecked sequence addition. All four C#
cases first accept a write despite insufficient signed sequence space. The fix
preflights admission and complete batches. When an observed cascade does not
fit, all of its undo records stay in pending memory, with no assigned numbers
or global-history/journal publication. Automatic cleanup runs immediately;
explicit transactions reject commit and further writes until rollback.

C# fixtures call the primitive Create/Delete overloads directly, matching the
observed-batch contract. The convenient Create()/Delete(address) extensions can
perform multiple decorator calls; they are not a single callback batch. Initial
post-fix probing exposed that distinction, so the corrected primitive fixtures
are rerun against the previous implementation before verifying the fix.

Verbose C# tracing identifies the nested resolver update and delete even when
the primitive Delete overload is used. Their valid assigned numbers remain consumed after
rollback. The final fixture leaves one number after those writes and rejects the
two-row batch without numbering any part of that batch. It checks the valid
preceding history entries, exact baseline restoration, the remaining successful
single write and persistent reopening. The diagnostic remains opt-in.

The corrected C# MAX-3 fixtures were rerun against the previous source: all
four fail because the write is accepted. All four pass with the fix. Rust
focused sequence/cascade/creation and undo suites pass. The full Rust suite
passes 536 standard tests plus the stdout harness; C# Debug and Release each
pass 871 tests. All 17 actual WASM tests, native/WASM Clippy and formatting
checks pass. Exact coverage counts are recorded in VERIFICATION.md.
