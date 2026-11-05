# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- changelog-insert-here -->

## [2.1.1] - 2026-11-05

Update links-notation to 0.30.1 while auditing the associative architecture roadmap.

Preserve quoted names in triplet imports and substitution queries, and export names without turning numeric names, wildcards or variables into query syntax.

## [2.1.0] - 2026-11-05

Added `clink --serve <address> --graphql`: a Hasura-compatible GraphQL server implementing the Deep Standard, so `DeepClient` from `@deep-foundation/deeplinks` and DeepCase connect to link-cli. See `docs/GRAPHQL.md`.

## [2.0.0] - 2026-11-05

A reference to an unreadable link needs its own right (#161). Without it an unreadable address answers exactly as a missing one, in the view, in operations and in queries, and the name of a link the subject may not refer to is an unknown name that cannot be taken over. A subject may refer to a link it may read, to itself, to what a rule `(r: AllowReference (g: Subject Object))` covers (`Object` and the links of type `Object`), and to the `Object` of its own `AllowRead` and `AllowWrite` rules. `AllowReference` does not allow reading; a reference a link already holds may stay; choosing the address of a new link is a reference; the write check runs first. Breaking: a subject that referred to an unreadable link needs a rule now, and a rule `Anything` to `Anything` brings the previous behaviour back. `PermissionMarkers` has a new field `allow_reference`, `PermissionAccess` a new variant `Reference`, `AccessPolicy` a method `can_reference` that defaults to `false`, and `Triplets` a method `reference_is_triplet`.

## [1.5.0] - 2026-11-05

Serve triplets over the Links Notation text and binary protocol (#152). `clink --serve --triplets` (`ServerOptions::triplets`) answers `(count: (i T f t))`, `(each: (i T f t))`, `(create: (T f t))`, `(update: (i T f t))`, `(delete: i)` and three-part substitution queries with `(i: T f t)` triplets, keeps names and `(auth: token)`, and leaves a server without the flag as it was. Subscriptions push triplet changes and permissions judge them through the triplets view of `PermissionsDecorator`. A binary client whose packet asks for `--arity 2..3` gets each triplet as one link of three references, `(i: (T f t))`, and every parser reads both forms. `RemoteLinks` implements `Triplets` and adds `subscribe_triplets` and `receive_triplet_notification`, `triplets_operations` holds the requests and replies, and `docs/protocol/triplets.txt` is the conversation both ports replay.

## [1.4.0] - 2026-11-05

Add the `Triplets` trait and `TripletsOverDoublets` (#151): triplets `(index: type from to)` stored in any `NamedTypeLinks` with an associative API, the twin of the C# `TripletsOverDoublets` with the same tests. A node `(i: T 0 0)` is the doublet `(i: T i)` and a link `(i: T f t)` is `(i: T ((Pair f) t))`, with the points `Pair` and `Nothing` found or created by name, so the type stays the source and `TypesDecorator` reads triplets as values of their types. `create_triplet`, `update_triplet` (the address stays), `delete_triplet` (cascading) and `each_triplet` / `count_triplets` by index, type, from and to leave no unused wrapper or pair behind. `process_triplets_query` runs substitution queries with three parts and `triplets_to_lino` / `import_triplets_lino` export and import `(index: type from to)` lines. The encoding and its costs are in `docs/TRIPLETS.md`; `cargo run --example triplets` walks through it.

## [1.3.0] - 2026-11-03

Add an optional permission layer with permissions stored in links (#149), the port of the C# one with the same tests, which replay those of `@link-foundation/ui-links`. A rule is a pair `(g: Subject Object)` marked by `(r: AllowRead g)` or `(r: AllowWrite g)`; `Anything` matches everything and write implies read. `PermissionPolicy` reads the rules from the data (`in_data`) or from a store of their own (`with_rules`), and `PermissionsDecorator` is the store as a subject sees it: reads skip what it may not read, and a create, update or delete it may not make fails with `Forbidden` (`Forbidden: update 14`) and changes nothing. A link the subject created is its own while its source is 0. `UndoableLinksDecorator` makes a request atomic, and `NamedTypeLinks` gains `reference_exists`, so references are checked in the store below the view.

`clink` gains `--permissions self|<rules.links>`, `--subject`, `--tokens`, `--grant-token`, `--revoke-token` and `--token`. `LinksSessions` (`with_permissions`, `with_subject`, `with_authenticator`) gives every connection of `LinksServer::serve_sessions` a session; `(auth: token)` replies `(subject: i)` through any authenticator callback or `LinksTokenTable`, a token table stored as links named after the SHA-256 of each token. Without `--permissions` nothing changes.

Add layers that compose with it and with each other, none depending on another. `UnionLinks` shows many links networks as one (a client's links joined with a server's, a cache with its source): the first member that has an address shadows the rest, a new link is created in the first member, and an update, a delete or a name goes to the member that has the link. `RemapLinks` shows a network under other addresses through an `AddressMap`: `OffsetAddressMap` moves it out of the way of another one, and `AddressTable` gives addresses in the order they are seen. `LinksExternalIds` binds the ids of other systems (UUIDs, ObjectIds, URLs) to links, in the data or in a store of its own, in the layout the C# and JavaScript ports share. `NamedTypeLinks` gains `claims`, so the undo of a request recreates a link in the member it came from, and `NamedLinksStorage` lets the transactions and version control layers sit over any of these views: a `PermissionsDecorator` over a union restricts every member at once, and one under it a single member.

Update `doublets` to 0.19.0 and `sha2` to 0.11.1, with the lock files.

## [1.2.0] - 2026-11-03

The LiNo protocol server now supports subscriptions (#150).

- `(subscribe: (index source target))` replies `(subscription: N)`.
- Later changes that other connections make to matching links are pushed as `(notify: N)` followed by their `(before) (after)` lines. Pushes and replies arrive in the order the store changed.
- `(unsubscribe: N)` replies nothing, and closing a connection drops its subscriptions.
- `LinksServer::with_readable` hides links from subscribers, so a link that becomes unreadable arrives as a deletion.
- `ServerOptions::max_pending_notifications` disconnects subscribers that fall too far behind.
- `LinksClient` sets pushes aside for `receive_notification`.
- `RemoteLinks` gains `subscribe`, `unsubscribe` and `receive_notification`.

Both ports replay the shared conversation in `docs/protocol/subscriptions.txt`.

Updated `doublets` to 0.18.0.

## [1.1.0] - 2026-11-02

Add the `Types` trait and `TypesDecorator` (#148): a type hierarchy `Type -> SubType -> … -> Value` with multiple inheritance kept in plain links, the port of the C# `TypesDecorator` with the same tests. The type of a link is its source, the root is `(Type: Type Type)`, a sub-type is `(S: T S)` and extra parents are pairs `(p: S T2)` marked by `(m: Extends p)`. `type_of`, `is_type`, `parents_of`, `ancestors_of`, `is_a` and `is_of` terminate on cycles, and `define`, `extend` and `instance` write. `NamedTypeLinks` gains `query_links`, which finds links by a pattern.

Update `doublets` to 0.17.0, `links-notation` to 0.30.0 and the WebAssembly `wasm-bindgen` family to their latest releases, with the lock files. With doublets 0.17 a cascading delete resets every usage to `(0 0)` before deleting it, as C# does; the net change reported for each link is unchanged.

## [1.0.9] - 2026-10-24

Expanded Rust query processor parity with the C# implementation, including unwrapped query pairs, structural and wildcard deletes, variable-driven updates, named composite rename behavior, direct Rust basis dependencies on doublets, links-notation, and lino-arguments without a direct clap dependency, and a doublet-backed Rust port of the C# Unicode string storage path with explicit Data.Doublets.Sequences-style converter abstractions.

Added `--out`/`--lino-output` database export support that writes the complete links database as LiNo with named references when available.

Added Rust `NamedTypes` and `NamedTypesDecorator` parity for link storage with external-reference names, name cleanup on delete, and unique name reassignment behavior.

Added strict validation for missing numeric and named link references, plus `--auto-create-missing-references` to create missing references as self-referential point links.

Added Rust pinned type decorator parity and composed pinned type access into the named types decorator.

Routed Rust query processing and CLI string ID aliases through the named types decorator so aliases resolve from the separate names database, matching C# full string ID behavior.

Added `--export` as an alias for `--out` database export.

Added `--in`/`--lino-input`/`--import` database import support for reading LiNo files into the links database with named references enabled by default.

Fixed issue 20 query parity by preserving unbound substitution parts from existing matched links and rejecting unsupported explicit creation addresses.

Added issue 62 regression coverage for reversible explicit updates, finite pinned types, unsupported placeholders, and stale named-link mappings.

Moved Rust release automation into `rust/scripts/`, added crates.io publication
checks, and relocated the WebAssembly wrapper crate under `rust/wasm/`.

Documented the dual `[lib]` + `[[bin]]` nature of the `link-cli` crate and
added crates.io metadata (`description`, `documentation`, `homepage`,
`keywords`, `categories`) so the public library surface is discoverable
from crates.io and docs.rs.

Added optional transactions and version-control layers (issue #94).
`transactions::TransactionsDecorator` records each Create/Update/Delete
as a reversible transition in a sidecar doublets store and exposes
`begin_transaction()` / `commit()` / `rollback()` plus three retention
policies (`infinite`, `sized:<n>`, `chunked:<n>:<dir>`) and two commit
modes (`sync`, `async`). `version_control::VersionControlDecorator`
adds branching, tagging, and time-travel checkout over the recorded
log. The `clink` binary surfaces both layers through `--transactions`,
`--transactions-file`, `--commit-mode`, `--retention`, `--log`,
`--vc`, `--vc-file`, `--branch`, `--branch-from`, `--checkout`,
`--tag`, `--list-branches`, and `--list-tags`. When no flag is passed,
behaviour is byte-identical to the existing CLI — no sidecar is
written and no extra cost is paid.

Hardened the Rust pipeline (issue #96). Every job now declares a
timeout and a least-privilege token, release jobs run in a
non-cancellable writer concurrency group, the advertised
`changelog-pr` release mode is implemented, and the lint job fails on
`Cargo.lock` drift. `anyhow` and `memmap2` were refreshed in both
lockfiles to clear the outstanding RUSTSEC advisories.
`cargo clippy` now runs with `-D warnings` and also covers the `rust/wasm`
workspace, which had never been formatted or linted, and pull requests
re-run the fast checks on a simulated merge with the tip of `main` so a
semantic merge conflict fails the pull request instead of `main`. The
pattern-matching helpers moved from `query_processor.rs` into
`query_processor/matching.rs` to clear the file-size warning; no public API
changed.

Refreshed every Rust dependency to its latest release (issue #98):
`doublets` 0.3.0 -> 0.4.0, `links-notation` 0.13.0 -> 0.16.1,
`thiserror` 1.0 -> 2.0.20 and `anyhow` -> 1.0.104 in the CLI workspace,
plus `wasm-bindgen` 0.2.127, `serde` 1.0.229, `serde_json` 1.0.151,
`web-sys` 0.3.104 and `wasm-bindgen-test` 0.3.77 in the WASM workspace.
The `doublets` bump matters beyond hygiene: an external crate that
already depends on `doublets` 0.4 can now link against `link-cli`
without pulling two semver-incompatible copies of the same store into
one binary.

Fixed opening a `transactions::FileTransitionLog` whose tail was left
half-written by a crash on Windows (issue #98). The torn tail is trimmed
with `File::set_len`, but Windows grants an append-only handle
`FILE_APPEND_DATA` without `FILE_WRITE_DATA`, so the call failed with
`ERROR_ACCESS_DENIED` and the log could not be reopened at all. The
repair now runs through a dedicated read/write handle, and the
append-only handle is opened afterwards.

Also replaced a `chunks_exact(2)` loop in the balanced variant converter
with `as_chunks::<2>()`, which the Rust 1.98 `clippy::chunks_exact_to_as_chunks`
lint rejects under `-D warnings`.

Made the Rust library reusable as a doublets-backed transactional store
(issue #98).

`storage::LinksStorage<T>` is the trait the transactions layer is now
written against, so any doublets-compatible store can be composed under
it. `storage::DoubletsStorage` implements it over a real
`doublets::unit::Store`: `DoubletsStorage::open` creates a file-mapped
store whose links are mutated **in place** (the inode never changes, so
other processes that already mapped the file keep seeing the same data),
and `DoubletsStorage::wrap` adopts a store the embedding application
already owns. The CLI's own text-file `LinkStorage` implements the same
trait, so nothing about `clink` changed.

`GenericTransactionsDecorator<T, S, L>` is generic over the doublets
address type, the wrapped store and the transitions log, so a consumer
with a `unit::Store<usize, _>` is a first-class user rather than being
pinned to `u32`; `TransactionsDecorator` remains the `u32` +
`NamedTypesDecorator` specialisation `clink` uses. The transition wire
format writes addresses in decimal and is therefore identical across
address types, and an address that does not fit the target type is
reported as `LinkError::AddressOutOfRange` instead of being truncated.
`transactions::FileTransitionLog` adds a plain append-only,
`fsync`-per-append log for consumers that do not want a second links
database; a torn tail left by a crash is discarded when it is reopened.

`storage::lock` adds advisory file locking over a `<database>.lock`
sidecar (shared for readers, exclusive for writers, blocking `acquire`
and non-blocking `try_acquire`), surfaced as
`DoubletsStorage::open_shared` / `open_exclusive` /
`try_open_exclusive`, and `LinksStorage::has_external_changes` answers
"has anyone else written since I last looked?" from a cheap
`StorageRevision` fingerprint. Durability is now stated explicitly:
writes to a file-mapped store survive a process crash without any
`save()`, while surviving a machine crash requires the `fsync` that
`LinksStorage::flush` performs; recovery replays committed-but-unapplied
transitions and rolls back uncommitted ones, and is covered by tests
that crash a file-backed store mid-rebuild.

The public storage and transactions APIs take `AsRef<Path>` rather than
`&str` and return the typed `LinkError` rather than `anyhow::Error`.

Building the crate now requires Rust 1.89, the release that stabilised
`std::fs::File::lock`.

Updated the `doublets` basis crate to 0.5.0 (both the `link-cli` and `clink-wasm` lockfiles), which brings the upstream `doublets::decorators` layer into reach for the Rust storage stack.

Reused the `doublets` 0.5.0 decorator layer in the Rust storage stack: `DoubletsStorage::map_store` composes any upstream (or custom) decorator onto an open database while keeping its path, advisory lock and change-detection fingerprint, and `DoubletsStorage::with_automatic_uniqueness_and_usages_resolution` applies the same stack C# gets from `ILinksExtensions.DecorateWithAutomaticUniquenessAndUsagesResolution`. The `doublets` crate and its `decorators` module are re-exported from `link_cli` so downstream crates can build custom stacks without a version-mismatched direct dependency.

Routed the CLI's `LinkStorage` through the upstream `doublets` uniqueness and cascade resolvers, so a delete now removes the links that referenced the deleted one and an update that would duplicate an existing link merges into it, matching the C# CLI. The transactions log records one transition per link a write actually touched — including the ones a cascade touched — so rollback and version-control branch switching no longer lose the cascaded changes, and the query processor restores links that a resolved write deleted as a side effect, mirroring `RestoreUnexpectedLinkDeletions` in the C# processor.

Fixed Rust name lookups losing links whose external reference collided with a `doublets` service constant. `LinkStorage` now reports the hybrid `LinksConstants`, which reserve the upper half of the address range for external references, and its inherent `get_or_create` no longer resolves to `Doublets::search` — which treats `any` as a wildcard — through the `Doublets` impl for `&mut LinkStorage`. Reserved pinned type names such as `Type` or `UnicodeSymbol` also resolve deterministically now that name holders are ordered by address instead of by hash map iteration order.

Added a Rust `PersistentTransformationDecorator` that ports the C# persistent transformation triggers, including the `Once`/`Always` trigger schema, the `<database>.triggers.links` sidecar store and the embedded store. Triggers written by either implementation are readable by the other.

Exposed the persistent transformation triggers on the Rust CLI: `--always`, `--once` and `--never` store or remove a trigger, `--triggers`/`--triggers-file` point at the trigger store and `--embed-triggers` keeps the triggers in the main database, matching the C# CLI.

Fixed address allocation in the Rust storage so that it matches the C# store: a freed address is reused before the store grows, the most recently freed one first, deleting the last link shrinks the store, and ensuring an address gives back the addresses passed over on the way to it. The free list is persisted so the order survives between CLI invocations.

Fixed `--changes` reporting in the Rust CLI: an auto-created reference is now reported as an update of the placeholder it started from, a delete reports the whole cascade of removed usages, and the reported changes are emitted in a reproducible order instead of one derived from the hash seed of the process.

Made every module of the `link_cli` library public, along with the query patterns, the resolved links, the link reference validator and the transition wire-format constants, so an alternative CLI or an embedding application can reuse and replace the same pieces `clink` is built from instead of reimplementing them.

Fixed unbound substitution variables and `*` in a substitution: they are now resolved at the write boundary the way C# resolves `Constants.Any`, so a created half becomes null, a lookup treats the half as a wildcard, and an update keeps the half already stored, instead of writing the literal `4294967295`.

Added `PersistentFileMapped::open_existing`, a safe constructor that adopts
the full element capacity represented by an existing file instead of starting
with a logical capacity of zero. This lets embedding applications reopen a
persisted `doublets::unit::Store` without an `unsafe` `grow_assumed` call.

Added LiNo substitution operations over TCP (issue #105): a dot-stuffed UTF-8 `TextLinoProtocol` and an interchangeable `BinaryLinoProtocol` whose packets are sections of links sharing one reference width (8, 16, 32 or 64 bits) and one arity range. Doublets with one uniform width are the default; external (Hybrid) references, any arity range such as `2..3` or `1..`, and packed per-section widths are options, and packed output is never larger than uniform output. The format is pinned by shared golden vectors in `docs/protocol/binary-links-notation-vectors.txt`. `LinksServer`/`LinksClient` and the new `clink --serve`/`--connect` options (`--protocol`, `--external-references`, `--arity`, `--packed-widths`) run create, read, update and delete queries over either protocol.

Upgraded `links-notation` to 0.22.0. In earlier versions, parse time grew exponentially with nesting depth, so one short message could stall a server.

Fixed the `--changes` report of creations:

- Every link a query creates is reported, named point links and leaves created on the way included. Each one is reported once, as `() ((name: name name))`, not as an empty link filled in later.
- Creating a link under an existing name redefines that link in place, and a new name for an existing doublet names it instead of copying it.
- Reference validation predicts the addresses new links get by asking the store. Stores reuse the address freed last first, so the lowest free address is not always next. A reference to a freed address the query does not refill is now reported missing instead of being dropped.

Added `RemoteLinks`, which implements `doublets::Links`, `doublets::Doublets` and `NamedTypeLinks` for links that live behind a `LinksServer`. Code written against those traits, the `QueryProcessor` included, switches from a local file to a server by swapping one value. `LinksServer` now also answers `LinksOperation` documents, one per interface call: `(count: …)`, `(each: …)`, `(create: …)`, `(update: …)`, `(delete: …)`, `(get-name: …)`, `(set-name: …)`, `(get-by-name: …)` and `(remove-name: …)`. These are the documents the C# `RemoteLinks` sends, and both test suites replay the same recorded conversation, `docs/protocol/links-operations.txt`. A write replies with the net change of every link it touched: the steps of a cascading delete differ between the C# and the Rust stores, the net changes do not.

Added `--export-binary <PATH>` (aliases `--binary-output`, `--binary-out`) and `--import-binary <PATH>` (aliases `--binary-input`, `--binary-in`), which write and read the whole store, names included, as a store archive in binary links notation: a links packet that keeps every address and hole, followed by a names packet. The library exposes the same as `protocol::export_store`, `protocol::import_store` and their `_file` variants. A binary archive is imported before the `--in` LiNo file and exported wherever `--out` is written. The archive bytes match the C# port.

Updated `thiserror` to 2.0.21 and refreshed `Cargo.lock`. A new `Dependencies` workflow fails while any dependency or lock file entry is behind its latest release, and Dependabot opens the update pull requests.

Fixed the WebAssembly build, which broke when `NamedTypeLinks` started requiring `update_observed` and `delete_observed`. The browser workbench now runs on the new `LinkStorage::in_memory`, the CLI's store without a file, instead of its own simplified copy. In the browser, deleting a link now also deletes its usages, and updating a link into an existing pair merges the two, exactly as in the CLI.

`protocol::packet::address_tier` now returns a `ProtocolResult` and fails as unencodable for an address beyond the internal range instead of silently returning width 8. A server that is stopping now answers a request it already read with `(error: "server is shutting down")` instead of hanging up, and its trace notes when a client hangs up. Malformed-operation errors print the offending link in LiNo instead of Rust `Debug` syntax, and the trailing-bytes error no longer depends on the byte count. Tests now cover every line of the protocol code: each malformed-packet test asserts the exact error, which fixed cases that passed for the wrong reason, and new tests cover misshapen `name`/`link` replies, archive file errors and a stopped server.

The binary links notation now comes from `links_notation::binary` (links-notation 0.23.0) instead of a copy in `link_cli::protocol` (issue #104). `link_cli::protocol` re-exports it, so `protocol::packet`, `LinksPacket`, `ArityRange`, `BinaryLinoOptions`, `DecodeLimits`, `encode_document`, `decode_document`, `format_document` and `parse_document` keep their paths. Breaking changes:

- `DecodeLimits::max_text_bytes` is gone: the text limit belongs to the transport, so `TextLinoProtocol { limits }` becomes `TextLinoProtocol { max_text_bytes }` (64 MiB by default). The new `ProtocolLimits { binary, max_text_bytes }` replaces `DecodeLimits` in `ServerOptions::limits`, `read_any_document` and `MessageFormat::protocol`.
- The upstream defaults apply: the default `max_depth` drops from 1024 to 64, the same depth the text parser accepts, and the new `max_string_bytes` limit (64 MiB) bounds the strings a packet expands to.
- `BinaryLinoProtocol` checks its limits when it encodes too, so it fails with `ProtocolError::Unencodable` instead of writing a packet its peer would reject.
- The re-exported codec (`encode_document`, `decode_document`, `parse_document`, `LinksPacket::from_bytes`, `LinksPacket::read_from` and the rest of `protocol::packet`) returns `BinaryError` instead of `ProtocolError`. Every protocol, server, client and archive function still returns `ProtocolError`, and `From<BinaryError> for ProtocolError` keeps the kind and message, so `?` keeps working.
- The upstream decoder validates more: a packet's link count and references are checked against the limits before any link is decoded.

Updated `links-notation` to 0.23.0. The `Dependencies` workflow now checks every manifest git tracks, and a dependency can be held back only by a comment on its manifest line that links an open issue.

### Changed
- Publish to crates.io through GitHub Actions trusted publishing (OIDC), falling back to the `CARGO_REGISTRY_TOKEN`/`CARGO_TOKEN` secret when the trusted publishing exchange is unavailable.

Mark the internal `clink-wasm` browser wrapper as `publish = false` so registry scans no longer treat it as an unpublished crates.io package. Its wasm-pack app build and GitHub Pages deployment remain available.

Update Rust dependencies and lockfiles to satisfy the latest-dependency CI gate, including doublets 0.8.0, links-notation 0.26.0, and lino-arguments 0.5.0. Verify the canonical lossless reference literals used by the newer Links Notation formatter while retaining coverage for legacy multiline text framing.

Add optional `IndexedLinksDecorator` lookup indexes, composed through
`LinkStorage::with_indexed_lookups`, `NamedTypesDecorator::with_indexed_lookups`
and the Rust CLI's `--indexed-lookups` flag. Named LiNo imports and exports use
persistent exact-pair/source/target indexes instead of repeated full-store
scans when enabled. Keep the original scan algorithms as the default for
comparison. Preserve Unicode names, duplicate resolution, cascading deletion,
address reuse and persisted readback. Add bounded scaling and parity tests.

Synchronize protocol and dependency documentation with the versions inherited
from main. Add compatibility tests for legacy quoted input and lossless CRLF
transport with links-notation 0.26. Binary version 1 encoding is unchanged.

Add `Clink.request(text)` to the WebAssembly wrapper: it executes a
LinksOperation or substitution document exactly like `clink --serve` and
returns the reply text, so `@link-foundation/link-cli-client` drives the
in-process store and remote servers through one contract (issue #108).

`LinkStorage::set_name` takes a name away from the link that had it before,
as the file-backed names store and the C# port do. Previously the old link
kept reporting the name, and removing that stale name unnamed the new link;
the WebAssembly store showed this through `get-name` (issue #108).

Saving a database now writes a new file next to it and moves it over the old one instead of truncating the file and rewriting it in place. Stopping `clink --serve` (Ctrl+C, a crash) during a write used to leave an empty or partial database and lose every link, the acknowledged ones included; it now leaves the last complete save. Nothing is synced to disk, so a power loss can still lose recent writes.

Update links-notation to 0.27.1 and refresh the native and WebAssembly lockfiles
to satisfy the repository's dependency freshness check.

Validate every LiNo import definition, including nested links, before changing links or names so invalid input leaves library stores unchanged.

Preserve quoted and encoded query references as names throughout validation,
creation, matching and mutation, including numeric names such as "0" and "1".
Bare numbers remain link addresses. Applies to the CLI, library and WASM wrapper.
Keep quoted names intact in stored triggers, and quote numeric names in LiNo
output so exported names round-trip through queries.

Fix the release pipeline so a Rust release always commits its version bump.
`rust/CHANGELOG.md` did not exist, so the release script's `git add` failed
silently, nothing was committed and every push to main re-published the same
changelog fragments as a new patch version (1.0.6, 1.0.7 and 1.0.8). The
script now creates `CHANGELOG.md`, consumes the fragments, syncs `Cargo.lock`,
verifies the release write, fails on any staging error, retries only lost push
races and tags the release only after its commit reached the remote.

crates.io trusted publishing now runs only when the
`CRATES_IO_TRUSTED_PUBLISHING` repository variable is `true`. Attempting it
unconditionally left a "No Trusted Publishing config found" error annotation on
every successful release that published with the token secret.

The crates.io, Docker Hub and GitHub release lookups that decide whether and
which version to release now retry network failures, 429 and 5xx with backoff
and fail with an error once their attempts run out. A failed lookup used to
count as "not published", so a single dropped connection re-released a
published version or bumped from a stale base.

Fix Auto Release compilation with warnings denied by importing the test-only
PathBuf in the test module. Compile every executable release helper in its
non-test release configuration on pull requests before it can reach main.
