# Issue #147: associative architecture delivery audit

## Work plan

- [x] Read #147, all six sub-issues, #161, their comments, and the conversation,
  inline comments and reviews on PR #164 and the implementation PRs.
- [x] Check the prepared branch, working tree and relationship to `main`.
- [x] Inventory acceptance criteria and existing implementation/test evidence.
- [x] Restore the missing roadmap, including dependency order and links.
- [x] Reproduce and fix any gaps found in the audit, with matching regression
  tests in C# and Rust.
- [x] Refresh the stale notation dependency and include patch release notes.
- [x] Run focused local checks within the container resource budget.
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
log lines 206–208 identify the manifest and both lockfiles. No checks need
to be weakened.

## Reproduction and fix

Import these lines into a new triplets store with `TripletsLino.Import` or
`import_triplets_lino`:

```text
(Type: Type 0 0)
('0': Type 0 0)
(r: '0' '0' '0')
```

Before the fix both ports reject the named definition with
`A triplet cannot be at address 0.` Other quoted numeric names silently
become addresses, while quoted wildcards and variables are rejected as
query syntax. Export also loses the distinction for syntax-like names.

The triplet entry points now use the existing reference-preserving query
parser, and export uses its matching formatter. Bare addresses, variables
and wildcards retain their meaning. Both ports exercise 14 reference forms
through imports, export/reimport, query matching, creation and in-place
substitution, including all quote delimiters, reference literals, numeric
names, wildcards, variables, whitespace, controls and mixed quotes. Existing
orphan checks run after the writes.

The baseline Rust test fails and the original nine C# cases give six failures
and three passes before the production fix. Preserved logs:
[Rust](../../../dev/log/issues/147/pulls/164/rust-before.log),
[C#](../../../dev/log/issues/147/pulls/164/csharp-before.log), and
[initial dependency CI](../../../dev/log/issues/147/pulls/164/dependencies-42048542376.log).

## Focused local verification

- Rust triplet suite: 19 passed, including the new regression and the
  1000-triplet reference-store comparison.
- Rust triplet protocol: 20 passed; shared GraphQL vectors: 1 passed.
- C# triplet, triplet-protocol and GraphQL suites: 59 passed.
- JavaScript, repository-layout and workflow/release policy checks:
  `npm --prefix js run test:js`, 230 passed; this does not build WebAssembly.
- Rust formatting and whitespace checks pass.

Rust builds use one job, disabled debug information and a per-process virtual
memory limit of 1,900,000 KiB. C# uses one build worker, disabled shared
compilation and a 1 GiB managed heap limit. Full cross-platform suites,
WebAssembly, release checks and the real DeepClient run use existing CI.

The previously committed [DeepCase screenshot](../../screenshots/deepcase-clink.png)
shows the fresh database served by the Rust port. Shared GraphQL fixtures and
the real DeepClient CI cover both ports. This audit changes query semantics,
with no visual UI changes.
