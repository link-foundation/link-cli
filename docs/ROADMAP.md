# Associative architecture roadmap

[Issue #147](https://github.com/link-foundation/link-cli/issues/147) tracks
link-cli as a server for the associative architecture of Deep on doublets
and triplets. The C# and Rust implementations share the same behaviour;
the JavaScript client consumes their Links Notation protocol, and
`DeepClient` consumes their GraphQL endpoint.

The six implementation issues are completed. Their dependency order is:

| Step | Issue | Depends on | Implementation and verification |
|------|-------|------------|---------------------------------|
| 1 | [#148: Type hierarchy](https://github.com/link-foundation/link-cli/issues/148) | None | `TypesDecorator`, recursive sub-types and multiple inheritance; `TypesDecoratorTests.cs` and `types_decorator_tests.rs` cover depth, diamonds, cycles and pinned types. |
| 2 | [#149: Permissions](https://github.com/link-foundation/link-cli/issues/149) | #148 | `PermissionsDecorator`, rules in data or a separate file, authentication and atomic writes; the permissions, session, CLI and composition suites in both ports. [#161](https://github.com/link-foundation/link-cli/issues/161) adds the separate reference right. |
| 3 | [#150: Subscriptions](https://github.com/link-foundation/link-cli/issues/150) | None | `subscribe`, `notify`, `unsubscribe`, permission filtering and client push handling; both ports replay [subscriptions.txt](protocol/subscriptions.txt). |
| 4 | [#151: Triplets in doublets](https://github.com/link-foundation/link-cli/issues/151) | None | `TripletsOverDoublets`, stable addresses, pair collection, substitution queries and LiNo import/export; matching triplet suites compare with a reference store on at least 1000 triplets. |
| 5 | [#152: Triplet protocol](https://github.com/link-foundation/link-cli/issues/152) | #151 | `--triplets`, text/binary operations, permissions, subscriptions and remote clients; both ports and the JavaScript client replay [triplets.txt](protocol/triplets.txt). |
| 6 | [#153: Deep GraphQL](https://github.com/link-foundation/link-cli/issues/153) | #149, #150, #151 | `--graphql`, recorded Hasura schema, Deep tables, filters, mutations, JWT/guest authentication, both WebSocket protocols and optional packages; shared GraphQL vectors and the real DeepClient run against both ports in CI. |

Steps 1, 3 and 4 can start independently. Step 2 follows step 1; step 5
follows step 4; step 6 joins permissions, subscriptions and triplets.

See [REQUIREMENTS.md](REQUIREMENTS.md) for the full requirement inventory,
[ARCHITECTURE.md](ARCHITECTURE.md) for the layers and their composition,
[TRIPLETS.md](TRIPLETS.md) for the encoding and protocol, and
[GRAPHQL.md](GRAPHQL.md) for DeepClient and DeepCase setup and compatibility.
The corresponding client roadmap is
[link-foundation/ui#109](https://github.com/link-foundation/ui/issues/109).
