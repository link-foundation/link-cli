# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [5.0.0] - 2026-11-05

A reference to an unreadable link needs its own right (#161). Without it an unreadable address answers exactly as a missing one, in the view, in operations and in queries, and the name of a link the subject may not refer to is an unknown name that cannot be taken over. A subject may refer to a link it may read, to itself, to what a rule `(r: AllowReference (g: Subject Object))` covers (`Object` and the links of type `Object`), and to the `Object` of its own `AllowRead` and `AllowWrite` rules. `AllowReference` does not allow reading; a reference a link already holds may stay; choosing the address of a new link is a reference; the write check runs first. Breaking: a subject that referred to an unreadable link needs a rule now, and a rule `Anything` to `Anything` brings the previous behaviour back. `PermissionMarkers<T>` has a fourth parameter `AllowReference`, `PermissionAccess` a new value `Reference`, `IPermissionPolicy<T>` a method `CanReference` that defaults to `false`, and `ITriplets<T>` a method `ReferenceIsTriplet`. `AdvancedMixedQueryProcessor.Options.References` and the `references` parameter of `LinksServer.ExecuteRequest` are gone, as a query checks its references through the store it runs on; a store may implement the new `IReferencedLinks<T>` to answer them.

## [4.5.0] - 2026-11-05

Serve triplets over the Links Notation text and binary protocol (#152). `clink --serve --triplets` (`LinksServerOptions.Triplets`) answers `(count: (i T f t))`, `(each: (i T f t))`, `(create: (T f t))`, `(update: (i T f t))`, `(delete: i)` and three-part substitution queries with `(i: T f t)` triplets, keeps names and `(auth: token)`, and leaves a server without the flag as it was. Subscriptions push triplet changes and permissions judge them through the new `TripletsPermissionsDecorator`. A binary client whose packet asks for `--arity 2..3` gets each triplet as one link of three references, `(i: (T f t))`, and every parser reads both forms. `RemoteLinks` implements `ITriplets<uint>` and adds `SubscribeTriplets` and `ReceiveTripletNotification`, `TripletsOperation` holds the requests and replies, and `docs/protocol/triplets.txt` is the conversation both ports replay.

## [4.4.0] - 2026-11-05

Add `TripletsOverDoublets<TLinkAddress>` and `ITriplets<TLinkAddress>` (#151): triplets `(index: type from to)` stored in any doublets store with an associative API. A node `(i: T 0 0)` is the doublet `(i: T i)` and a link `(i: T f t)` is `(i: T ((Pair f) t))`, with the points `Pair` and `Nothing` found or created by name, so the type stays the source and `TypesDecorator` reads triplets as values of their types. `CreateTriplet`, `UpdateTriplet` (the address stays), `DeleteTriplet` (cascading) and `EachTriplet` / `CountTriplets` by index, type, from and to leave no unused wrapper or pair behind. `TripletsQueryProcessor.ProcessQuery` runs substitution queries with three parts, such as `((($r: knows alice bob)) (($r: knows alice carol)))`, and `TripletsLino` imports and exports `(index: type from to)` lines. The encoding and its costs are in `docs/TRIPLETS.md`.

Also updates `Platform.Data.Doublets` to 0.18.8.

## [4.3.0] - 2026-11-03

Add an optional permission layer with permissions stored in links (#149), the port of `createPolicy` and `withPermissions` of `@link-foundation/ui-links` with its tests. A rule is a pair `(g: Subject Object)` marked by `(r: AllowRead g)` or `(r: AllowWrite g)`; `Anything` matches everything and write implies read. `PermissionPolicy<TLinkAddress>` reads the rules from the data or from a store of their own, and `PermissionsDecorator<TLinkAddress>` is the store as a subject sees it: reads skip what it may not read, and a create, update or delete it may not make throws `ForbiddenException` (`Forbidden: update 14`) and changes nothing. A link the subject created is its own while its source is 0. `UndoableLinksDecorator<TLinkAddress>` makes a request atomic, and the query processor checks references in the store below the view (`Options.References`).

`clink` gains `--permissions self|<rules.links>`, `--subject`, `--tokens`, `--grant-token`, `--revoke-token` and `--token`. A server with `LinksServerOptions.Permissions` gives every connection a session (`LinksSessions`); `(auth: token)` replies `(subject: i)` through `LinksServerOptions.Authenticate`, any callback, or `LinksTokenTable`, a token table stored as links named after the SHA-256 of each token. Without `--permissions` nothing changes.

The dependency `Platform.Data.Doublets` is updated to 0.18.7.

Add layers that compose with it and with each other, none depending on another. `UnionLinksDecorator<TLinkAddress>` shows many links networks as one (a client's links joined with a server's, a cache with its source): the first member that has an address shadows the rest, a new link is created in the first member, and an update, a delete or a name goes to the member that has the link. `RemapLinksDecorator<TLinkAddress>` shows a network under other addresses through an `IAddressMap<TLinkAddress>`: `OffsetAddressMap<TLinkAddress>` moves it out of the way of another one, and `AddressTable<TLinkAddress>` gives addresses in the order they are seen. `LinksExternalIds` binds the ids of other systems (UUIDs, ObjectIds, URLs) to links, in the data or in a store of its own, in the layout the Rust and JavaScript ports share. `IComposedLinks<TLinkAddress>` (`Claims`, `EnsureCreated`) lets the undo of a request recreate a link in the member it came from, so a `PermissionsDecorator<TLinkAddress>` over a union restricts every member at once and one under it a single member.

## [4.2.0] - 2026-11-03

Added subscriptions to the LiNo protocol server (#150). `(subscribe: (index source target))` answers
`(subscription: N)`, and every later change another connection makes to a link matching the pattern is pushed to the
subscriber as `(notify: N)` followed by its `(before) (after)` changes; `(unsubscribe: N)` answers nothing, and closing
the connection drops its subscriptions. `LinksServerOptions.Readable` hides links from subscribers (a link that becomes
unreadable arrives as a deletion) and `LinksServerOptions.MaxPendingNotifications` disconnects a subscriber that stops
reading. `LinksClient` sets pushes aside for `ReceiveNotification`, and `RemoteLinks` gains `Subscribe`, `Unsubscribe`
and `ReceiveNotification`.

Updated Platform.Data.Doublets to 0.18.7.

## [4.1.0] - 2026-11-02

Add `TypesDecorator<TLinkAddress>` and `ITypes<TLinkAddress>` (#148): a type hierarchy `Type -> SubType -> … -> Value` with multiple inheritance kept in plain links. The type of a link is its source, the root is `(Type: Type Type)`, a sub-type is `(S: T S)` and extra parents are pairs `(p: S T2)` marked by `(m: Extends p)`. `TypeOf`, `IsType`, `ParentsOf`, `AncestorsOf`, `IsA` and `IsOf` terminate on cycles, and `Define`, `Extend` and `Instance` write. The decorator is not sealed, and pinned types are sub-types of its root.

Update Platform.Data.Doublets to 0.18.6 and System.CommandLine to 2.0.14. Counting `(* point)` now visits a self-referencing point once. Since 0.18.6 a merging update also ends by reporting the link it merged into, unchanged; `LinksUniquenessAndUsagesRepointingResolver` does not pass that notice on, so `clink --changes` and the protocol server still report only the links a write changed, as the Rust port does (creating a duplicate replies with no change).

## [4.0.6] - 2026-10-24

Harden the C# release automation (issue #134): the release commit is rebased
onto the latest main before any file is touched, a push that loses the race to
a concurrent release is rebased and retried while a repository-rule rejection
fails fast with an explicit error, the tag is created only after the commit
reached main and exactly that tag is pushed, git runs without a shell so
release descriptions are passed verbatim, and CHANGELOG.md no longer gains an
extra blank line on every release.

The NuGet and GitHub release lookups that decide whether to release now retry
network failures, 429 and 5xx with backoff and report the network cause, so a
single dropped connection no longer fails the release.

A changeset whose bump type cannot be parsed now fails the release with an
"Invalid changeset" error naming the file. It used to be skipped with a
warning and then deleted, so its bump and description were silently lost.

`LinksServer` no longer crashes the process when it shuts down right after
accepting a connection. The worker used to get its stream while `Shutdown`
disposed the client, and `TcpClient` throws `ArgumentNullException` when that
happens midway; the stream is now taken in the accept loop, under the lock
`Shutdown` holds.

## [4.0.5] - 2026-10-24

Preserve quoted query references as names, including numeric names such as "0"
and "1", throughout reference validation, creation, matching and mutation. Bare
numbers remain link addresses. Encoded reference literals also resolve as names.
Stored triggers preserve this distinction when normalized and replayed, and
numeric names exported as LiNo can be replayed through queries.

## [4.0.4] - 2026-10-24

Parse and validate the whole LiNo input before changing links or names, so malformed imports leave the database unchanged through both the CLI aliases and the library APIs.

## [4.0.3] - 2026-10-24

Preserve literal backslashes and multiline quoted names during LiNo import. Quote
backslash-containing names on export, and use the existing UTF-8 hexadecimal
reference literals for control characters and mixed quotes so user text survives
import/export/import into a fresh database.

## [4.0.2] - 2026-10-15

Synchronize protocol documentation with links-notation 0.26 inherited from main.
Verify legacy quoted input and lossless CRLF transport. Binary version 1
encoding is unchanged.

## [4.0.1] - 2026-10-15

Update Links Notation, System.CommandLine, and test dependencies to satisfy the latest-dependency CI gate. Verify canonical lossless reference literals while retaining coverage for legacy multiline text framing.

Quote numeric names in LiNo text exports and preserve quoted references as names on import, preventing numeric names from being confused with link addresses.

Handle a socket disconnected before its connection worker starts so server shutdown cannot crash the process.

## [4.0.0] - 2026-10-07

The binary links notation codec now comes from `Link.Foundation.Links.Notation.Binary` (links-notation 0.23.0) instead of a local copy. `ArityRange`, `LinksPacket`, `Section`, `PacketReference`, `DecodeLimits`, `LinoMapping` and `LinoFormat` are removed from `Foundation.Data.Doublets.Cli.Protocol`; import them from `Link.Foundation.Links.Notation.Binary`. `LinoStreamReader` is replaced by `PacketReader`.

- `DecodeLimits.MaxTextBytes` moves to `TextLinoProtocol.MaxTextBytes`. The new `ProtocolLimits` record (`Binary`, `MaxTextBytes`, `Default`, `Unlimited`) is now the type taken by `LinksServerOptions.Limits`, `MessageFormat.Protocol` and `LinoProtocols.ReadAnyDocument`.
- `BinaryLinoProtocol` now enforces `Limits` when it encodes too. The upstream defaults apply, so the default `MaxDepth` is 64 and there is a new `MaxStringBytes` limit.
- The protocols still raise only `LinoProtocolException`. A codec `BinaryNotationException` is converted with the same kind and detail and is kept as `InnerException`, and `LinoProtocolException.From` exposes this conversion.

## [3.2.0] - 2026-10-05

Fixed the `--changes` report of creations:

- Every link a query creates is reported, named point links and leaves created on the way included. Each one is reported once, as `() ((name: name name))`, not as an empty link filled in later.
- Creating a link under an existing name redefines that link in place, and a new name for an existing doublet names it instead of copying it.
- Reference validation predicts the addresses new links get by asking the store. Stores reuse the address freed last first, so the lowest free address is not always next. A reference to a freed address the query does not refill is now reported missing instead of being dropped.
- `EnsureCreated` no longer throws when the store hands out a freed address above the target before reaching it.

A query that refers to a missing link or does not parse is reported in one `Error: Query error: ...` or `Error: Parse error: ...` line on stderr, as the Rust clink reports it, instead of an unhandled exception with its stack trace. `--trace` still prints the stack trace.

Updated `System.CommandLine` to 2.0.12, and the test dependencies `Microsoft.NET.Test.Sdk` to 18.10.1 and `coverlet.collector` to 10.1.0. A new `Dependencies` workflow fails while any dependency is behind its latest release, and Dependabot opens the update pull requests.

Added LiNo substitution operations over TCP (issue #105). There are two protocols, and either can replace the other:

- `TextLinoProtocol`: dot-stuffed UTF-8 messages.
- `BinaryLinoProtocol`: each packet is a list of sections of links that share one reference width (8, 16, 32 or 64 bits) and one arity range. The default is doublets with one uniform width. Options add external (Hybrid) references, any arity range such as `2..3` or `1..` (`ArityRange`), and packed per-section widths; packed output is never larger than uniform output. Shared golden vectors in `docs/protocol/binary-links-notation-vectors.txt` pin the format.

`LinksServer` and `LinksClient` run create, read, update and delete queries over either protocol. The new `clink --serve` and `--connect` options do the same from the command line, together with `--protocol`, `--external-references`, `--arity` and `--packed-widths`. The wire format is byte-for-byte the same as the Rust port's, so C# and Rust servers and clients work with each other.

Upgraded Link.Foundation.Links.Notation to 0.22.0. In earlier versions, parse time grew exponentially with nesting depth, so one short message could stall a server.

An update into an existing doublet merges into it and now re-points every link that used the merged-away address at the surviving one, as the Rust port does. Before, the `MergeUsages` of Platform.Data.Doublets 0.18.1 ([Data.Doublets#515](https://github.com/linksplatform/Data.Doublets/issues/515)) blanked the half of the usage instead, so `() ((1 2) (2 1))` followed by `((1: 1 2)) ((1: 2 1))` left `(2: 2 0)` rather than `(2: 2 2)`. Stores are composed with the new `DecorateWithAutomaticUniquenessAndUsagesRepointing()`, whose top layer, `LinksUniquenessAndUsagesRepointingResolver`, replaces `LinksCascadeUniquenessAndUsagesResolver`.

`LinksPacket.AddressTier` now throws an unencodable `LinoProtocolException` for an address beyond the internal range instead of silently returning width 8, and the unused `LinoFormat.IsReference` was removed. A server that is stopping now answers a request it already read with `(error: 'server is shutting down')` instead of hanging up, and its trace notes when a client hangs up. `LinksPacket.WidthFromCode` is total over two-bit codes, and the accept loop no longer keeps a separate branch for a client accepted while stopping (its worker closes it). Tests now cover every line of the protocol code, and each malformed-packet test asserts the exact error, which fixed three cases that passed for the wrong reason.

Added `RemoteLinks`, an `INamedTypesLinks<uint>` whose links live behind a `LinksServer`. Code written against the links interface, the query processor included, switches from a local file to a server by swapping one value. `LinksServer` now also answers `LinksOperation` documents, one per interface call: `(count: …)`, `(each: …)`, `(create: …)`, `(update: …)`, `(delete: …)`, `(get-name: …)`, `(set-name: …)`, `(get-by-name: …)` and `(remove-name: …)`. These are the documents the Rust `RemoteLinks` sends, so either port's client works with either port's server. Both test suites replay the same recorded conversation, `docs/protocol/links-operations.txt`, to prove it. A write replies with the net change of every link it touched.

Fixed a crash of a `LinksServer` shut down right after it accepted a connection: the connection thread set an option on the socket the shutdown had already disposed, and the `NullReferenceException` ended the process.

Added `--export-binary <PATH>` (aliases `--binary-output`, `--binary-out`) and `--import-binary <PATH>` (aliases `--binary-input`, `--binary-in`), which write and read the whole store, names included, as a store archive in binary links notation: a links packet that keeps every address and hole, followed by a names packet. The library exposes the same as `StoreArchive.Export`, `StoreArchive.Import`, `StoreArchive.ExportToFile` and `StoreArchive.ImportFromFile`. A binary archive is imported before the `--in` LiNo file and exported wherever `--out` is written. The archive bytes match the Rust port.

## [3.1.0] - 2026-08-29

Opened the library up for extension: every decorator (`NamedTypesDecorator`, `NamedLinksDecorator`, `SimpleLinksDecorator`, `PinnedTypesDecorator`, `TransactionsDecorator`, `VersionControlDecorator`, `PersistentTransformationDecorator`) is now unsealed with overridable members, disposable ones follow the `protected virtual void Dispose(bool)` pattern so a subclass can release resources of its own, and `PersistentTransformationDecorator.PersistentTransformationQuery` and `InternalNamePrefix` are public. A custom CLI can now subclass any layer of the stack instead of forking it.

## [3.0.0] - 2026-08-29

Refreshed every C# dependency to its latest release and retargeted the
packages to `net10.0` (issue #98). `Link.Foundation.Links.Notation`
moves 0.13.0 -> 0.16.1 so the C# and Rust implementations parse LiNo
with the same version of the same grammar; that release only ships a
`net10.0` assembly, so `Directory.Build.props` and all three projects
now target `net10.0` and CI provisions the .NET 10 SDK. `System.CommandLine`
moves 2.0.7 -> 2.0.11, and the test project picks up
`Microsoft.NET.Test.Sdk` 18.9.0, `xunit.runner.visualstudio` 4.0.0 and
`coverlet.collector` 10.0.1.

This is a breaking change for consumers still on `net8.0`: upgrade to
the .NET 10 SDK/runtime before taking this release.

Made the C# transactions layer reusable from outside the CLI, matching
what the Rust library now offers (issue #98).

`TransactionsDecorator` is generic over the doublets address type:
`TransactionsDecorator<TLinkAddress>` works over any
`INamedTypesLinks<TLinkAddress>` whose address is an
`IUnsignedNumber<TLinkAddress>`, so a consumer with a `ulong`-addressed
store is a first-class user rather than being pinned to `uint`. The
non-generic `TransactionsDecorator` remains as a `uint` specialisation,
so existing code that constructs it keeps compiling. The transitions
wire format is unchanged and address-type independent — addresses are
written in decimal under the invariant culture, so a log written by a
`uint`-addressed store reads back unchanged in a `ulong`-addressed one,
and an address that does not fit the target type is rejected instead of
being silently truncated.

New `LinksFileLock` and `StorageRevision` cover multi-process access:
advisory locking of a database's `.lock` sidecar (shared for readers,
exclusive for writers, with a blocking `Acquire` and a non-blocking
`TryAcquire`) and a cheap "has anyone else written since I last looked?"
fingerprint. The lock file path and the shared/exclusive semantics match
the Rust `storage::lock` module, so the two implementations can guard the
same database.

Source-breaking: `Transition`, `ITransaction` and `ITransactionsLinks`
are now generic. Existing `uint` code should use `Transition<uint>`,
`ITransaction<uint>` and `ITransactionsLinks<uint>`.

## [2.6.0] - 2026-08-18

Added optional transactions and version-control layers (issue #94). The
new `TransactionsDecorator` records each Create/Update/Delete as a
reversible transition in a sidecar doublets store and exposes
`BeginTransaction()` / `Commit()` / `Rollback()` plus three retention
policies (`infinite`, `sized:<n>`, `chunked:<n>:<dir>`) and two commit
modes (`sync`, `async`). The new `VersionControlDecorator` adds
branching, tagging, and time-travel checkout over that log. The CLI
surfaces both layers through `--transactions`, `--transactions-file`,
`--commit-mode`, `--retention`, `--log`, `--vc`, `--vc-file`,
`--branch`, `--branch-from`, `--checkout`, `--tag`, `--list-branches`,
and `--list-tags`. When no flag is passed, behaviour is byte-identical
to the existing CLI — no sidecar is written and no extra cost is paid.

Hardened the C# build and the pipelines around it (issue #96).
`Directory.Build.props` now turns warnings into errors and enables the
.NET analyzers, and `TransactionsDecorator` / `VersionControlDecorator`
implement `IDisposable` so the memory-mapped databases they own are
released deterministically — the leak that made the Windows test job
fail while the pipeline still reported success. The C# workflow no
longer masks those Windows failures with `continue-on-error`, verifies
formatting and file sizes, and finally implements the `changeset-pr`
release mode it had been advertising without handling.
Pull requests also re-run the build and tests on a simulated merge with
the tip of `main`, and the coverage upload moved to
`codecov/codecov-action@v7` to stop the Node.js 20 deprecation warning.

## [2.5.0] - 2026-05-15

Split the C# distribution into two NuGet packages so external .NET
projects can consume the public library without pulling in the
`dotnet tool` packaging:

- `clink` — unchanged dotnet tool, now built from a CLI csproj that only
  contains `Program.cs` and `System.CommandLine` wiring.
- `Foundation.Data.Doublets.Cli` — new library package that ships the
  parser, query processors (basic / advanced / mixed), `ChangesSimplifier`,
  named/pinned type decorators, persistent transformation trigger
  decorator, LiNo I/O adapters, the `UnicodeStringStorage` extension, and
  every other reusable building block. Generated XML doc comments are
  packed alongside the assembly and rendered into a DocFX site published
  to GitHub Pages.

## [2.4.0] - 2026-05-12

Added `--export` as an alias for `--out` database export.

Added `--in`/`--lino-input`/`--import` database import support for reading LiNo files into the links database with named references enabled by default.

Added `--out`/`--lino-output` database export support that writes the complete links database as LiNo with named references when available.

Added a universal `NamedTypesDecorator` that implements both links operations and named type lookups, with automatic cleanup and uniqueness checks for external-reference names.

Added binary links-backed persistent transformation triggers with `--always`, `--once`, `--never`, `--triggers-file`, and `--embed-triggers`.

Added `IPinnedTypes` and `PinnedTypesDecorator`, and composed pinned type support into `NamedTypesDecorator`.

Fixed self-link substitution with outgoing links by preserving unbound substitution parts from the matched link and rejecting unsupported link addresses during explicit creation.

Fixed explicit indexed numeric updates so auto-created numeric references do not steal the substitution pair, and added issue 62 regression coverage.

Moved C# release automation into `csharp/scripts/` and packaged the C# README
with the NuGet tool.

Added full string ID alias support for advanced LiNo queries through the named types decorator.

Updated the C# LiNo parser dependency to the current `Link.Foundation.Links.Notation` package and refreshed supported NuGet package versions.

Added strict validation for missing numeric and named link references, plus `--auto-create-missing-references` to create missing references as self-referential point links.
