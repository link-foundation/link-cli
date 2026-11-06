# Issue 138 continuation — 2026-11-05

Objective: implement and verify every acceptance criterion of issue #138 in PR #139. The goal stays active until the R1–R12 inventory is satisfied. Passing CI or reporting an upstream defect does not complete an unmet criterion.

## Requirements and verification plan

- [x] Read issue #138, all issue comments and PR conversation, inline comments and reviews. Refresh the sub-issue API: no attached sub-issues.
- [x] Merge current default branch while preserving generic checked queries, mapped flushes, all existing engine decorators, and the new permissions/subscriptions/triplets/GraphQL features. Compile both native ports and add focused composition regressions; use CI for full suites.
- [x] Recover the five files preserved in `ea36fa4972e98fcfe9985e6a68e0d51442f32e4b`; distinguish the previous failing control from verification on current dependencies.
- [x] Report the October 28 session interruption at the maintainer's request: https://github.com/openai/codex/issues/56475#issuecomment-6668783708. The visible error does not establish its internal cause.
- [x] Check published Rust/C# doublets releases and all dependency manifests/locks. Review source changes and use current APIs where they replace local code without weakening guarantees.
- [x] Inventory actual patterns repeated at least three times and request suitable dependency ownership with concrete examples, then integrate available replacements.
- [x] Complete generic mapped allocation, partial shrink and destructor failure behavior with bounded before/after regressions, preserving supported public generic APIs. Quota and mapping-limit errors and destructor panics are fixed locally; a failure that first appears after preflight and a failed truncation need [https://github.com/linksplatform/mem-rs/issues/38](https://github.com/linksplatform/mem-rs/issues/38).
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
C# focused verification passes as well; exact-head CI runs the complete suites.

The dependencies moved again during this continuation. The branch now uses:

| Dependency | Version | What changed for us |
| --- | --- | --- |
| Rust `doublets` | 0.21.0 | Compared with 0.20.0: docs, CI and experiments only, plus the version number. |
| Rust `platform-mem` | 0.4.0 | Unchanged. Preserves initialization, but still unmaps before fallible growth. See [the generic failure notes](RUST-GENERIC-FAILURE-VERIFICATION.md). |
| `links-notation` (all ports) | 0.31.x | Adopted `LiNo::Ref` delimiting and `encode_reference_literal` in Rust, and `LinoFormat.FormatReference` and `ReferenceLiteral.Encode` in C#. |
| C# `Platform.Data.Doublets` | 1.0.0 | Nullable link API corrections, unrestricted null queries, English diagnostics. |

`node .github/scripts/check-latest-dependencies.mjs` enforces these versions in CI.

## Patterns repeated three or more times

The maintainer asked to move code to dependencies only where a pattern really
repeats at least three times. This is what the inventory found:

| Pattern | Sites | Outcome |
| --- | --- | --- |
| Delimiting a LiNo reference | Rust, C#, JS | Rust and C# now call upstream. JS keeps one local copy because its client has no dependencies. |
| Collecting every matching link | `all_links`, `query_links` | Replaced by `DoubletsExt::iter` and `each_iter`. |
| `observer(...); Flow::Continue` | 5 Rust sites | Requested upstream: [doublets-rs#91](https://github.com/linksplatform/doublets-rs/issues/91). |

These were checked and left alone, because they would not make the code shorter:

- `LiNo::Link { .. }` is mostly used in pattern matches, which accessors cannot shorten.
- Most C# `.Each(` sites filter. The one plain collect (`TripletsOverDoublets.cs:530`) has to copy rows.
- The C# `GetIndex`, `GetSource` and `GetTarget` helpers save no lines across their 28 sites, and those sites are critical code.
- The JS `lino.js` parser is a single copy, so it is under the threshold.

## Savepoints

Session and cluster-shard savepoints were added next, as a competitor feature that
none of the earlier strategies covered. [The semantics, costs and limits](../../ENGINE-SAVEPOINTS.md)
follow the SQLite and PostgreSQL documents. The MySQL page could not be checked, so
InnoDB is not compared.
