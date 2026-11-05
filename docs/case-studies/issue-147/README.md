# Issue #147: associative architecture delivery audit

## Work plan

- [x] Read #147, all six sub-issues, #161, their comments, and the conversation,
  inline comments and reviews on PR #164 and the implementation PRs.
- [x] Check the prepared branch, working tree and relationship to `main`.
- [x] Inventory acceptance criteria and existing implementation/test evidence.
- [x] Restore the missing roadmap, including dependency order and links.
- [ ] Reproduce and fix any gaps found in the audit, with matching regression
  tests in C# and Rust.
- [ ] Refresh the stale notation dependency and include patch release notes.
- [ ] Run focused local checks within the container resource budget.
- [ ] Inspect current-commit CI, preserve failure logs and resolve failures.
- [ ] Review the PR diff, merge current `main` if needed, update the PR text,
  verify a clean tree and mark PR #164 ready without merging.

## Requirement inventory

| Issue | Requirements | Implementation and automated evidence |
|-------|--------------|---------------------------------------|
| #147 | Roadmap listing all six issues in dependency order; all children completed; later comment asks for a delivery and bug audit | `docs/ROADMAP.md`; all six issues closed; this audit and regressions |
| #148 | Source is type; self-target sub-types; marked extra parents; six queries; Define/Instance; arbitrary depth, diamonds, cycles; pinned type compatibility; README; both ports | `TypesDecorator.cs`, `types.rs`; matching `TypesDecoratorTests.cs` / `types_decorator_tests.rs`; `examples/type-hierarchy` |
| #149 | Read/write rules and Anything; write implies read; owned untyped creations; before/after update checks; invisible links; rollback including hidden writes; rules in either file; CLI configuration; per-connection token/callback authentication; unchanged default | Permissions decorator, integration, CLI and sessions suites in both ports; `examples/permissions` |
| #149 review | Arbitrary type roots/policies; composition over/under decorators; independent union and remap layers in C#/Rust/JS; external ids; documentation | `PermissionsIntegrationTests.cs`, `permissions_integration_tests.rs`, union/remap suites in all three languages; `docs/COMPOSITION.md`, `examples/composition` |
| #161 | Reference right independent of read; unreadable/missing indistinguishable without it; unchanged references retained; name and explicit-address checks; write check first | `PermissionsReferencesTests.cs` / `permissions_references_tests.rs`; token and triplet integration cases |
| #150 | Per-connection subscribe/notify/unsubscribe; other-connection writes; permission transitions; cleanup on disconnect; remote and JS demultiplexing; recorded protocol | `Protocol/SubscriptionsTests.cs` / `subscriptions_tests.rs`, remote/client suites; `docs/protocol/subscriptions.txt` |
| #151 | Document encoding/costs; stable address on update; no auxiliary orphans; index/type/from/to Each/Count against 1000+ reference triplets; three-part substitutions; LiNo import/export | `TripletsOverDoubletsTests.cs` / `triplets_over_doublets_tests.rs`; `docs/TRIPLETS.md` |
| #152 | Triplet count/each/create/update/delete/substitutions; explicit server mode; permissions and subscriptions; arity 2..3 packs three references together; both ports and remote/JS clients; unchanged doublet conversation | `Protocol/TripletsProtocolTests.cs` / `triplets_protocol_tests.rs`, JavaScript protocol replay; `docs/protocol/triplets.txt` |
| #153 | Recorded Hasura schema and client operations; triplets/values/can mapping; bool expressions and relations planned as substitutions; serial/nested mutations; JWT/guest; legacy and modern WebSockets; optional Deep packages; real DeepClient and DeepCase | Shared GraphQL vectors; `GraphqlTests.cs`, `GraphqlServerTests.cs`, `graphql_vectors.rs`, `graphql_server.rs`; `examples/deep-client`, `deep-client.yml`; existing DeepCase screenshot and `docs/GRAPHQL.md` |

## Initial findings

`main` at `482eabc` already contains the six implementations, but
`docs/ROADMAP.md` was never added. The prepared branch contains only the task
placeholder on top of that commit.

Triplet patterns use the notation parser's decoded strings directly, unlike
doublet queries, which preserve whether a reference was quoted. Thus a
quoted numeric name becomes an address, and quoted `*` / `$name` become
query syntax. This breaks LiNo round trips and named substitution queries
in both ports. The fix reuses the existing reference-preserving query parser.

The initial Dependencies run, `42048542376` (2026-11-05 14:48:13 UTC, head
`0e0abb7`), fails because links-notation 0.30.1 superseded 0.30.0. Downloaded
log lines 208–210 identify the manifest and both lockfiles. No checks need
to be weakened.
