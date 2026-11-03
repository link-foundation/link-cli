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

End-to-end demo scripts live in
[`examples/transactions/`](../examples/transactions) and
[`examples/version-control/`](../examples/version-control).

### Serve Over TCP

```bash
clink --db data.links --serve 127.0.0.1:7777                 # serve a database
clink --connect 127.0.0.1:7777 '() ((1 1))'                  # text protocol
clink --connect 127.0.0.1:7777 --protocol binary --arity 1.. # binary protocol, list all links
```

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
- **Durability.** Writes to a file-mapped store survive a *process* crash
  without any `save()` — the mapping is the page cache. Surviving a *machine*
  crash needs the `fsync` that `LinksStorage::flush` performs (a clean drop also
  syncs). In-memory stores keep everything in memory until `flush`/`save`.
  Recovery runs when the decorator is created: committed-but-unapplied
  transitions are replayed, uncommitted ones are rolled back.
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
