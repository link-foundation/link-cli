# link-cli Architecture

`link-cli` is a multi-implementation repository centered on the same data
model: each link is a doublet with an index, source, and target.

```text
(index: source target)
```

The CLI accepts LiNo substitution expressions and applies them to a persistent
links database.

## Repository Layout

| Path | Responsibility |
|------|----------------|
| `README.md` | User-facing quick start and feature overview. |
| `docs/` | Requirements, architecture notes, behavior notes, and case studies. |
| `csharp/` | Production .NET CLI package published as the `clink` tool. |
| `rust/` | Rust library and native `clink` binary that mirror core C# behavior. |
| `csharp/scripts/` | C# release, changeset, and repository validation helpers. |
| `rust/scripts/` | Rust release, changelog, crate publication, and repository validation helpers. |
| `rust/wasm/` | `wasm-bindgen` wrapper crate around the Rust query processor. |
| `js/` | React/Vite browser workbench and JavaScript package lockfile. |
| `js/client/` | `@link-foundation/link-cli-client`: the links contract in JavaScript and TypeScript over TCP, a `clink --serve` process or WebAssembly, plus the union, remap and external-id layers. |
| `docs/protocol/` | The links protocol: binary links notation, links operations and subscriptions, with test vectors. |
| `docs/case-studies/` | Research notes per issue, with evidence. |
| `examples/` | Runnable examples, one folder per feature (TCP, permissions, composition, transactions, version control, type hierarchy, archives, embedding). |
| `.github/workflows/` | CI per part of the repository; see [CI Workflows](#ci-workflows). |

## C# Implementation

The C# implementation is the production NuGet tool.

Key files:

- `csharp/Foundation.Data.Doublets.Cli/Program.cs`: command-line surface and
  command orchestration.
- `csharp/Foundation.Data.Doublets.Cli/CliPermissions.cs`: the parts of
  `clink` behind `--permissions`, `--subject` and the token options.

The library files below are in `csharp/Foundation.Data.Doublets.Cli.Library/`.

Queries:

- `AdvancedMixedQueryProcessor.cs`: LiNo restriction/substitution processing,
  split by concern into `.Matching.cs` (patterns against the store),
  `.Mutations.cs` (creates, updates and deletes) and `.References.cs` (the ids
  and names a substitution defines).
- `MixedQueryProcessor.cs` and `BasicQueryProcessor.cs`: the earlier, simpler
  query processors, which only the library API and their tests use; the CLI
  and the server use `AdvancedMixedQueryProcessor`.
- `QueryParser.cs`: keeps reference kinds (names, addresses, variables,
  wildcards) across the notation parser, whose string model drops them.
- `QueryConstants.cs`: the constants of query patterns.
- `ChangesSimplifier.cs`: reduces the raw changes of a query to one change per
  link.
- `LinksUniquenessAndUsagesRepointingResolver.cs`: keeps every doublet unique;
  an update that would duplicate a link merges into it and repoints its usages.
- `Exceptions.cs`: the exceptions of malformed links and queries.

Storage and names:

- `NamedLinks.cs`, `NamedLinksDecorator.cs`: names stored as Unicode sequences
  in links; `INamedTypes.cs` declares `INamedTypes<T>` and
  `INamedTypesLinks<T>`, the contract every layer implements.
- `UnicodeStringStorage.cs`: Unicode strings as doublet sequences.
- `PinnedTypes.cs`: the pinned type addresses behind `PinnedTypesDecorator`.
- `SimpleLinksDecorator.cs`: a links file with automatic uniqueness and a
  names file beside it, without the named-types layer.
- `LinksExtensions.cs`: helpers over `ILinks<T>`, including `EnsureCreated`,
  which asks a composed network where an address belongs.
- `LinksFacadeDisposer.cs`: releases every disposable facade of a decorator
  chain.
- `EnumerableExtensions.cs`: deconstruction of short sequences.
- `NamedTypesDecorator.cs`: names layered over links with a sidecar names
  database.
- `PinnedTypesDecorator.cs`: pinned type support composed into named types.
- `TypesDecorator.cs`: optional type hierarchy layer (#148). `ITypes` answers
  `TypeOf`, `IsType`, `ParentsOf`, `AncestorsOf`, `IsA` and `IsOf` through
  sub-types `(S: T S)` and extra parents `(p: S T2)` marked by
  `(m: Extends p)`, and writes with `Define`, `Extend` and `Instance`. It wraps
  any `ILinks`, is not sealed, and its members are virtual; pinned types are
  sub-types of its root.
- `TripletsOverDoublets.cs`: optional triplets layer (#151). `ITriplets` keeps
  `(index: type from to)` in doublets, a node as `(i: T i)` and a link as
  `(i: T ((Pair f) t))`, and answers `GetTriplet`, `EachTriplet` and
  `CountTriplets` by index, type, from and to; `CreateTriplet`,
  `UpdateTriplet` (in place) and `DeleteTriplet` (cascading) collect the
  wrappers and pairs nothing uses. `TripletsQueryProcessor.cs` runs
  substitution queries with three parts and `TripletsLino.cs` imports and
  exports `(index: type from to)` lines. `TripletsPermissionsDecorator.cs`
  judges triplets under permissions. See [TRIPLETS.md](TRIPLETS.md).
- `LinoDatabaseInput.cs`: `.lino` import.
- `LinoDatabaseOutput.cs`: database export, change formatting, and structure
  formatting.
- `PersistentTransformationDecorator.cs`: stored trigger support.
- `TransactionsDecorator.cs`: optional transactions layer that records each
  Create/Update/Delete as a reversible transition into a sidecar doublets
  store.
- `VersionControlDecorator.cs`: optional version-control layer that sits above
  the transactions decorator and provides branching, tagging, and time-travel
  checkout.
- `Protocol/`: LiNo over TCP (#105). The binary links notation itself
  (`LinksPacket`, `LinoMapping`, `LinoFormat`, `PacketReader`) comes from
  `Link.Foundation.Links.Notation.Binary` (#104). This folder contains the
  transport around it:
  - `TextLinoProtocol` and `BinaryLinoProtocol` (`LinoProtocols.cs`)
  - `LinksServer` and `LinksClient`
  - `RemoteLinks` and `LinksOperation`
  - `TripletsOperation` and `RemoteTriplets.cs`, where `RemoteLinks` is an
    `ITriplets` store: triplets over the protocol (#152), which a server reads
    with `LinksServerOptions.Triplets` (`--triplets`)
  - `StoreArchive`
- `Permissions.cs`, `PermissionsDecorator.cs`: the optional permission layer
  (#149), `PermissionPolicy` over rules stored as links and the
  `PermissionsDecorator` view of a subject; see [Permissions](#permissions).
- `LinksUndoLog.cs`: `UndoableLinksDecorator`, which makes a request atomic
  under permissions.
- `LinksTokenTable.cs`: the token table of `(auth: token)`, stored as links.
- `Protocol/LinksSessions.cs`: a store served to many sessions, each with its
  subject; `LinksServer` gives every connection one.
- `UnionLinksDecorator.cs`, `RemapLinksDecorator.cs`, `AddressMaps.cs`
  (`IComposedLinks`, `IAddressMap`, `OffsetAddressMap`, `AddressTable`) and
  `LinksExternalIds.cs`: the composing layers of #149; see
  [Composing Links Networks](#composing-links-networks).
- `Protocol/LinksConnection.cs`: one server connection, with its
  subscriptions and pending documents. `Protocol/LinoProtocolException.cs`:
  protocol errors and their kinds.
- `LinksFileLock.cs`: advisory locking of a database's `.lock` sidecar plus the
  `StorageRevision` fingerprint used to detect writes by other processes.

Main C# dependencies:

- `Platform.Data`
- `Platform.Data.Doublets`
- `Platform.Data.Doublets.Sequences`
- `Link.Foundation.Links.Notation`
- `System.CommandLine`

## Rust Implementation

The Rust implementation mirrors the core CLI behavior and is the basis for the
browser runtime.

Key files:

- `rust/src/main.rs`: native `clink` entry point.
- `rust/src/cli.rs`: argument parser and help text.
- `rust/src/lib.rs`: the public surface of the `link_cli` crate.
- `rust/src/error.rs`: `LinkError`.
- `rust/src/link.rs`, `rust/src/lino_link.rs`, `rust/src/parser.rs`: the
  doublet, the parsed LiNo link, and the LiNo parser.
- `rust/src/query_processor.rs`: LiNo query execution, with
  `query_processor/matching.rs` and `query_processor/mutations.rs`.
- `rust/src/query_parser.rs`, `rust/src/query_types.rs`,
  `rust/src/query_options.rs`: reference kinds over the notation parser, the
  patterns a query passes through, and the options it runs with.
- `rust/src/changes_simplifier.rs`: the net change of every link of a query.
- `rust/src/query_processor_substitution.rs`: substitution preservation helpers.
- `rust/src/link_reference_validator.rs`: reference validation and auto-create.
- `rust/src/named_types.rs`: names sidecar storage.
- `rust/src/named_type_links.rs`: `NamedTypeLinks`, the interface every layer
  is written against, with `claims` for composed networks.
- `rust/src/named_links.rs`, `rust/src/unicode_string_storage.rs`: names
  stored as Unicode sequences in links, as C# `NamedLinks`.
- `rust/src/link_storage.rs`: `LinkStorage`, the CLI's store, in memory or in
  a file; `rust/src/link_storage_doublets.rs` bridges it to the `doublets`
  traits.
- `rust/src/pinned_types.rs`, `rust/src/hybrid_reference.rs`: pinned types and
  the `Hybrid<uint>` reference encoding, compatible with C#.
- `rust/src/persistent_transformations.rs`: stored triggers.
- `rust/src/union_links.rs`, `rust/src/remap_links.rs`,
  `rust/src/external_ids.rs`: `UnionLinks`, `RemapLinks` with `AddressMap`,
  `OffsetAddressMap` and `AddressTable`, and `LinksExternalIds` (#149); see
  [Composing Links Networks](#composing-links-networks).
- `rust/src/types.rs`: the `Types` trait and `TypesDecorator`, the type
  hierarchy layer of `TypesDecorator.cs`, over any `NamedTypeLinks`, with the
  same test cases in `rust/tests/types_decorator_tests.rs`.
- `rust/src/triplets/`: the `Triplets` trait and `TripletsOverDoublets`
  (`mod.rs`), substitution queries with three parts (`query.rs`) and Links
  Notation import and export (`lino.rs`), permissions (`permissions.rs`), the
  triplets layer of
  `TripletsOverDoublets.cs`, with the same test cases in
  `rust/tests/triplets_over_doublets_tests.rs`.
- `rust/src/indexed_links.rs`: optional `IndexedLinksDecorator`, composed by
  `with_indexed_lookups()` / Rust `--indexed-lookups` under local data and names
  stores. It maintains pair/source/target indexes at raw mutation boundaries;
  the default remains the plain map with scans.
- `rust/src/lino_database_input.rs`: `.lino` import.
- `rust/src/sequences/`: Unicode sequence conversion and related parity code.
- `rust/src/storage/`: the `LinksStorage` trait the transactions layer is
  written against, the doublets-backed `DoubletsStorage`, the
  `PersistentFileMapped` backing memory, the advisory-locking helpers, and
  `NamedLinksStorage`, which makes any `NamedTypeLinks` layer (a permissions
  view, a union, a remap) a `LinksStorage` so transactions stack over it.
- `rust/src/transactions/`: optional transactions decorator and the
  transitions-log, retention-policy, and commit-mode types.
- `rust/src/version_control/`: optional version-control decorator with
  branching, tagging, and time-travel checkout.
- `rust/src/protocol/`: LiNo over TCP (#105). The binary links notation
  itself comes from `links_notation::binary` (#104); the module adds the
  transport around it and mirrors the C# `Protocol/` folder file for file:
  - `protocols.rs`
  - `server.rs`
  - `client.rs`
  - `remote_links.rs`
  - `links_operations.rs`
  - `triplets_operations.rs` and `remote_triplets.rs`, where `RemoteLinks` is a
    `Triplets` store
  - `archive.rs`
  - `sessions.rs`: `LinksSessions`, the counterpart of `LinksSessions.cs`.
- `rust/src/permissions.rs`: the optional permission layer (#149):
  `PermissionPolicy`, `PermissionsDecorator`, `LinksTokenTable` and
  `Forbidden`, with the same test cases as C# in
  `rust/tests/permissions_decorator_tests.rs`, and with the other layers in
  `rust/tests/permissions_integration_tests.rs`.
- `rust/src/undoable_links.rs`: `UndoableLinksDecorator` and `LinksUndoLog`.

Main Rust dependencies:

- `doublets = "0.19.0"` for links storage foundations.
- `links-notation = "0.30.0"` for LiNo parsing and binary links notation.
- `lino-arguments = "0.5.0"` for argument initialization compatibility.
- `anyhow` and `thiserror` for error handling.

The Rust CLI supports the same options as the C# one: query, storage,
import/export, structure, output, named-reference, auto-create, persistent
transformation trigger, transaction and version-control options.

## WebAssembly Workbench

The browser workbench is a wrapper around the Rust query processor.

Key files:

- `rust/wasm/src/lib.rs`: exports `Clink` with `execute`, `snapshot`, `reset`, `version`,
  and `rustCoreVersion`.
- `js/src/App.jsx`: React application, query editor, graph view, and runtime
  status.
- `js/src/styles.css`: application styling.
- `js/vite.config.js`: Vite build configuration.

Runtime flow:

1. Vite loads the generated `clink-wasm` package.
2. `Clink` stores links in `LinkStorage::in_memory`, the CLI's store without a
   file, so deletes cascade and updates merge exactly as in the CLI.
3. Queries are passed into the Rust `QueryProcessor`.
4. The result includes formatted output plus a structured `links` snapshot.
5. React renders the snapshot and mirrors it into `doublets-web` `UnitedLinks`.

The committed lockfile currently pins `doublets-web` to `0.1.3`.

## JavaScript Client

`js/client` is `@link-foundation/link-cli-client`, one links contract over
interchangeable backends, the JS counterpart of `INamedTypesLinks` /
`NamedTypeLinks`. See [js/client/README.md](../js/client/README.md).

- `src/client.js`: the client over a transport; `src/tcp.js` (Node.js, Bun,
  Deno), `src/process.js` (a `clink --serve` child process) and `src/wasm.js`
  (the in-process `clink-wasm` store) are the transports.
- `src/framing.js`, `src/lino.js`, `src/operations.js`: the text protocol
  framing, canonical LiNo text, and the LinksOperation documents, triplets
  included, with every address a `BigInt`.
- `src/capabilities.js`, `src/errors.js`, `src/deadline.js`: what each backend
  guarantees, the errors with their outcomes, and cancellation.
- `src/composed.js`, `src/union.js`, `src/remap.js`, `src/external-ids.js`:
  `UnionLinks`, `RemapLinks`, `OffsetAddressMap`, `AddressTable`,
  `LinksExternalIds` and `ExternalIdMap` (#149).
- `types/index.d.ts`: the TypeScript declarations.

## Composing Links Networks

Every layer is a links network over another one, so the layers stack in any
order: permissions over a union restrict every member, permissions under it
one member, a remap moves a network out of the way of another, and external
ids keep the ids of other systems in links. [COMPOSITION.md](COMPOSITION.md)
describes each layer in C#, Rust and JavaScript, what each order does, where
rules and type roots may come from, and the scenarios they serve.

## Data Files

`--db` selects the primary links database file. Companion files are derived from
the primary filename.

| File pattern | Owner | Purpose |
|--------------|-------|---------|
| `<name>.links` | C# and Rust | Primary numeric links database. |
| `<name>.names.links` | C# and Rust | Mapping between string names and numeric link references. |
| `<name>.triggers.links` | C# | Persistent trigger definitions when triggers are not embedded. |
| `<name>.transitions.links` | C# and Rust | Optional transitions log (created when `--transactions` is requested). |
| `<name>.versioncontrol.links` | C# and Rust | Optional version-control branches/tags store (created when `--vc` is requested). |
| `<name>.links.lock` | C# and Rust | Advisory lock sidecar, created only when a caller opts into locking through the library. |

For `graph.links`, the default names file is `graph.names.links`, and the
default triggers file is `graph.triggers.links`.

## Query Pipeline

The high-level pipeline is the same across C# and Rust:

1. Parse the LiNo input into local link-pattern structures.
2. Split the top-level expression into restriction and substitution sides.
3. Validate references, optionally creating missing point links.
4. Find all database links that satisfy the restriction patterns.
5. Resolve variables from each match into substitution patterns.
6. Determine create, read/no-op, update, and delete operations.
7. Apply writes to storage.
8. Format requested output: before, changes, after, structure, import/export.

## Optional Transactions Layer

The `TransactionsDecorator` (C#) and `transactions::TransactionsDecorator`
(Rust) wrap a `NamedTypesDecorator` and record each Create / Update /
Delete as a reversible `Transition` (before + after doublet state, plus a
sequence number, transaction id, and timestamp). Transitions are serialized
as names inside a *second* links store — the transitions log is itself a
links database, so the same storage, recovery, and tooling apply. In C#
that second store is a `UnitedMemoryLinks` doublets store; in the Rust CLI
it is the same text-backed `LinkStorage` the CLI uses for data, and the
library also offers two alternatives (see *Embedding the Library* below).

Composition: `LinkStorage → NamedTypesDecorator → TransactionsDecorator`.

Public surface:

- `Create / Update / Delete / CreateAndUpdate` — recorded automatically;
  logical writes that affect multiple links record one transition per affected
  link so rollback and checkout restore the complete graph.
- `BeginTransaction()` / `begin_transaction()` — explicit batches with
  commit and rollback APIs. C# returns a disposable transaction handle;
  Rust keeps the active transaction on the decorator and commits or rolls
  it back through that decorator.
- `Log()` — read the recorded transitions.
- Three retention policies: `infinite`, `sized:<n>` (drop oldest applied),
  and `chunked:<n>:<dir>` (archive oldest applied to rolling files).
- Two commit modes: `sync` (default — flushes data side-effects before
  returning) and `async` (durably persists the log first).
- Crash recovery: on open, every committed-but-not-applied transition is
  replayed against the underlying store.
- Deterministic disposal: file-backed `NamedTypesDecorator` instances close
  the decorated data and names stores so tests and callers can reopen the same
  sidecar files in-process.

When no transaction flag is passed at the CLI and the decorator is not
instantiated through the library API, the existing `NamedTypesDecorator`
behaviour is preserved exactly — no transitions file is written and no
extra runtime cost is paid.

## Embedding the Library

Both packages are usable as libraries, not just as the backing code of a CLI
(issue #98). The transactions layer is written against a storage abstraction
rather than against one concrete store, so an embedding application can supply
its own.

Rust:

- `storage::LinksStorage<T>` is the trait every backend implements. The CLI's
  in-memory `LinkStorage`, `PinnedTypesDecorator`, and `NamedTypesDecorator`
  implement it, and so does `storage::DoubletsStorage`.
- `storage::DoubletsStorage` is the doublets-backed implementation.
  `DoubletsStorage::open` (and the locking variants) create a file-mapped
  `doublets::unit::Store` whose links are mutated **in place**, so the inode
  never changes and other processes keep observing the same data.
  `DoubletsStorage::wrap` adopts a store the caller already owns.
- `transactions::GenericTransactionsDecorator<T, S, L>` is generic over the
  doublets address type `T`, the wrapped store `S`, and the transitions log
  `L`. `TransactionsDecorator` is the `u32` + `NamedTypesDecorator`
  specialisation `clink` uses.
- `transactions::FileTransitionLog` is an append-only, `fsync`-per-append text
  log for consumers that do not want a second links database. A tail torn by a
  crash is discarded when the log is reopened.
- Public entry points take `AsRef<Path>` and return the typed `LinkError`.

C#:

- `TransactionsDecorator<TLinkAddress>` is generic over the doublets address
  type (`IUnsignedNumber<TLinkAddress>`); the non-generic
  `TransactionsDecorator` is the `uint` specialisation the CLI uses.
- `LinksFileLock` and `StorageRevision` provide the same locking and
  external-change primitives as the Rust `storage::lock` module.

The transition wire format writes addresses in decimal, so it is identical
across address types: a log written by a `u32`/`uint` store reads back
unchanged in a `u64`/`ulong` one, and an address too wide for the target type
is rejected rather than silently truncated.

### Durability

| Store | What survives a process crash | What is needed for a machine crash |
|-------|-------------------------------|------------------------------------|
| In-memory (`LinkStorage` and the CLI decorators) | Nothing that was not saved — `flush()`/`save()` is required. | Same. |
| File-mapped (`DoubletsStorage`) | Every write: the mapping is the page cache, and the kernel writes dirty pages back. | `flush()`, which `fsync`s the mapping. A clean drop also syncs. |
| `FileTransitionLog` | Every append (`fsync` per append by default); at most the in-flight entry is lost. | Same. |

Transitions are appended to the log before the write they describe is reported
as committed, so recovery on the next open can replay committed-but-unapplied
transitions and roll back transitions that were never committed.

### Multi-Process Access

A doublets store has no internal concurrency control, so concurrent writers to
one file corrupt it. Both implementations therefore expose advisory locking of
a `<database>.lock` sidecar — shared for readers, exclusive for writers, with a
blocking acquire and a non-blocking try-acquire — plus a cheap
`StorageRevision` fingerprint (`has_external_changes()` in Rust,
`StorageRevision.HasChanged` in C#) that answers "has anyone else written since
I last looked?" without reparsing the database. Rust uses `std::fs::File::lock`
(hence the 1.89 minimum supported Rust version); C# expresses the same
semantics through `FileShare`, which the runtime maps onto `flock` on Unix and
share modes on Windows.

## Optional Version-Control Layer

The `VersionControlDecorator` (C#) and `version_control::VersionControlDecorator`
(Rust) sit *above* the transactions decorator and add three operations
over the recorded transitions log:

- **Branching** — `Branch(name, forkSeq?)` creates a new branch that
  points at an existing sequence number; `SwitchBranch(name)` rewinds or
  replays transitions so the live store matches the target branch's head.
- **Tagging** — `Tag(name, seq?)` records a stable name for any
  sequence number.
- **Time-travel checkout** — `Checkout(seq)` rewinds (or replays) the
  live store to an arbitrary sequence number.
- **Version-control transactions** — `BeginTransaction()` delegates to the
  inner transactions layer and defers branch attribution until commit; rollback
  leaves branch heads and transition-to-branch metadata unchanged.

Composition:
`LinkStorage → NamedTypesDecorator → TransactionsDecorator → VersionControlDecorator`.

Branch metadata, tags, current-branch, and applied-seq markers are all
stored inside a second sidecar doublets store so version-control state
is itself a links database.

C# trigger support stores transformation queries as links. `--always` and
`--once` create trigger records, `--never` removes matching records, and normal
write operations apply stored triggers afterward. One-shot triggers delete
themselves after a successful application.

Trigger storage can be:

- A default sidecar file derived from the primary database.
- A custom file selected by `--triggers-file`.
- The main database itself when `--embed-triggers` is enabled.

## Permissions

The permission layer (#149) is optional: without `--permissions`, or without
`Permissions` in the server options, nothing is checked and nothing changes.
It ports `createPolicy` and `withPermissions` of `@link-foundation/ui-links`
and replays its permission tests in both ports
(`PermissionsDecoratorTests.cs`, `rust/tests/permissions_decorator_tests.rs`).

Rules are links. A rule is a pair `(g: Subject Object)` marked by
`(r: AllowRead g)` or `(r: AllowWrite g)`, where `AllowRead`, `AllowWrite` and
`Anything` are named points:

- a subject matches `Subject` when it is that link or a link of that type
  (types of the `TypesDecorator` hierarchy, #148);
- a link matches `Object` when it is a value or sub-type of `Object`;
  `Anything` matches every subject or every link;
- write implies read.

The rules live in the data (`--permissions self`) or in a links file of their
own (`--permissions rules.links`); the markers are always points of the data,
so both places behave the same. Rules in the data are links like any other:
whoever may write them may change the rules.

`PermissionsDecorator` is the store as a subject sees it:

- Reads skip what the subject may not read, so a hidden link is as missing as
  a deleted one, to reads and writes alike (`Link not found`).
- Create, update and delete the subject may not make fail with `Forbidden`
  (`Forbidden: update 14`) and leave the store unchanged. An update needs the
  right to write the link before and after it. The store below may change
  more than asked (cascading deletes, merges); every link a write changes must
  be the subject's to change, or the write is undone and refused whole.
- Ownership: a link the subject created through the view is its own while
  its source is 0 (it has no type yet), so it can give it one.
- Names are references, not links: a subject may refer to a type it cannot
  read by name; naming a link needs the right to write it.
- References of a query are checked in the store below the view
  (`AdvancedMixedQueryProcessor.Options.References` in C#,
  `NamedTypeLinks::reference_exists` in Rust), so a query may refer to a link
  its subject cannot read, such as the type of its own link. A hidden link is
  still not found by reads and writes, and its address stays taken, so a
  query cannot probe which addresses are taken by creating links there. This
  is the decision #149 asked to document.

Requests are atomic. `UndoableLinksDecorator` records every write under the
view in an undo log; `clink --permissions` and every served request begin it,
commit on success and roll back on any error, undoing writes the store made
on its own, names included, even on links the subject cannot see. Triggers
(`--always`, `--once`, `--never`) run below the view, as the store itself, so
they are refused together with `--permissions`.

The server gives every connection a session (`LinksSessions`) with the
default subject (`--subject`, 0 is a guest). `(auth: token)` replies
`(subject: i)` and switches the session to the subject the authenticator
gives; an unknown token, or no authenticator, replies
`(error: 'Forbidden: authentication failed')`. The authenticator is a
callback (`LinksServerOptions.Authenticate` in C#,
`LinksSessions::with_authenticator` in Rust). `LinksTokenTable` is the one
`clink --tokens` uses: a subject `s` has a pair `(p: Token s)`, and each token
is a link `(t: p t)` named `sha256:<hex>` after the token's SHA-256, so the
store never holds a token itself. The table lives in the data
(`--tokens self`) or a file of its own; `--grant-token` and `--revoke-token`
write it, and `clink --connect --token` sends `(auth: token)` first.

`docs/protocol/links-operations.txt` is unchanged: `(auth:)` is a session
message, not a links operation.

## GraphQL for Deep

`clink --serve --graphql` (`rust/src/graphql/`,
`csharp/Foundation.Data.Doublets.Cli/Graphql/`) is a transport and a store over
the same stack: `DeepStore` reads and writes through triplets over the undoable
decorator, under `PermissionsDecorator` when a policy is given. See
[GRAPHQL.md](GRAPHQL.md).

## CI Workflows

| Workflow | Scope |
|----------|-------|
| `.github/workflows/csharp.yml` | .NET restore, build, tests, package, and release. |
| `.github/workflows/rust.yml` | Rust formatting, clippy, file-size gate, tests, package, and release. |
| `.github/workflows/wasm.yml` | Rust core tests, wasm-pack tests, Vite build, artifact upload, and manual Pages deployment. |
| `.github/workflows/js-client.yml` | `@link-foundation/link-cli-client` unit tests and type declarations, conformance against the Rust, C# and WebAssembly backends on Node.js, Bun and Deno, the composition example, and npm release with trusted publishing. |
| `.github/workflows/deep-client.yml` | The real `DeepClient` against the Rust and C# `clink --serve --graphql`. |
| `.github/workflows/docs.yml` | Builds the C# (DocFX), Rust (rustdoc) and workbench documentation into one site. |
| `.github/workflows/links.yml` | Checks the links of every Markdown and HTML file with lychee, falling back to the Web Archive. |
| `.github/workflows/security.yml` | CodeQL, dependency review, `cargo audit` of every committed `Cargo.lock`, and `npm audit`. |
| `.github/workflows/workflows.yml` | actionlint, zizmor, and the workflow policy tests. |
| `.github/workflows/dependencies.yml` | Fails while any Cargo, NuGet or npm dependency, lock file entry, GitHub Action, or the Node.js or .NET version declared in any tracked manifest is behind its latest release, unless a comment on its manifest line links an open issue that blocks the update. Runs on pull requests and daily; `.github/dependabot.yml` opens the update pull requests. |

Path filters keep most workflows focused on the parts of the repository they
own.

## External References

- NuGet `clink`: https://www.nuget.org/packages/clink
- crates.io `link-cli`: https://crates.io/crates/link-cli
- Rust `doublets`: https://docs.rs/doublets/latest/doublets/
- Rust `links-notation`: https://docs.rs/crate/links-notation/0.26.0
- npm `doublets-web`: https://www.npmjs.com/package/doublets-web
- WebAssembly local docs and implementation notes: [../js/README.md](../js/README.md)
