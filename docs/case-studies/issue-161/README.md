# Case study: a reference to an unreadable link (#161)

Issue: [#161](https://github.com/link-foundation/link-cli/issues/161). The
decision comes from
[link-foundation/ui#119](https://github.com/link-foundation/ui/issues/119), which
found the same leak in `@link-foundation/ui-links`.

## The leak

`PermissionsDecorator` hid an unreadable link from reads, but references were
checked on the store below the view: `reference_exists` was forwarded, and the
query processor was handed the store below as `Options.References`. So a subject
could tell a hidden link from a missing one by referring to it. As `alice`, who
may not read the secret, with the code before the change:

| Request                   | Secret (exists, hidden)   | Missing                   |
| ------------------------- | ------------------------- | ------------------------- |
| `() ((Note <address>))`   | success                   | `Invalid reference to …`  |
| `() ((<address>: Note Note))` | fails: address is taken | success                 |
| `((<address>: * *)) ()`   | success, nothing matches  | `Invalid reference to …`  |
| `() ((Note secret))` / `() ((Note nothing))` | success, by name | `Invalid reference to …` |

Each row tells the subject whether a hidden link exists, and in the first and the
last the subject's own link ends up pointing at a link it was never allowed to
know about.

The tests of commit `0cc9426` show it before any fix. In Rust:

```text
left:  (Some(0), "() ((26: Note secret))\n")
right: (Some(1), "Error: Query error: Invalid reference to non-existent link '<address>' in substitution pattern...")
```

In C#, four of the five cases of `AnUnreadableAddressAnswersAsAMissingOne` and
`TheNameOfAnUnreadableLinkAnswersAsAnUnknownName` failed, with the same
differences as the table. The fifth already answered alike, and pins what must
not change.

## Options compared

| Option                          | Hidden and missing alike | `User writes Note` still types links as `Note` | Lets a subject link to a hidden link | Rules to add                     |
| ------------------------------- | ------------------------ | ---------------------------------------------- | ------------------------------------ | -------------------------------- |
| Keep the leak                   | no                       | yes                                            | always, for every subject            | none                             |
| Require read                    | yes                      | no: reading `Note` itself needs read on `Doc`  | never                                | read on parents, exposing values |
| New `AllowReference` rule only  | yes                      | only with one more rule per type               | where granted                        | one per type used                |
| `Link not found` for both only  | yes                      | only if something defines what may be used     | —                                    | —                                |
| **Chosen: rule + implied uses** | yes                      | yes                                            | where granted                        | none, unless it is needed        |

"Require read" fails on the type hierarchy. A rule covers the values and
sub-types of its object, not the object's own link, whose source is its parent:
to read `Note` a subject needs read on `Doc`, which also shows every `Secret`.

## Decision

A reference is a right of its own. A subject may refer to an address when:

1. it may read the link there;
2. it is that link;
3. a rule `(r: AllowReference (g: Subject Object))` covers it: `Object` itself and
   the links of type `Object`;
4. it is the `Object` of an `AllowRead` or `AllowWrite` rule of the subject.

`AllowReference` does not allow reading. A rule `Anything` to `Anything` brings
the previous behaviour back as an explicit choice. A custom policy that does not
implement `can_reference` / `CanReference` allows references to readable links
only.

What follows the same rule:

- **A reference a link already holds** may stay. Once it is replaced it cannot be
  put back without the right.
- **Choosing the address of a new link** is a reference. A limited subject cannot
  choose addresses, so a query cannot refer forward to an address it creates later
  in the same request, and a query replacing a link it has just deleted at the same
  address fails atomically. Holders of the right still can.
- **Names**: the name of a link the subject may not refer to is an unknown name,
  and taking it over is `Forbidden`.
- **Types it may not write** stay `Forbidden` for hidden and missing alike: the
  write check runs before the reference check.

## Where the answers differ

The rule is the same in every layer; the words depend on the layer that answers,
and each layer answers alike for hidden and missing.

| Layer                                      | Answer                                       |
| ------------------------------------------ | -------------------------------------------- |
| The view, `(create:` and `(update:`        | `Link not found: N`                          |
| A query                                    | `Invalid reference to non-existent link 'N'` |
| A triplet part in an operation             | `The from part N is not a triplet.`          |
| Picking an address with a query on triplets | `Link not found: N`                         |

A query checks its references before it writes, so a query that names a hidden or
a missing type answers `Invalid reference` for both. It is `(update:` and the view
that answer `Forbidden: update N` for such a type, and a query with a type the
subject may refer to but not write (`Public`, for `alice`) stays `Forbidden` too.
For a guest, C# plans the addresses of a query's new links by creating them, so
`() ((1 2))` answers `Forbidden: create`, while Rust answers `Invalid reference`.
Both answer the same for an existing and a missing address.

Under `Anything` to `Anything`, or for an administrator, `(create: (Note 9999))`
still stores a target with no link there: the doublets store takes one, as it did
before. `@link-foundation/ui-links` answers `Link not found` there. This is the
behaviour the rule restores, so it was kept; `reference_exists` tells the two apart.

## What this does not hide

- **A subject with the right** can tell a hidden link from a missing one, by design.
- **Addresses in readable links**: a readable link's source and target are shown,
  including one that points at a hidden link.
- **Address allocation**: a create answers with the next free address, which shows
  how many addresses are taken or freed.
- **Name uniqueness**: taking a name held by a hidden link answers `Forbidden`, where
  a free name succeeds. So a subject can learn that a name is taken, though not which
  link holds it.
- **Timing** was not considered.

## Upgrading

The rules of an existing store keep working, and the `AllowReference` point is
created on first use, after every other link, so no address moves. A subject that
referred to links it could not read gets errors now: add an `AllowReference` rule
for what it needs, or `Anything` to `Anything` for everything. That is why both
packages take a major version.

## Verification

```sh
cargo test --manifest-path rust/Cargo.toml --test permissions_references_tests
dotnet test csharp/Foundation.Data.Doublets.Cli.Tests --filter FullyQualifiedName~PermissionsReferencesTests
examples/permissions/clink-queries.sh rust/target/debug/clink | diff examples/permissions/expected-output.txt -
```
