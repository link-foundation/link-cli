# Recovering selected-address protocol work

The [latest feedback](https://github.com/link-foundation/link-cli/pull/139#issuecomment-6467849231)
requires backward compatibility and asks that complete suites run in CI, with
only focused local checks. This supersedes the earlier local full-suite recipes.
The R1–R12 inventory remains the acceptance checklist; no required sub-issues,
inline PR comments or reviews were returned on the paginated refresh.

- [x] Download both parts of the interrupted public log. Part 00 alone ends
      before the C# work; part 01 contains the final interruption and saved work.
- [x] Recover commit `2dff50de` from the recovery branch by fast-forward,
      preserving history. Its parent is the published checked-query head.
- [x] Read the issue, all conversation feedback, requirements and the C# and
      physical-query parity plans. Verify the default branch is an ancestor.
- [x] Review the restored protocol algorithms and established uint wrappers;
      retain original signatures, wire shapes and existing query behavior.
- [x] Rerun the focused causal-query, generic-operation and existing protocol
      contracts with bounded .NET build concurrency and heap.
- [x] Supply stable focused validation scripts referenced by the recovered
      gate, run formatting and cheap repository/release checks, and record
      exact commands and limits. Complete suites and coverage belong in CI.
- [x] Extend the existing physical session examples to exercise ordinary
      selected-address protocol operations and original-text queries. Retain
      exact expected versions, publication tokens and crash-recovery requests.
- [x] Assert owned observations, private saves, preparation freezes and
      unchanged provider bytes before explicit publication in each port.
- [ ] Run a small local physical check where compiled tools are available;
      run complete five-runtime handoff and finite process-exit matrices in CI.
- [ ] Commit each useful step, push only the prepared issue branch, inspect
      exact-head runs and preserve any nonpassing logs with line references.
- [ ] Update PR title/body, requirements and verification evidence. Review the
      diff and clean tree; mark ready only after all acceptance work is done.

The final log reports model capacity exhaustion at 2026-10-28T00:48:22Z;
it does not establish a database defect. It records a successful intermediate
219-test focused run followed by further edits. That earlier run does not
verify the final recovered bytes, and no complete final suite is claimed.
Existing interruption reports to OpenAI remain linked in SESSION-RECOVERY.md.

Broader unresolved criteria in REQUIREMENTS.md remain active, including full
native/C#/linked-WASM coverage, generic mapped failure safety, physical power
loss, remaining competitor algorithms and cluster allocation/scaling work.
The pull request stays unmerged until those criteria are addressed.
