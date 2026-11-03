# Permissions stored in links, and the layers they compose with (issue #149)

Issue #149 asked for an optional permission layer whose rules are links. The
review of PR #157 widened the scope: the rules' roots may be defined in any
way, permissions must restrict every other layer, and three new layers were
asked for (union, remap and external ids) in C#, Rust and JavaScript. The
whole repository's documentation was to be brought up to date.

[COMPOSITION.md](../../COMPOSITION.md) describes the layers and how they
compose. [ARCHITECTURE.md](../../ARCHITECTURE.md#permissions) describes the
permission design. This page maps each requirement to its evidence.

## Issue acceptance criteria

| Criterion | Evidence |
|-----------|----------|
| A user with "User writes Note, User reads Public" sees notes and public links only, and an update of another type fails with `Forbidden: update N`, the store unchanged. | C# `PermissionsDecoratorTests`, Rust `shows_a_subject_only_what_it_may_read*` and `refuses_writes_the_store_unchanged*`; over TCP, `serves_each_subject_what_its_permissions_allow` (`links_sessions_tests.rs`) and the CLI tests `a_subject_sees_what_its_rules_allow_and_writes_nothing_else*`. [`examples/permissions`](../../../examples/permissions) runs it with both CLIs. |
| A failed request is undone completely, even where it touched hidden links. | `undoes_every_part_of_a_failed_substitution_including_hidden_ones`, `refuses_a_delete_that_would_take_a_hidden_link_with_it`, `refuses_a_merge_that_would_repoint_a_hidden_link`, in both ports. |
| Rules in a separate file work the same as rules in the data. | Every rules test runs twice (`_rules_in_data` / `_rules_apart` in Rust, `separateRules` in C#); `serves_rules_in_a_store_of_their_own_alike`. |
| Without `--permissions`, every existing test passes unchanged. | `without_permissions_every_session_sees_the_whole_store`; the existing suites run unchanged in CI. |
| References to unreadable links: decide and document. | A query may refer to a link its subject cannot read, such as the type of its own link (`lets_a_subject_type_the_links_it_creates_as_types_it_may_not_read`). A hidden address stays taken, so creating links cannot probe it. Documented in [ARCHITECTURE.md](../../ARCHITECTURE.md#permissions). |
| Pluggable authentication; a token table as links. | `authenticates_with_the_callback`, `authenticates_with_a_token_table_in_links`, `authenticates_over_tcp`, `served_connections_authenticate_with_tokens_stored_as_links*`. Tokens are stored as their SHA-256 only. |

## Review of PR #157

| Request | Evidence |
|---------|----------|
| Use the type system decorator (#148) as the foundation, or any other way to define the root permission links. | The default policy reads types through `TypesDecorator`. Three other options each have a test in both ports: `RootsAndMarkersMayBeUnnamedLinksOfAnyAddress`, `APolicyMayBringATypeSystemOfItsOwn` and `APolicyMayKeepNoRulesAtAll`. [COMPOSITION.md](../../COMPOSITION.md#roots-markers-and-type-systems) lists all four. |
| Permissions applied before the other decorators restrict their data flow in and out. | `PermissionsOverAUnionRestrictEveryMember`, `ARefusedDeleteOverAUnionComesBackInItsMember`, `PermissionsUnderAUnionRestrictTheirMemberOnly`, `ARemapOverPermissionsSeesWhatTheSubjectMay`, `PermissionsOverTransactionsLogOnlyWhatTheyAllow`, `TransactionsOverPermissionsLogOnlyWhatTheyAllow` and `PermissionsOverVersionControlSeeEveryBranchThroughTheRules`, in `PermissionsIntegrationTests.cs` and `rust/tests/permissions_integration_tests.rs`. |
| Rules stored separately and in the same file. | `--permissions self` and `--permissions rules.links`; see above. |
| A union decorator that joins networks, in C#, JavaScript and Rust, independent of permissions. | `UnionLinksDecorator<T>`, `UnionLinks` (Rust) and `UnionLinks` (JS). None of them depends on the permission layer; the two only meet in doc comments and tests. Tests: `UnionAndRemapLinksTests.cs`, `rust/tests/union_and_remap_links_tests.rs`, `js/client/test/union-remap.test.mjs`. |
| A remap decorator for UUIDs and other systems' ids, both ways. | `RemapLinksDecorator<T>` / `RemapLinks` with `OffsetAddressMap` and `AddressTable`; `LinksExternalIds` in all three languages keeps the ids as links; the JS `ExternalIdMap` reads a network under UUIDs. |
| How the abstractions play together, and in which scenarios. | [COMPOSITION.md](../../COMPOSITION.md#scenarios). [`examples/composition`](../../../examples/composition) joins a client's links with a served store that enforces permissions, under UUIDs. The JS client CI runs it against the Rust and C# CLIs and compares the output. |
| Repository-wide documentation update. | README (permissions, joining and remapping networks, the documentation list), [ARCHITECTURE.md](../../ARCHITECTURE.md) (every source file, the JS client, every workflow), [HOW-IT-WORKS.md](../../HOW-IT-WORKS.md) (types, permissions, composition), [REQUIREMENTS.md](../../REQUIREMENTS.md), and the C#, Rust and JS client READMEs. |

## Limits

- The JavaScript client has no permission layer of its own. A client is
  restricted by the server it connects to, and a JS union can put such a
  server under a client's own store.
- Union and remap translate the calls that carry addresses. Raw LiNo
  requests, queries and subscriptions belong to one store, so they are sent
  to a member directly.
- Subscriptions (#150) and triplets (#151) are out of scope, as the issue
  says.
