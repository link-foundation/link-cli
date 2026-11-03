# Composing Links Networks

Every layer of link-cli is a links network over another links network.
Each one exposes the same create, read, update, delete and naming contract
as the store under it. A layer can therefore wrap a store, another layer, or
a remote server, and other code can wrap it in turn. This document describes
the layers that issue #149 added: permissions, union, remap and external ids.
It explains how they compose with each other and with the older layers, and
which scenarios each composition serves.

- C#: `csharp/Foundation.Data.Doublets.Cli.Library`.
- Rust: the `link_cli` crate.
- JavaScript: the `@link-foundation/link-cli-client` package in `js/client`.
  It has the union, remap and external-id layers. Permissions are enforced by
  the server it talks to.

## The Layers

| Layer | C# | Rust | JavaScript | What it does |
|-------|----|------|------------|--------------|
| Store | `UnitedMemoryLinks`, `NamedTypesDecorator` | `LinkStorage`, `NamedTypesDecorator`, `DoubletsStorage` | `connect`, `connectProcess`, `WasmLinksClient` | Keeps the links and their names. |
| Types (#148) | `TypesDecorator<T>`, `ITypes<T>` | `TypesDecorator`, `Types` | (served) | Type hierarchy: `(S: T S)` and `Extends`. |
| Permissions (#149) | `PermissionsDecorator<T>`, `PermissionPolicy<T>`, `IPermissionPolicy<T>` | `PermissionsDecorator`, `PermissionPolicy`, `AccessPolicy` | (served: `--permissions`, `(auth: token)`) | The store as one subject sees it. |
| Union (#149) | `UnionLinksDecorator<T>` | `UnionLinks` | `UnionLinks` | Many networks seen as one. |
| Remap (#149) | `RemapLinksDecorator<T>`, `IAddressMap<T>`, `OffsetAddressMap<T>`, `AddressTable<T>` | `RemapLinks`, `AddressMap`, `OffsetAddressMap`, `AddressTable` | `RemapLinks`, `OffsetAddressMap`, `AddressTable`, `ExternalIdMap` | A network under other addresses. |
| External ids (#149) | `LinksExternalIds` | `LinksExternalIds` | `LinksExternalIds` | Ids of other systems, such as UUIDs, bound to links and stored as links. |
| Undo (#149) | `UndoableLinksDecorator<T>` | `UndoableLinksDecorator` | - | Makes a request atomic. |
| Transactions (#98) | `TransactionsDecorator<T>` | `TransactionsDecorator`, `GenericTransactionsDecorator` (over any layer through `NamedLinksStorage`) | - | Records every write in a log. |
| Version control | `VersionControlDecorator` | `VersionControlDecorator` | - | Adds branches, tags and checkout over the log. |
| Composed | `IComposedLinks<T>` (`Claims`, `EnsureCreated`) | `NamedTypeLinks::claims` | `UnionLinks.ownerOf` | Reports which network an address belongs to. |

No layer depends on another. The union knows nothing of permissions, and
permissions know nothing of unions. They compose because each one is a links
network. The tests that combine them are listed under
[Evidence](#evidence).

## Union: Many Networks as One

`UnionLinks([a, b, ...])` reads the links of every member as one network.

- **Shadowing.** The link at an address is the one in the first member that
  has it. A member therefore shadows the members after it, the way the local
  overlay of minimongo or minilinks shadows the server's documents.
- **Writes stay where the link lives.** A new link is created in the first
  member, the primary one. Updates, deletes and names go to the member that
  holds the link (`OwnerOf` / `owner_of` / `ownerOf`). That member's store
  resolves the write, including merges and cascades, and they stay inside
  the member: a doublet is unique within its member, not across members.
- **Links may refer across members.** For example, a link of the client can be
  typed by a type of the server; it holds the server type's address in the
  union.
- **Names** are looked up in the members in order, so a name in the primary
  member wins.
- **Creating at a given address.** This happens with `EnsureCreated`, and when
  a refused delete is undone. The link is created in the first member that
  *claims* the address, so a link that comes back returns to its own member.

When members have overlapping addresses, the union shadows. To avoid that,
move a member out of the way with a remap. `RemapLinks(server,
OffsetAddressMap(2^24))` reads the server's link 14 as `16777230`, so the
union's two members never collide.

The JS union translates the calls that carry addresses. Raw LiNo text,
queries and subscriptions belong to one store, so send them to a member
(`union.members[i]`). The JS union also refuses a write that matches many
links at once, as the C# and Rust unions do. Such a write could span members,
so it cannot be routed to a single owner.

## Remap: Other Addresses, Both Ways

`RemapLinks(links, map)` maps every address on its way down to the network's
own address, and every address on its way up back to the caller's address. The
code above the remap never has to deal with the translation.

| Map | Use |
|-----|-----|
| `OffsetAddressMap(offset)` | Moves a network out of the way of another, for a union. |
| `AddressTable(first)` | Gives the addresses of another system a range of our own addresses, in the order they are seen. In JS, with `newId`, it gives our links ids of another kind. |
| `ExternalIdMap(table, scheme)` (JS) | Reads a network under ids that are strings, such as UUIDs, and keeps those ids in links (see below), so they survive the process. |
| Custom `IAddressMap<T>` / `AddressMap` / `{ toInner, toOuter }` | Any mapping you need. |

- A read that names an address foreign to the network finds nothing. A write
  that names one fails: with `ArgumentException` in C#, an error in Rust, and
  `AddressRangeError` or `NotFoundError` in JS. The write fails before
  anything is sent.
- The null link and `Any` are constants, not addresses. They read the same on
  both sides.
- Errors raised below a remap name the inner addresses (`Forbidden: update 14`).

## External Ids: Other Systems' Ids, Stored as Links

`LinksExternalIds(scheme)` binds the ids that other systems give our links
(UUIDs, ObjectIds, URLs) to links, and back. The binding is stored as links:

```text
(E: E E)        the point named ExternalId
(S: E S)        the scheme, named after it, e.g. uuid
(p: S l)        link l has id x; p is named "uuid:x"
```

A link has at most one id per scheme, and an id names at most one link. The
table can live in the data, or in a store of its own. In a separate store,
every address in the table other than its own links is an address of the
data. The operations are `Find`, `IdOf`, `Bind`, `Ensure` (find the link, or
create one) and `Unbind`, and they are the same in all three languages. In JS,
`ExternalIdMap` turns the table into the map of a `RemapLinks`. The whole
network then reads under UUIDs, and each link gets an id the first time it is
seen.

## Permissions

The permission layer is described in [ARCHITECTURE.md](ARCHITECTURE.md#permissions)
and in the README. Here is how it composes.

### Over Or Under Another Layer

A permission layer filters the data that flows through it: links the subject
may not read are absent, and writes it may not make fail. Where it sits
decides which flow it restricts.

| Order (top first) | Effect | Test |
|-------------------|--------|------|
| Permissions, union, members | Every member is restricted at once. Markers, types and rules are found through the union, in whichever member holds them. A rule written in the client counts as much as a rule on the server. | `PermissionsOverAUnionRestrictEveryMember` |
| Permissions, union (refused delete) | A cascade the subject may not make is undone, and every link comes back in its own member, at its own address. | `ARefusedDeleteOverAUnionComesBackInItsMember` |
| Union, permissions, one member | Only that member is restricted; the client's own store stays fully writable. This is the composition of a client joined to a server that enforces its own rules. | `PermissionsUnderAUnionRestrictTheirMemberOnly` |
| Remap, permissions | The subject's view, under other addresses. | `ARemapOverPermissionsSeesWhatTheSubjectMay` |
| Permissions, transactions | Only allowed writes reach the log. Refused writes log nothing. | `PermissionsOverTransactionsLogOnlyWhatTheyAllow` |
| Transactions, permissions | The log records the subject's writes and nothing it was refused. In Rust, the transactions layer sits over the view through `NamedLinksStorage`. | `TransactionsOverPermissionsLogOnlyWhatTheyAllow` |
| Permissions, version control | Every branch and checkout is seen through the rules. | `PermissionsOverVersionControlSeeEveryBranchThroughTheRules` |

When permissions are the outermost layer, they restrict what flows into and
out of every layer below them. A layer above permissions only ever sees what
the subject may see.

### Where the Rules Live

- **In the data** (`--permissions self`, `PermissionPolicy(data, data)`,
  `PermissionPolicy::in_data`). The rules are ordinary links in the same
  file, so a subject that may write them may change them.
- **In a file of their own** (`--permissions rules.links`,
  `PermissionPolicy(data, rules)`, `PermissionPolicy::with_rules`). The
  markers are still points of the data, so both placements behave the same.
  The test suites run every permissions case both ways
  (`separateRules: true` / `Some(rules)`).
- **Across a union.** A policy built over a union reads its rules from all
  members.

### Roots, Markers and Type Systems

A policy needs a way to tell what type a link has. It can get that in four
ways:

1. **The default.** Points named `Type`, `Extends`, `AllowRead`, `AllowWrite`
   and `Anything` are created on first use (`EnsureMarkers`,
   `PermissionMarkers::ensure`, `TypeMarkers::ensure`). The policy uses the
   `TypesDecorator` hierarchy (#148).
2. **Explicit roots and markers.** Pass any links as the markers and the type
   root: `new PermissionMarkers<uint>(read, write, anything)`,
   `new TypesDecorator<uint>(data, root)`, and the Rust `PermissionPolicy::new`
   with `TypeMarkers`. The links may be unnamed and at any address, and the
   store never gets a point named `Type`
   (`RootsAndMarkersMayBeUnnamedLinksOfAnyAddress`).
3. **A type system of your own.** Keep the rules as links, but decide subject
   and object matching yourself. In C#, override `IsSubject` / `IsObject` of
   `PermissionPolicy<T>`. In Rust, implement `AccessPolicy` and read the
   stored rules with `PermissionPolicy::granted`
   (`APolicyMayBringATypeSystemOfItsOwn`). The test
   uses a type system where a link's type is simply its source.
4. **No rules at all.** Implement `IPermissionPolicy<T>` / `AccessPolicy`
   directly, for example as owner-writes / everyone-reads
   (`APolicyMayKeepNoRulesAtAll`). The decorator still provides filtering,
   `Forbidden` errors, atomic writes and ownership of new links.

## Scenarios

**A client joined to a server.** Use `UnionLinks([local, RemapLinks(server,
OffsetAddressMap(2^24))])`. The client's drafts live locally and may refer to
the server's links. The server is served with `--permissions`, so its rules
apply to what the client reads and writes there. The runnable version, with
both CLIs, is [examples/composition](../examples/composition).

**An optimistic overlay (minilinks, minimongo).** Put a writable local
network first and the server's network after it, with no offset, so local
links shadow the server's at the same addresses. Reads show the local
version until it is dropped.

**Ids from another system.** Use `RemapLinks(links, ExternalIdMap(ids,
'uuid'))` in JS, or `LinksExternalIds` in C# and Rust. A service that names
records by UUID reads and writes our links under its own ids. The id table is
stored as links, in the data or apart from it.

**Several services as one network.** Remap each service's network by its own
offset (or `AddressTable`) and join them all in one union. Put a permission
layer over the union to apply one set of rules to all of them, or under it to
apply each service's own rules.

**Audited access.** Use permissions over transactions (or transactions over
permissions) to keep a log of exactly the writes a subject was allowed to
make.

## Limits

- A union cannot make a doublet unique across members. Merges and cascades
  stay within a member.
- A union refuses a write that matches many links at once. Write the links
  one at a time.
- Names in a union are per member: the first member that has a name wins.
- Over the protocol, a served store with `--permissions` hides the names of
  links its subject cannot read, so `getByName` returns null for them. Queries
  still resolve names below the view, as ARCHITECTURE.md describes.
- Errors from below a remap name the inner addresses.
- The JS package has no permission layer of its own. Permissions are enforced
  by the server (`--permissions`), and the connection authenticates with
  `client.requestText('(auth: token)')`.

## Evidence

- C#: `csharp/Foundation.Data.Doublets.Cli.Tests/UnionAndRemapLinksTests.cs`,
  `PermissionsIntegrationTests.cs` and `PermissionsDecoratorTests.cs`.
- Rust: `rust/tests/union_and_remap_links_tests.rs`,
  `permissions_integration_tests.rs` and `permissions_decorator_tests.rs`.
  These are the same cases as C#, one for one.
- JavaScript: `js/client/test/union-remap.test.mjs`, which mirrors those
  cases over an in-memory network.
- End to end: [examples/composition](../examples/composition), run by the JS
  client CI against both CLIs.
