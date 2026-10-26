# Recovering the interrupted database work

The maintainer requested recovery in [PR feedback](https://github.com/link-foundation/link-cli/pull/139#issuecomment-6396567610).
The linked public log contains 210,089 lines (56,773,907 bytes). Its final
completed work concerns iterative branch ancestry, semantic compatibility,
bounded stack regressions and local validation. The last error occurs at
2026-10-25 04:12:43 UTC, followed by a failed turn. The visible events do not
establish what input triggered the interruption or its internal cause.

The wrapper preserved 22 changed files in recovery commit
`f5af989db90d87a1ddcdec64b6b5c67cd8d99853` without changing PR head `0e95c81`.
This session fast-forwarded the prepared PR branch to that recovery commit,
preserving the original history and failing-before regression evidence.
Full local ancestry validation has been rerun successfully; prior partial results
were not treated as complete verification. Exact counts, coverage and the repaired
workspace-layout check are recorded in deep-ancestry.txt and VERIFICATION.md.
See [the recovered plan](DEEP-ANCESTRY-PLAN.md).

All nine workflows for prior recovery-validation commit `0e95c81` completed
successfully. Each run was created at 2026-10-25 04:03:39 UTC. This resolves the
previous plan's pending CI entry. All nine workflows also pass for recovered
ancestry head `aa601e2`; deep-ancestry-ci.txt records the exact runs. No new
issue comments, inline PR comments, reviews or required sub-issues
were present on the initial refresh. Conversation feedback and the Codecov
coverage report remain part of the acceptance inventory.

The interruption was reported with a concise, database-specific account in
[OpenAI Codex issue 56475](https://github.com/openai/codex/issues/56475#issuecomment-6396607549).
That existing report describes the same visible error during database work.
The comment records the time, requested model, invocation mode, finite local
regression and public evidence, distinguishing observed facts from unknowns.
It requests investigation and makes no claim about a classifier workaround.
The current recovery container reports Codex CLI 0.164.0; the interrupted
session's CLI version and subscription were not established by extracted events.

Official [developer command documentation](https://learn.chatgpt.com/docs/developer-commands?surface=cli)
also documents interactive `/feedback` submission. This session uses a
noninteractive tool interface, so the public GitHub comment is the completed
reporting action; no interactive feedback submission is claimed.

The overall R1–R12 objective remains active. In particular, broader competitor
strategies, ordinary legacy name/history atomicity and complete project coverage
were not completed by the interrupted session. See [requirements](REQUIREMENTS.md)
and [the resumed checklist](SESSION-RECOVERY-PLAN.md).

## Second recovery: structure output

The [next maintainer comment](https://github.com/link-foundation/link-cli/pull/139#issuecomment-6407850523)
requests recovery of another interrupted session, reporting it to OpenAI, and
verification that SQLite remains optional while native doublets implementations
cover the requested guarantees. These are additional requirements; selecting the
SQLite adapter does not complete the remaining native storage criteria.

The linked public log has 110,895 lines. Its final failure is at
2026-10-25 21:15:12 UTC. It preserves bounded stack-overflow reproductions and an
iterative structure formatter, but the complete coverage run was interrupted.
The wrapper saved 18 files in commit
`500ec8f951b4e7ebf5904488400b11acecacca29` on the recovery branch. The prepared
PR branch was fast-forwarded to that commit without rewriting its history.
Complete local validation has been rerun successfully, including native Rust,
both C# configurations, actual WASM, JS coverage and all three client runtimes.
Interrupted partial results were not counted as a completed run. See
[the structure plan](STRUCTURE-DEPTH-PLAN.md) and structure-depth.txt.

All nine workflows passed on the previously published Unicode-graph head
`1a482ac1a4d51b122a4864a3361d44e7258b3a0c`; they were created at
2026-10-25 20:56:54–55 UTC. Those runs do not verify the recovered changes.
No required sub-issues, issue comments, inline review comments or reviews were
returned on the refreshed paginated endpoints.

The second interruption was reported in the
[existing OpenAI issue](https://github.com/openai/codex/issues/56475#issuecomment-6408065263).
The report distinguishes the observed failed turn from its unknown internal
cause and links the public evidence and database task. It does not claim that
an interactive `/feedback` report was submitted.
