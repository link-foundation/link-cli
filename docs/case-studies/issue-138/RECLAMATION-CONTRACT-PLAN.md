# Reclamation contract coverage audit

Objective: close concrete untested public behavior identified in the complete
reclamation coverage reports, preserving the wider issue scope in PR139.
This is a test audit of published source `419f7c7`; no production failure or
production change is presumed.

- [x] Publish the reclamation implementation only after full local validation.
- [x] Start all nine exact-head workflows and retain their timestamps/SHAs.
- [x] Inspect complete reports for uncovered construction, routing, durability
      and interrupted multi-step I/O behavior.
- [x] Add matching bounded Rust/C#/JS public contract controls for empty/invalid
      partition configuration, full pre-I/O route validation, read-only routing,
      all eight durability capability combinations, failed initialization read,
      failed staging confirmation and failures during whole-group refresh.
- [x] Run focused controls; investigate actual failures before changing source.
- [x] Run complete native/C# Debug+Release/actual-WASM/JS coverage suites and
      relevant repository checks; retain full denominators and original evidence.
- [x] Complete the preceding exact-head CI investigation, preserving any failed
      logs and correcting concrete failures.
- [ ] Review, commit and push only the prepared branch, then verify new exact-head
      CI. Update PR evidence and continue the wider unfinished requirements.

The seven new native Rust and seven Node controls pass on the published
implementation. The focused C# class passes all 45 tests, including the seven
new controls. No production failure was reproduced and no production source
change is needed for these contracts. The complete sequential driver is
`experiments/issue-138/validate-reclamation-contracts.sh`; its reports have
their own prefix so the preceding production/integration evidence is preserved.

The tests check observable failure boundaries: invalid routing sends no request;
read-only dependencies need no route; capability queries send no request; a lost
initialization confirmation leaves one acknowledged epoch without replay; a lost
staging confirmation leaves an orphan that collection removes; and interrupted
catalog/object refreshes throw before returning a group, followed by a coherent
fresh read. Native Rust contracts also run in actual WASM.


The complete sequential driver passes from 2026-10-26T22:28:46Z through
22:53:33Z with 1,064 native Rust tests plus six doctests, 663 actual-WASM
tests, 2,314 C# tests in each configuration, 525 JS tests, 166 selected tests
in each of Bun/Deno and 221 repository checks. Complete coverage denominators
are preserved in `reclamation-contracts.txt`; native/C#/linked-WASM coverage
still falls below the whole-project target. Formatting, strict Clippy,
TypeScript, JS 100% coverage thresholds, SQLite optionality, source limits,
changesets, actionlint and ShellCheck pass. No OOM event occurs. Published
production source is unchanged; its separate real-etcd and nine-workflow
results remain in `reclamation.txt` and `reclamation-ci.txt`. New exact-head
CI must verify this test batch after publication.
