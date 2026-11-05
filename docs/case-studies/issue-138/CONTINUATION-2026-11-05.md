# Issue 138 continuation — 2026-11-05

Objective: implement and verify every acceptance criterion of issue #138 in PR #139. The goal stays active until the R1–R12 inventory is satisfied. Passing CI or reporting an upstream defect does not complete an unmet criterion.

## Requirements and verification plan

- [x] Read issue #138, all issue comments and PR conversation, inline comments and reviews. Refresh the sub-issue API: no attached sub-issues.
- [ ] Merge current default branch while preserving generic checked queries, mapped flushes, all existing engine decorators, and the new permissions/subscriptions/triplets/GraphQL features. Compile both native ports and add focused composition regressions; use CI for full suites.
- [ ] Recover the five files preserved in `ea36fa4972e98fcfe9985e6a68e0d51442f32e4b`; distinguish the previous failing control from verification on current dependencies.
- [x] Report the October 28 session interruption at the maintainer's request: https://github.com/openai/codex/issues/56475#issuecomment-6668783708. The visible error does not establish its internal cause.
- [ ] Check published Rust/C# doublets releases and all dependency manifests/locks. Review source changes and use current APIs where they replace local code without weakening guarantees.
- [ ] Inventory actual patterns repeated at least three times and request suitable dependency ownership with concrete examples, then integrate available replacements.
- [ ] Complete generic mapped allocation, partial shrink and destructor failure behavior with bounded before/after regressions, preserving supported public generic APIs.
- [ ] Continue R1–R12: physical recovery/undo; independent guarantee costs; distributed ownership/membership/history management; competitor strategies and comparisons; all-language parity; complete coverage; bounded and scalable catalog operations. Detailed existing evidence and remaining criteria are in REQUIREMENTS.md and PLAN.md.
- [ ] Review the complete PR diff, update releases/title/description, preserve positive closing reference `Fixes #138`, commit all changes, push only the prepared branch, download failing exact-head CI logs, fix failures, and mark PR ready only when every required criterion is verified.

## Evidence and constraints

The starting head is `d3ba9bba0b3ad5577264235a1245c926f4afcd14`. Its five most recent workflow runs were successful on 2026-10-28; these do not verify this continuation. Current `main` is `e183f0d` and updates doublets to 0.19.0 and links-notation to 0.30.1. Merge conflicts were reproduced locally. The existing PR description explicitly lists unfinished acceptance criteria; it must not imply completion until those criteria are met.

Local resources: one compiler job, reduced Rust debug data, focused checks, finite resource-limited failure experiments. Full test and coverage matrices run in CI. Research and command logs are retained outside the checkout until relevant concise evidence is committed.

## Merge and current releases

The conflict resolution retains main's subscriptions, permission sessions,
triplets, GraphQL and composition APIs together with the branch's generic
addresses, original-query envelope, finite admission and allocator previews.
New permission and undo wrappers forward checked admission and reservation
capabilities, and observed execution retains change notifications for original
query envelopes. Focused Rust checks pass 21 cases (including two new contracts).
JS client/composition checks pass 36 cases; repository policy checks pass 186.
C# focused verification is still in progress.

Rust doublets 0.20.0 and C# Platform.Data.Doublets 0.18.9 were released during
this continuation (2026-11-05). All affected manifests and lockfiles use those
releases. Rust 0.20.0 has no source changes relative to 0.19.0. Its transitive
platform-mem remains 0.4.0; that release preserves initialization but still
unmaps the old view before fallible growth operations. The generic failure
criterion therefore requires separate verification and repair. C# needed an
uncached NuGet restore after the new publication; it is now restored.
