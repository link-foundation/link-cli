# Transaction identity after clock rollback and process restart

R1/R7/R8 require a new legacy transaction to remain distinct from recovered
commit and abort decisions. Rust currently combines wall-clock milliseconds
with a counter that restarts at zero. Two finite Linux probes reproduce row 21
after reopening where an aborted/uncommitted update should leave row 11.

- [x] Reproduce actual process restarts with frozen realtime, keeping monotonic
  deadlines intact, finite inputs and bounded child memory/stack.
- [x] Add automated native restart and actual-WASM collision contracts before
  changing production allocation. Preserve the behavioral before logs.
- [x] Reject IDs already present in recovered commit/abort decisions, covering
  automatic and explicit owners without changing wire or public APIs.
- [x] Verify repeated reopening, following commits, existing clocks, corruption,
  rollback and publication contracts. Review cost and legacy compatibility.
- [x] Run applicable full tests, coverage, lint and release checks.
- [x] Commit/push only the prepared branch, update PR 139 and verify exact-head CI.

Keep the existing recovered decision sets as the uniqueness index rather than
adding a database scan or mandatory random-number dependency per transaction.
The raw cache path must incur no new work. IDs remain journal-local identities;
this audit does not promise cryptographic/global identity or repair journals
already written with conflicting reused decisions.

Four native parent contracts first fail with a genuinely fresh process and a
frozen CLOCK_REALTIME; monotonic ten-second deadlines remain live. Every child
has a 512 MiB address-space and 8 MiB stack limit. Tests cover prior commit/abort
decisions followed by explicit rollback or an exit after flushing pending rows,
two reopenings and a following commit. Four actual-WASM cases first fail using
an imported decision and a frozen Date.now, covering explicit/automatic owners
and committed/aborted IDs without resetting a production counter or adding APIs.
All eight focused cases now pass, as do both original finite restart probes.
Initial fixture compile errors are excluded from behavioral before evidence.

The allocator rejects zero and known decisions using existing hash sets. Normal
candidate checks cost expected O(1); a restarted prefix of K collisions costs
O(K), with the process counter advancing past those candidates. There is no
extra retained index, journal scan or cost on the bare store. A default-off
trace reports that historical IDs were skipped once per affected allocation.
Full validation passes: 737 native Rust, 408 actual-WASM, 1,573 C# per
configuration and 106 JS tests, types, formatting, strict lint and release checks.
Full project coverage remains below 100%; exact metrics and preserved before
logs are in transaction-identity.txt. All nine workflows pass on published
identity head 6d231fa; see transaction-identity-ci.txt. Later code needs fresh CI.
