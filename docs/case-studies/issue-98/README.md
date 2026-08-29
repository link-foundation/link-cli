# Issue 98 Case Study: Reusable Doublets-Backed Transactional Store

Issue: <https://github.com/link-foundation/link-cli/issues/98>

Prepared PR: [#99](https://github.com/link-foundation/link-cli/pull/99)

> Scope of this case study: this folder captures the evidence, the restated
> requirements, the root-cause analysis, the implemented design, and the
> verification evidence for making the `link-cli` libraries reusable as an
> embedded, doublets-backed transactional store, plus the cross-language
> dependency refresh that shipped with it.

## 1. Issue summary

`link-assistant/router` keeps its tokens in a memory-mapped `doublets` store
and is building a persistent-store-plus-transaction-log design
([router#357](https://github.com/link-assistant/router/issues/357)). It would
rather depend on the transactions layer `link-cli` already publishes than
maintain a second implementation of transitions, commit markers and recovery.
The issue enumerates the six things that blocked that reuse:

| # | Blocker | Ask |
|---|---------|-----|
| 1 | The Rust storage layer is not doublets-backed — `LinkStorage` is `HashMap`s serialised to a text file, despite docs describing a "sidecar **doublets** log store". | Back the storage with `doublets`, **or** expose the decorator generically over a storage trait so an external doublets store can be supplied. |
| 2 | `u32` link ids. The router uses `unit::Store<usize, _>`; a `u32` API cannot address a `usize` store. | Make the storage and transactions API generic over the doublets address type, or at minimum offer `usize`/`u64`. |
| 3 | No in-place / crash-safety guarantees exposed. Writing a temp file and renaming over it replaces the inode, silently staling every other process's mapping. | Document and test crash recovery against a *file-backed* store, including a torn write mid-rebuild, and state whether `save()` is required for durability or whether mapping writeback is relied on. |
| 4 | Multi-process access. Nothing takes an advisory lock or notices the file changed underneath it. | Advisory locking (shared for reads, exclusive for writes) plus a cheap "has anyone else written?" check. |
| 5 | Not exported for external use. `LinkStorage::new` takes `&str`; errors are `anyhow::Error`. | `AsRef<Path>` for paths and typed `LinkError` on the public API. |
| 6 | Dependencies are behind in every language. | Refresh Rust, C# and JS together, keeping shared libraries (notably `links-notation`) on the same version across languages. |

A follow-up comment from the issue author narrowed nothing and widened the
scope explicitly:

> We must do it for all programming languages we support, and update
> dependencies in all of them.

That is why every ask below is answered twice — once in Rust, once in C# —
even though only the Rust library was named in the issue title.

## 2. Restated requirements

| ID  | Requirement |
|-----|-------------|
| R1  | A real doublets-backed storage exists: a `doublets::unit::Store` over file-mapped memory, reachable from the public API. |
| R2  | The transactions layer is written against a storage *abstraction*, so an external, caller-owned doublets store can be supplied instead. |
| R3  | The transactions layer is generic over the doublets address type; `usize` and `u64` in particular are first-class. |
| R4  | The transitions wire format does not depend on the address type, and an address that does not fit the target type is reported rather than silently truncated. |
| R5  | A file-backed store is mutated **in place**: the inode is stable, so another process's existing mapping never goes stale. |
| R6  | Durability is documented per backend, stating explicitly whether an explicit save is required. |
| R7  | Crash recovery is *tested* against a file-backed store: committed writes survive, uncommitted writes are rolled back, committed-but-unapplied transitions are re-applied, and a torn final log entry is tolerated. |
| R8  | Advisory locking of a database is available — shared for readers, exclusive for writers, blocking and non-blocking — and is honoured across processes, not merely within one. |
| R9  | A cheap "has anyone else written since I last looked?" check exists that does not reparse the database. |
| R10 | Public entry points accept `AsRef<Path>` (Rust) and return typed errors rather than `anyhow::Error`. |
| R11 | Every dependency in every language is refreshed, with shared libraries pinned to the same version across languages. |
| R12 | The C# port gets the same reusability treatment as Rust (address genericity, tested crash recovery, locking helpers). |
| R13 | The CLI's observable behaviour is unchanged: no new files, no new cost, no changed output for existing invocations. |

## 3. Evidence captured in this folder

```
docs/case-studies/issue-98/
├── README.md                     # This document.
└── github-data/
    ├── issue-98.json             # Raw issue payload at investigation time.
    ├── issue-98-comments.json    # Issue comments at investigation time.
    └── pr-99.json                # PR snapshot.
```

## 4. Root-cause analysis

### 4.1 The documentation was right about the design and wrong about the code

`grep doublets rust/src/named_types.rs rust/src/transactions/mod.rs` returned
only a doc comment. The Rust `LinkStorage` was a pair of `HashMap`s written
out as text; the "sidecar doublets store" the README described existed only
in C#, where the sidecar really is a `UnitedMemoryLinks` doublets store. The
transactions layer itself was not the problem — its shape (before/after
transitions, commit and rollback markers, replay on open) is exactly what a
consumer wants. What was missing was a seam between that layer and the store
underneath it.

### 4.2 A latent upstream data-loss bug blocked the obvious fix

Simply constructing a `doublets::unit::Store` over `platform-mem`'s
`FileMapped` does not work: reopening an existing database zeroes it.

`RawMem::grow_filled`'s default implementation fills the **entire** newly
mapped region with `Default::default()`:

```rust
fn grow_filled(&mut self, cap: usize, value: Self::Item) -> Result<&mut [Self::Item]> {
    unsafe { self.grow(cap, |_, (_, uninit)| { uninit::fill(uninit, value); }) }
}
```

`FileMapped` correctly computes how many elements were already initialised on
disk and passes that as the `inited` argument — and the default `grow_filled`
ignores it. Every link in an existing file is overwritten with zeros on open.
`experiments/doublets_persistence.rs` reproduces this against upstream
`doublets` directly, with no `link-cli` code involved.

`storage::PersistentFileMapped` works around it by forwarding to
`RawMem::grow_filled_exact`, which fills only `uninit[inited..]`. This is a
workaround in `link-cli`, not a fix upstream; it is called out in
[§8](#8-risks-and-follow-ups).

### 4.3 In-place mutation is a property of the *write path*, not of doublets

The issue's inode demonstration is the crux of ask #3: a store that is
rebuilt through a temporary file and `rename` is safe against a torn write but
poisonous to a long-lived reader in another process. A memory-mapped store is
the opposite — always in place, therefore always visible to other mappings,
therefore dependent on the transitions log for crash consistency. Both
properties had to be proven by test rather than asserted in prose, which is
what `doublets_storage_mutates_the_database_file_in_place` and the crash
recovery suite do.

## 5. Implemented solution

### Rust: a storage seam

- **`storage::LinksStorage<T>`** is the trait the transactions layer is now
  written against (R2). It is generic over the doublets address type (R3).
  The CLI's `LinkStorage`, `PinnedTypesDecorator` and `NamedTypesDecorator`
  implement it through one macro in `storage/decorator_impls.rs`, so the CLI
  path is unchanged (R13). `LinksStorageRef<T>` is the extension for stores
  that keep links resident and can lend references; memory-mapped stores
  deliberately do not implement it.
- **`storage::DoubletsStorage`** implements that trait over a real
  `doublets::unit::Store` (R1). `open`/`open_shared`/`open_exclusive`/
  `try_open_exclusive` create a file-mapped store; `wrap` adopts a store the
  caller already owns — which is what lets the router keep ownership of its
  own `unit::Store<usize, _>` (R2).
- **`storage::PersistentFileMapped`** is the backing memory that survives
  reopen (see §4.2).
- **`storage::lock`** provides `FileLock`/`LockMode`/`lock_file_path` over a
  `<database>.lock` sidecar, using `std::fs::File::lock` (R8). This is why the
  crate now declares `rust-version = "1.89"`.
- **`StorageRevision`** fingerprints a database by length and modification
  time, and `LinksStorage::has_external_changes` answers the "has anyone else
  written?" question from it (R9).

### Rust: a generic transactions layer

- **`GenericTransactionsDecorator<T, S, L>`** is generic over the address
  type `T`, the wrapped store `S`, and the transitions log `L` (R3).
  `TransactionsDecorator` remains the `u32` + `NamedTypesDecorator`
  specialisation `clink` uses (R13).
- **`TransitionLogStore`** is the log abstraction: append one entry, read them
  back in order, flush. `NamedTypesDecorator` implements it (the existing
  links-backed sidecar), and **`FileTransitionLog`** adds a plain append-only
  text log that `fsync`s every append and discards a tail torn by a crash when
  it is reopened (R7).
- **The wire format** writes addresses in decimal, so it is byte-identical
  across address types; `parse` returns `LinkError::AddressOutOfRange` — not
  `InvalidFormat` — for a structurally valid entry whose addresses do not fit
  `T`, so recovery can skip a torn entry while refusing to silently drop a log
  written by a wider store (R4).
- **Paths and errors**: public entry points take `AsRef<Path>` and return
  `LinkError` (R10).

### C#: the same three properties

- **`TransactionsDecorator<TLinkAddress>`** is generic over
  `IUnsignedNumber<TLinkAddress>`; the non-generic `TransactionsDecorator`
  survives as the `uint` specialisation, so `Program.cs`,
  `VersionControlDecorator` and every existing test construct the same type
  name as before (R3, R12, R13).
- **`LinksFileLock`** and **`StorageRevision`** mirror the Rust `storage::lock`
  API — same `.lock` sidecar path, same shared/exclusive semantics — so a C#
  and a Rust process can guard the same database (R8, R9, R12).
- **Crash recovery** is covered by a file-backed test that abandons an open
  transaction to simulate a crash and reopens twice to prove recovery is
  idempotent (R7, R12).

### Dependencies (R11)

| Language | Refreshed |
|----------|-----------|
| Rust | `doublets` 0.3.0 → 0.4.0, `links-notation` 0.13.0 → 0.16.1, `thiserror` → 2.0.20, `anyhow` → 1.0.104; WASM workspace: `wasm-bindgen` 0.2.127, `serde` 1.0.229, `serde_json` 1.0.151, `web-sys` 0.3.104, `wasm-bindgen-test` 0.3.77. |
| C# | `Link.Foundation.Links.Notation` 0.13.0 → 0.16.1, `System.CommandLine` 2.0.7 → 2.0.11, `Microsoft.NET.Test.Sdk` → 18.9.0, `xunit.runner.visualstudio` → 4.0.0, `coverlet.collector` → 10.0.1. |
| JS | `doublets-web` → ^0.1.3, `react`/`react-dom` → ^19.2.8, `lucide-react` → ^1.37.0, `vite` → ^8.2.2, `@vitejs/plugin-react` → ^6.1.1. |

The four C# packages the issue named that do **not** appear above —
`Platform.Data` 0.16.1, `Platform.Data.Doublets` 0.18.1,
`Platform.Data.Doublets.Sequences` 0.6.5 and `xunit` 2.9.3 — were already
pinned to the newest version published on NuGet, so there was nothing to bump.

The `doublets` bump is the one that unblocks reuse: an external crate already
on `doublets` 0.4 can now link against `link-cli` without pulling two
semver-incompatible copies of the same store into one binary.
`links-notation` is deliberately 0.16.1 in *both* Rust and C#, per the issue's
alignment request. That release ships only a `net10.0` assembly, so the C#
projects retargeted `net8.0` → `net10.0` — a breaking change for consumers,
recorded as a `major` changeset.

## 6. Verification

| Check | Result |
|-------|--------|
| `cargo fmt --all -- --check` (both workspaces) | clean |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `cargo test --all-features` | 188 passed, 1 ignored |
| `dotnet format --verify-no-changes` | clean |
| `dotnet build --configuration Release` | 0 warnings, 0 errors |
| `dotnet test` | 234 passed |
| `node --test` (JS, C# release scripts, workflow policy) | 82 passed |

Tests written specifically for this issue:

| Ask | Rust | C# |
|-----|------|----|
| #1 doublets-backed | `doublets_storage_creates_updates_and_deletes_links`, `doublets_storage_queries_by_pattern`, `doublets_storage_survives_reopen`, `persistent_file_mapped_preserves_existing_contents` | — (the C# sidecar was already a doublets store) |
| #1 storage seam | `doublets_storage_wraps_an_externally_owned_store` | — |
| #2 address genericity | `doublets_storage_supports_usize_addresses`, `doublets_storage_supports_u64_addresses` | `TransactionsWorkOverAUlongAddressedStore` |
| #2 wire format | `a_log_written_by_a_wider_address_type_is_rejected_not_dropped` | `TheTransitionWireFormatDoesNotDependOnTheAddressType`, `AnAddressThatDoesNotFitTheStoreIsRejectedNotSilentlyTruncated` |
| #3 in-place | `doublets_storage_mutates_the_database_file_in_place`, `the_data_store_is_mutated_in_place_across_transactions` | `TheDataStoreIsMutatedInPlaceAcrossTransactions` |
| #3 crash recovery | `committed_writes_survive_a_crash_without_save`, `uncommitted_writes_are_rolled_back_after_a_crash`, `committed_but_unapplied_transitions_are_reapplied`, `a_torn_final_log_entry_is_ignored_during_recovery` | `CommittedWritesSurviveAReopenAndUncommittedOnesAreRolledBack` |
| #4 locking | `exclusive_lock_excludes_other_holders`, `shared_locks_allow_concurrent_readers_but_block_writers`, `opened_storage_holds_its_lock_for_its_lifetime`, `exclusive_lock_is_honoured_across_processes` | `AnExclusiveLockExcludesEveryOtherHolder`, `SharedLocksCoexistButStillExcludeWriters`, `ReleasingALockLetsTheNextHolderIn`, `AcquireGivesUpAfterItsTimeout` |
| #4 external changes | `doublets_storage_detects_external_writes` | `ARevisionDetectsAWriteByAnotherHolder`, `AMissingDatabaseHasTheDefaultRevision` |

The Rust suite asserts the locking semantics across *real processes*
(`exclusive_lock_is_honoured_across_processes` re-executes the test binary as
a lock probe), because an in-process assertion would not distinguish
`flock` from the runtime's own bookkeeping. The C# equivalent was verified the
same way with a two-process harness kept in `experiments/csharp-file-lock/`,
which confirmed on Linux/.NET 10 that exclusive-vs-exclusive and
exclusive-vs-shared block while shared-vs-shared succeeds; the committed xunit
tests then cover the same matrix in-process on all three CI operating systems.

## 7. What the issue asked for versus what shipped

| Ask | Status |
|-----|--------|
| 1. Doublets-backed storage | Done, **both** ways the issue offered: a real file-mapped `doublets::unit::Store` (`DoubletsStorage`) *and* a storage trait (`LinksStorage<T>`) that an external store can be supplied through (`DoubletsStorage::wrap`). The CLI keeps its text format, so no database migration is forced. |
| 2. Generic address type | Done in both languages. `usize`, `u64` and `u32` are covered by tests in Rust; `uint` and `ulong` in C#. |
| 3. In-place mutation and crash safety | Done and tested, including a torn-write case and the inode-stability property. Durability is stated per backend in `docs/ARCHITECTURE.md`, `docs/HOW-IT-WORKS.md` and the module docs. |
| 4. Multi-process access | Done in both languages: advisory locking of a `.lock` sidecar plus a `StorageRevision` fingerprint. |
| 5. Exported for external use | Done: `AsRef<Path>` and `LinkError` across the storage and transactions APIs. |
| 6. Dependency refresh | Done in Rust, C# and JS, with `links-notation` aligned at 0.16.1 across Rust and C#. |

## 8. Risks and follow-ups

- **Upstream `grow_filled` bug.** `PersistentFileMapped` is a local
  workaround for a data-loss bug in `platform-mem`'s default
  `RawMem::grow_filled`. It should be reported upstream; until it is fixed,
  any consumer that constructs a `FileMapped`-backed `doublets` store *without*
  this wrapper will lose data on reopen. The reproduction lives in
  `experiments/doublets_persistence.rs`.
- **MSRV 1.89.** Advisory locking uses `std::fs::File::lock`, stabilised in
  1.89. Consumers on an older toolchain cannot build the crate.
- **`net10.0` for C#.** Forced by `Link.Foundation.Links.Notation` 0.16.1,
  which ships no `net8.0` assembly. Recorded as a `major` changeset.
- **Source-breaking C# generalisation.** `Transition`, `ITransaction` and
  `ITransactionsLinks` are now generic; `uint` consumers substitute
  `Transition<uint>` and friends. `TransactionsDecorator` itself is
  unchanged for `uint` callers.
- **Advisory, not mandatory, locking.** On both platforms the locks bind only
  processes that ask for them. A writer that ignores the protocol can still
  corrupt a store, which is inherent to `flock`-style locking and is
  documented rather than worked around.
