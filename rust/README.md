# link-cli Rust Package

[![Rust CI/CD Pipeline](https://github.com/link-foundation/link-cli/actions/workflows/rust.yml/badge.svg)](https://github.com/link-foundation/link-cli/actions/workflows/rust.yml)
[![Crates.io](https://img.shields.io/crates/v/link-cli?logo=rust&label=Crates.io)](https://crates.io/crates/link-cli)
[![Docs.rs](https://docs.rs/link-cli/badge.svg)](https://docs.rs/link-cli)
[![GitHub Release](https://img.shields.io/github/v/release/link-foundation/link-cli?filter=rust-v*&label=Rust%20release)](https://github.com/link-foundation/link-cli/releases)

This directory contains the Rust `link-cli` crate, which publishes both a
reusable `[lib]` (`link_cli`) and the `clink` `[[bin]]` from the same
package. It mirrors the production C# tool: the query processor, named
references, LiNo import/export, structure formatting, persistent
transformation triggers, transactions and version control.
The WebAssembly wrapper crate lives in `rust/wasm/` and depends on this
package.

Quoted query references always resolve by name: `"0"`, `'1'`, and reference
literals such as `~1{31}` are names, while bare `1` is an address. This applies
to the CLI, library, and WebAssembly query engine, including definitions,
restrictions, updates, and deletions.

## Install

```bash
# Build and install the CLI binary.
cargo install link-cli
```

```bash
# Or pull in the public API to build your own tooling.
cargo add link-cli
```

API documentation for every published version is hosted on
[docs.rs/link-cli](https://docs.rs/link-cli). A copy is also published to
GitHub Pages alongside the C# DocFX site by `.github/workflows/docs.yml`.

## Use

```bash
clink '() ((1 1))' --changes --after
```

### Optional lookup indexes for named imports

LiNo input is fully parsed and validated before any links or names change,
including nested definitions. Invalid input leaves the store unchanged through
both the CLI aliases and `import_lino_text`/`import_lino_file` library APIs.

For thousands of named links, enable the Rust lookup-index decorator:

```bash
clink --db data.links --indexed-lookups --import pages.lino --export pages-copy.lino
```

In the library, call `NamedTypesDecorator::new(path, false)?.with_indexed_lookups()`
(or `LinkStorage::with_indexed_lookups()` for a plain store). This composes
`IndexedLinksDecorator` under both the data store and names sidecar. Exact
pair, source and target indexes live for the store's lifetime and follow raw
writes, uniqueness merges, cascading deletes and reloads. They add O(n)
memory and are rebuilt when a fresh process opens the persisted graph.
`--trace` reports when they are enabled. The flag works with transactions,
version control and a local TCP server; remote clients enable it on the server.

The default remains the original scan algorithm; omit the flag or pass
`--indexed-lookups=false` to compare it. On the issue-110 Linux release fixture,
10,000 69-character named self-links imported in about 1.3 seconds with indexes;
1,000 took about 11.2 seconds without them. These are measurements, not a
portable latency guarantee. Numeric imports also benefit. See the
[bounded benchmark](../examples/named-import/README.md) and
[investigation](../docs/case-studies/issue-110/README.md).

Current limits: the baseline named algorithm still scales poorly; the Unicode
facade and its conversion caches are still reconstructed per name operation.
The index removes the growing scans but does not eliminate that fixed cost.
Unconstrained queries still enumerate the full graph, save rewrites the whole
file, and import parses the input into memory. C# and the browser do not expose
this CLI flag; library consumers opt in explicitly. Numeric-only names remain
ambiguous in Rust text export; [issue #112](https://github.com/link-foundation/link-cli/issues/112)
documents the separate C# fix.

### Optional Transactions and Version Control

Pass `--transactions` (or any flag in the family — `--transactions-file`,
`--commit-mode`, `--retention`, `--log`) to record each Create/Update/Delete
as a reversible transition in a sidecar links store. Pass `--vc`
(or `--vc-file`, `--branch`, `--branch-from`, `--checkout`, `--tag`,
`--list-branches`, `--list-tags`) to add a version-control layer over the
recorded transitions log:

```bash
# Record reversible transitions into data.transitions.links
clink --db data.links --transactions --auto-create-missing-references '() ((1 1))'
clink --db data.links --log

# Branch and tag on top of the transitions log
clink --db data.links --vc --tag v1
clink --db data.links --vc --branch feature --branch-from 1
clink --db data.links --vc --list-branches
```

Tags accept `name=sequence` or `name=existing-tag`. For names containing `=`,
the command uses the first delimiter whose complete suffix resolves. For example,
after creating `v1`, `--tag 'release=雪=part=v1'` creates `release=雪=part`,
and `--tag 'alias=release=雪=part'` creates an alias at the same point.
Quote these arguments in the shell. The
[tag-name example](../examples/version-control/tag-names.sh) checks both forms
and checkout through repeated native command invocations.

End-to-end demo scripts live in
[`examples/transactions/`](../examples/transactions) and
[`examples/version-control/`](../examples/version-control).

### Serve Over TCP and Other IPC Transports

```bash
clink --db data.links --serve 127.0.0.1:7777                 # serve a database
clink --connect 127.0.0.1:7777 '() ((1 1))'                  # text protocol
clink --connect 127.0.0.1:7777 --protocol binary --arity 1.. # binary protocol, list all links
```

With `--triplets`, the server reads every request as triplets instead, as
described in [docs/TRIPLETS.md](../docs/TRIPLETS.md#protocol).

Every address is an endpoint of
[IPC transports v1](../docs/protocol/ipc-transports-v1.md), so the same server
also listens on UDP, Unix sockets, named and anonymous pipes, FIFOs, shared
memory, mapped files, files, signals, POSIX and System V message queues,
mailslots, `WM_COPYDATA` or ALPC. `--serve` repeats, and `--forward` stacks a
relay on top without opening a database:

```bash
clink --db data.links --serve 127.0.0.1:7777 --serve shm:/tmp/links   # two endpoints, one store
clink --serve udp:127.0.0.1:7778 --forward shm:/tmp/links              # a relay
clink --connect udp:127.0.0.1:7778 --protocol binary '() ((1 1))'
clink --connect 'exec:clink --db other.links --serve stdio' ''         # anonymous pipes
```

In the library, `link_cli::protocol::ipc` connects, listens, relays and registers custom kinds,
and `LinksServer::listen_all` and `LinksClient::connect_endpoint` take any endpoint. See [`examples/ipc/`](../examples/ipc/README.md).

In the library, `link_cli::protocol::LinksServer` serves any `NamedTypeLinks` store, and
`LinksClient` queries a server through either `TextLinoProtocol` or
`BinaryLinoProtocol`. The wire formats are described in
[docs/case-studies/issue-105](../docs/case-studies/issue-105/README.md).
The binary links notation codec itself (`LinksPacket`, `encode_document`,
`decode_document`, `ArityRange`, `DecodeLimits`) comes from
`links_notation::binary`, which `link_cli::protocol` re-exports;
`ProtocolLimits` bounds messages of both protocols.

## Use as a library

`link_cli` is usable as an embedded, doublets-backed transactional store, not
only as the code behind `clink`.

```rust
use link_cli::transactions::{
    CommitMode, FileTransitionLog, GenericTransactionsDecorator, LogRetentionPolicy,
};
use link_cli::DoubletsStorage;

fn main() -> Result<(), link_cli::LinkError> {
    // A file-mapped `doublets::unit::Store<usize, _>`, locked for writing.
    let store = DoubletsStorage::<usize, _>::open_exclusive("db.links")?;
    let log = FileTransitionLog::open("db.transitions.log")?;
    let mut tx = GenericTransactionsDecorator::new(
        store,
        log,
        LogRetentionPolicy::default(),
        CommitMode::default(),
        false,
    )?;

    tx.begin_transaction()?;
    let point = tx.create(0, 0)?;
    tx.commit()?;
    println!("created {point}");
    Ok(())
}
```

A runnable, self-checking version of this is
[`examples/embedded_store.rs`](examples/embedded_store.rs)
(`cargo run --example embedded_store`).

- **Any address type.** `GenericTransactionsDecorator<T, S, L>` is generic over
  the doublets address type, the wrapped store, and the transitions log.
  `TransactionsDecorator` is the `u32` + `NamedTypesDecorator` specialisation
  `clink` uses. The transitions wire format writes addresses in decimal, so it
  is identical across address types, and an address that does not fit the
  target type is reported as `LinkError::AddressOutOfRange` rather than
  truncated.
- **Any store.** `storage::LinksStorage<T>` is the trait the transactions layer
  is written against. `storage::DoubletsStorage::open` creates a file-mapped
  `doublets::unit::Store`; `DoubletsStorage::wrap` adopts a store you already
  own.
- **In-place mutation.** A file-mapped store is written through its mapping, so
  the inode never changes and other processes that mapped the same file keep
  observing the same data. Nothing is replaced through a temporary file.
- **Durability.** `LinksStorage::flush` synchronizes an owned mapped store's
  file. On Windows it first flushes the mapped view, then the file buffers.
  Explicit flush propagates both errors; Drop attempts the same ordering.
  Adopted mappings attach their matching `PersistentFileMapped::flush_handle()`
  with `DoubletsStorage::with_mapped_flush`; a path alone cannot discover a
  foreign store's view. Rejected plain-data growth retains the old view and
  flush owner until a replacement is fully prepared; fresh padding and failed
  extension tails receive the requested initializer. This temporarily holds
  two views and a cloned file handle. Failed shrink and arbitrary destructor
  growth retain their upstream limitations. These barriers do not supply
  torn-page repair or
  certify hardware power-loss safety. In-memory stores keep everything in
  memory until `flush`/`save`.
  Recovery runs when the decorator is created: committed-but-unapplied
  transitions are replayed, uncommitted ones are rolled back.
  A known durable commit retains ownership after a checkpoint error: retry
  `commit`, `rollback` or `save` before writing again. Public `recover` rejects
  an open transaction. In-memory transactions also run in browser/Node WASM;
  timestamps use its wall clock and do not provide distributed ordering.
- **Multi-process access.** `storage::lock` locks a `<database>.lock` sidecar,
  shared for readers and exclusive for writers, through
  `DoubletsStorage::open_shared` / `open_exclusive` / `try_open_exclusive`.
  `LinksStorage::has_external_changes` answers "has anyone else written since I
  last looked?" from a `StorageRevision` fingerprint. This requires Rust 1.89,
  the release that stabilised `std::fs::File::lock`.
- **Typed errors and paths.** Public entry points take `AsRef<Path>` and return
  `LinkError`, not `anyhow::Error`.

### Type hierarchy

`TypesDecorator` wraps any `NamedTypeLinks` and keeps a type hierarchy with
multiple inheritance in plain links (#148): the type of a link is its source,
the root is `(Type: Type Type)`, a sub-type is `(S: T S)`, and an extra parent
is a pair `(p: S T2)` marked by `(m: Extends p)`. Its queries stop on cycles.

```rust
use link_cli::{LinkStorage, Types, TypesDecorator};

let mut types = TypesDecorator::with_named_markers(LinkStorage::in_memory(false))?;
let root = types.root();
let doc = types.define(root, &[])?;
let public = types.define(root, &[])?;
let note = types.define(doc, &[])?;
let news = types.define(note, &[doc, public])?;
let today = types.instance(news)?;
assert!(types.is_a(today, public));
assert_eq!(vec![note, doc, public], types.parents_of(news));
```

The `Types` trait needs only `root()` and `extends()`; everything else has a
default that reads and writes through the implementing layer, so a layer that
wraps `TypesDecorator` can reuse or override any of it. Run
`cargo run --example type_hierarchy` for a complete walk-through.

### Triplets

`TripletsOverDoublets` wraps any `NamedTypeLinks` and keeps triplets
`(index: type from to)` in its doublets (#151): a node `(i: T 0 0)` is
`(i: T i)` and a link `(i: T f t)` is `(i: T ((Pair f) t))`, with the points
`Pair` and `Nothing` found or created by name.

```rust
use link_cli::{process_triplets_query, triplets_to_lino, LinkStorage, TripletQuery, Triplets, TripletsOverDoublets};

let mut triplets = TripletsOverDoublets::with_named_markers(LinkStorage::in_memory(false).with_indexed_lookups())?;
let root = triplets.create_triplet_point()?;          // (root: root 0 0)
let person = triplets.create_triplet(root, 0, 0)?;
let alice = triplets.create_triplet(person, 0, 0)?;
let bob = triplets.create_triplet(person, 0, 0)?;
let knows = triplets.create_triplet(root, alice, bob)?;
triplets.update_triplet(knows, root, bob, alice)?;   // the address stays
assert_eq!(1, triplets.count_triplets(TripletQuery::by_from(bob)));
assert_eq!(2, triplets.delete_triplet(alice)?.len()); // alice and knows
process_triplets_query(&mut triplets, &format!("(() (({root} {bob} {bob})))"))?; // a create
print!("{}", triplets_to_lino(&mut triplets)?);
```

The `Triplets` trait needs only `triplet_markers()`; every read and write has
a default over `NamedTypeLinks`. Run `cargo run --example triplets` and see
[docs/TRIPLETS.md](../docs/TRIPLETS.md) for the encoding and its costs.

### Permissions

`--permissions self|<rules.links>` runs queries as `--subject` (an index or a
name; a guest by default) and checks every read and write against rules stored
as links (#149); see [Permissions](../README.md#permissions) and
[`examples/permissions`](../examples/permissions). In the library:

```rust
use link_cli::{NamedTypesDecorator, PermissionAccess, PermissionMarkers, PermissionPolicy, PermissionsDecorator};

let mut data = NamedTypesDecorator::new("data.links", false)?;
let markers = PermissionMarkers::ensure(&mut data)?;
PermissionPolicy::allow(&mut data, markers, PermissionAccess::Read, user, public)?;
let policy = PermissionPolicy::in_data(&mut data)?; // or with_rules(&mut data, Box::new(rules))
let mut alice = PermissionsDecorator::new(&mut data, policy, alice_link);
// Reads skip what alice may not read; a forbidden write fails with
// Forbidden ("Forbidden: update 14") and changes nothing.
```

A link alice may not refer to answers as a missing one and its name as an unknown
one (#161). `PermissionAccess::Reference` grants the reference without the read;
see [the case study](../docs/case-studies/issue-161/README.md).

`UndoableLinksDecorator` (`begin`, `commit`, `rollback`) makes a request atomic
under the view, writes the store made on its own included.
`LinksSessions::with_permissions(store, policy)`, `with_subject` and
`with_authenticator` give every connection of `LinksServer::serve_sessions` a
session of its own; `(auth: token)` answers `(subject: i)`. The authenticator
is any callback; `LinksTokenTable` keeps the tokens as links named after their
SHA-256.

### Composing networks

`UnionLinks` joins links networks into one, `RemapLinks` reads one under other
addresses (`OffsetAddressMap`, `AddressTable` or any `AddressMap`), and
`LinksExternalIds` keeps the ids of other systems (UUIDs, URLs) as links
(#149). None of them depends on permissions or on each other; a
`PermissionsDecorator` over a union restricts every member, one under it a
single member. `NamedLinksStorage` puts the transactions layer over any of
them. See [docs/COMPOSITION.md](../docs/COMPOSITION.md).

```rust
use link_cli::{LinksExternalIds, OffsetAddressMap, PermissionPolicy, PermissionsDecorator, RemapLinks, UnionLinks};

let mut union = UnionLinks::new(vec![
    Box::new(&mut client),
    Box::new(RemapLinks::new(&mut server, OffsetAddressMap::new(1 << 24))),
])?;
let draft = union.try_create(0, 0)?;        // in client, the primary member
union.update(draft, server_note, draft)?;   // may refer to the server's links
assert_eq!(Some(0), union.owner_of(draft));
let policy = PermissionPolicy::in_data(&mut union)?;
let mut alice = PermissionsDecorator::new(&mut union, policy, alice_link);

let ids = LinksExternalIds::new("uuid")?;
let link = ids.ensure(&mut ids_store, Some(&mut data), "6f1c...")?; // bound, or a new point
```

## Develop

```bash
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features
cargo test --manifest-path rust/Cargo.toml --all-features
```

Release automation for this package lives in `rust/scripts/` and uses changelog
fragments from `rust/changelog.d/`.

The optional [`engine`](src/engine/mod.rs)
module provides private transaction sessions, configurable concurrency/isolation,
immutable history, constraints and replaceable revision logs. See
[engine strategies](../docs/ENGINE-STRATEGIES.md) for configuration, costs,
distributed ownership, guarantees and examples. The existing bare store remains
available for cache workloads without these layers.

Select `ConcurrencyControl::Pessimistic` for overlapping sessions that reserve
written addresses and immediately reject competing writes. Isolation and history
remain independent options. See [ownership, retry rules and costs](../docs/ENGINE-PESSIMISTIC.md).

Any store can be kept as a [cache with deadlines and a size limit](../docs/ENGINE-CACHE.md).
Any session can undo part of its work with [savepoints](../docs/ENGINE-SAVEPOINTS.md).
A running engine can be copied to an empty log with [online backup](../docs/ENGINE-BACKUP.md).
A damaged journal can be [checked and repaired](../docs/ENGINE-RECOVERY.md) from its valid prefix or a replica.

Legacy version sidecars preserve delimiter text, Unicode, NUL and newline in
branch, parent and tag names. New branch markers use a versioned encoding and
ordinary legacy markers remain readable; see the [marker protocol](../docs/protocol/version-metadata.md)
for compatibility and upgrade limits.

The portable `engine::paged::PagedLog<D>` stores ordered records as immutable
native doublets pages without SQL. Feature `native-pages` adds the exclusive
`FilePageDevice` adapter. Select a page quota with `PagedLog::with_max_pages`;
explicit `log.into_device().close()?` reports uncertain close outcomes without
flushing pending records. See [native page guarantees, costs and examples](../docs/ENGINE-NATIVE-PAGES.md).
