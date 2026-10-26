# Native ownership for checkpoint journals

Requirement mapping: R1/R3/R5/R6/R7/R8/R9/R10/R11/R12. The engine publication
batch is committed as 133c4ff; this following batch owns its fixed native files.
The complete acceptance inventory remains active.

- [x] Run missing-feature controls in actual Rust, C# and JS targets. Require
      the absence of the factory itself, preserving complete error logs.
- [x] Acquire the raw root and every single/mirrored bank before portable-log
      validation, repair or reset. Keep SQLite optional and native JS imports
      in a separate package entry. Preserve existing default journal paths.
- [x] Close every acquired raw device in reverse order, root last, on success
      and every constructor failure. Preserve original and all cleanup errors.
      Never flush pending work on close; make repeated close harmless and
      reject operations after closing.
- [x] Retain native writer fences after uncertain close. Even after root
      release, the next factory must acquire every bank before any recovery
      mutation. Verify exact preserved bytes, all handles attempted and no
      foreign-owner stealing with finite native filesystem tests.
- [x] Exercise finite process cuts before and after native writes, truncation,
      bank synchronization and root publication. Observe child termination
      before explicitly removing its writer fences. Reopen complete old/new
      records, then append, flush, compact and reopen again. Bound child input,
      memory and reaping time. Process exits do not certify hardware power loss.
- [x] Extend the existing 48-configuration Rust → Node → Bun → C# → Deno → Rust
      handoffs with selected factory compaction, pins, branch bases, exact
      names/allocators and following groups. Retain both existing controls and
      add closed selected-bank mirror damage/repair in normal CI.
- [x] Run full suites and measured coverage sequentially within the container
      budget; document fixed paths, upgrades, ownership and selected costs.
      Preserve complete reports and uncovered scope without reducing denominators.
- [ ] Review local/PR diffs, refresh main/comments, commit only to the prepared
      branch, update the PR and inspect all exact-head workflows/native platforms.
      Keep the PR draft and every wider unmet criterion active.

Rust retains independently owned raw leases through all fallible construction,
so portable value destruction cannot hide cleanup errors. A selected native
lease adds a small mutex per I/O; ordinary logs have no added work. C#/JS retain
their raw device references explicitly. Mirroring decorates bank I/O; ownership
cleanup closes each raw handle owner and never relies on repair-wrapper disposal.

Missing-feature controls failed at 133c4ff in the actual Rust, C# and JS targets.
The existing JS constructor-cleanup regression separately failed because the
original acquisition error was hidden by cleanup. Public before evidence is
in native-checkpoint-before.txt; minimal fixtures and the guarded baseline
reproducer remain under experiments/issue-138.

All three independent persistent controls pass 48 configurations and 288 groups
each: ordinary journals, imported initial checkpoints and native replacement.
Replacement covers epochs one through six and closed mirrored-bank damage.
Complete suites pass 1,013 native Rust tests plus six doctests, 620 actual-WASM,
2,227 C# per configuration, 239 JS at enforced 100% lines/branches/functions and
221 repository checks. All three real-etcd modes and runtime comparisons pass.
The new C# factory measures 100% lines/branches; Rust's factory measures 214/221
lines, 5/6 branches and 57/58 functions. Whole-project coverage remains below
target. See native-checkpoint.txt for complete denominators and saved reports.
A live edit of the shell driver disrupted its final C# success message after
both test commands exited zero. The original driver error is retained; both
complete reports and coverage attachments were verified without repeating them.
Strict Rust lint also found a range-loop style error in the new process test;
it was corrected to enumerate the observed operations. No guarantee is weakened.

A following combined directory synchronization/close fault also reproduces
masked original errors in JS. Four finite combinations now retain both original
error identities and every raw cleanup error, preserve acknowledged records and
verify following reopening. The complete nine-case raw fault suite and final
complete JS validation pass; native Rust/C# source and their checks are unchanged.

The published ownership head `600cf5f` has eight passing workflows and a C#
Windows failure. The new recovery fixture compared exclusive bank files before
its owner closed. Its byte comparison now follows disposal and a new owner
performs subsequent writes. A separate existing async probe reports a responsive
canary without checkpoint progress; retain it and add independent controls and
trace diagnostics before drawing a root-cause conclusion. See
native-checkpoint-ci-before.txt for run timestamps, permanent URLs and exact log
lines. Earlier all-pass CI does not validate this head.
