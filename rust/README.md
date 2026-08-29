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

## Develop

```bash
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features
cargo test --manifest-path rust/Cargo.toml --all-features
```

Release automation for this package lives in `rust/scripts/` and uses changelog
fragments from `rust/changelog.d/`.
