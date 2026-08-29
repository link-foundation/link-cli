---
bump: minor
---

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
