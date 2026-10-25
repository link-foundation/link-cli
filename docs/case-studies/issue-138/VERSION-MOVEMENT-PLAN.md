# Recoverable legacy checkout and branch switching

R1/R3/R7/R8 require a history movement to retain ownership across row, metadata
and persistence failures. Existing checkout preflights retained history, but a
failure after replay can admit a following write against an unpublished position.

- [x] Reproduce checkout and branch-switch failures before/after the applied
  marker, and absent persistence, in Rust and C# before changing production code.
- [x] Persist a validated, portable movement intent containing the resolved undo
  and redo before changing rows. Keep changed-history rather than database-size
  state; retain no cost in the bare store.
- [x] Fence both the version facade and public inner replay/writes/recovery until
  completion; serialize movement against asynchronous checkpoints.
- [x] Flush replayed data before publishing the new branch/applied marker, flush
  metadata before removing the intent, and retry every uncertain phase.
- [x] Recover interrupted movement on reopen, including after retained records
  disappear; reject contradictory/corrupt intents before changing data.
- [x] Cover row faults, repeated cleanup faults, real process exits, portable
  wire vectors, following edits, and exact repeated reopening. Run shared Rust
  contracts on actual WASM.
- [x] Run full local validation, review the diff and update protocol/release notes.
- [ ] Update PR, commit and push to the prepared branch, then verify exact-head CI.

Legacy reads expose the wrapped live rows. A failed movement has an uncertain
outcome until retry/reopen completes; this work must not imply isolated sessions
for that API. The staged engine supplies separate snapshot sessions.

The focused C# suite passes 102 cases. Shared native Rust contracts pass 34
movement failure schedules and 25 recovery scenarios; three parent tests verify
16 original process exits and 24 recovery interruptions. Full native Rust (733),
C# Debug/Release (1,573 each), actual WASM (404), JS (106), types and lint checks
pass. Prior publication fault tests pass. Complete project coverage is below
100%; exact metrics and before logs are in version-movement.txt. Fresh
published-head CI remains pending.

Recovery tests first reproduce sequence reuse after payload eviction and C#
sub-millisecond timestamp comparisons against the millisecond wire format.
Additional before logs reproduce accepted duplicate/malformed metadata and
broken ancestry. Twelve C# constructor-failure tests first reproduce an
unreachable cleanup owner; the inner transaction now retains its retry callback.
