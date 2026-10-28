# Physical author-fence handoffs

Extend R1/R4/R7/R8/R9 verification of the selected author guard after its
portable contract tests. A fake atomic provider cannot establish that native
physical stores preserve the fence comparison or cross-language claim bytes.

- [x] Preserve a missing-recipe baseline from the focused physical script.
- [x] Add Rust, C# and JS recipes over the actual SQL-free FileLsmJournalStore.
- [x] Write a first owner and guarded two-key group; reopen without rewriting.
- [x] Take over from another runtime, reject old reads/publications/retries,
      publish a new two-key group and preserve the fence generation on data CAS.
- [x] Require exact retained active-token retries and complete owned snapshots.
- [x] Exercise plain and fully mirrored directories; hash every physical file
      before/after reads, refused operations and read-only CAS.
- [x] Run two focused local Rust -> C# -> Node -> Rust/C# handoffs; wire all
      fifty five-runtime handoffs to full CI. Keep all child processes finite,
      apply local memory/stack budgets and reap before transferring ownership.
- [x] Commit/push and retain fresh exact-head CI, including the prior Windows
      socket-admission and new dependency-release failures.

All nine workflows pass on `9ece2d1`. The JS run
[40704187896](https://github.com/link-foundation/link-cli/actions/runs/40704187896)
verifies all fifty physical fence handoffs. `author-fence-followup-ci.txt`
retains exact raw lines, complete report counters and artifact digests.
Later process and portable coverage changes require their own fresh CI.

These are real clean-close/reopen handoffs. They do not certify hardware power
loss, hostile bypass writers, automatic takeover permission or receipt pruning.
