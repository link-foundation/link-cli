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
| `.github/workflows/` | Split C#, Rust, and WebAssembly CI workflows. |

## C# Implementation

The C# implementation is the production NuGet tool.

Key files:

- `csharp/Foundation.Data.Doublets.Cli/Program.cs`: command-line surface and
  command orchestration.
- `AdvancedMixedQueryProcessor.cs`: LiNo restriction/substitution processing.
- `NamedTypesDecorator.cs`: names layered over links with a sidecar names
  database.
- `PinnedTypesDecorator.cs`: pinned type support composed into named types.
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
- `rust/src/query_processor.rs`: LiNo query execution.
- `rust/src/query_processor_substitution.rs`: substitution preservation helpers.
- `rust/src/link_reference_validator.rs`: reference validation and auto-create.
- `rust/src/named_types.rs`: names sidecar storage.
- `rust/src/lino_database_input.rs`: `.lino` import.
- `rust/src/sequences/`: Unicode sequence conversion and related parity code.
- `rust/src/storage/`: the `LinksStorage` trait the transactions layer is
  written against, the doublets-backed `DoubletsStorage`, the
  `PersistentFileMapped` backing memory, and the advisory-locking helpers.
- `rust/src/transactions/`: optional transactions decorator and the
  transitions-log, retention-policy, and commit-mode types.
- `rust/src/version_control/`: optional version-control decorator with
  branching, tagging, and time-travel checkout.

Main Rust dependencies:

- `doublets = "0.4.0"` for links storage foundations.
- `links-notation = "0.16.1"` for LiNo parsing.
- `lino-arguments = "0.3.0"` for argument initialization compatibility.
- `anyhow` and `thiserror` for error handling.

The Rust CLI currently supports the core query, storage, import/export,
structure, output, named-reference, and auto-create options. Persistent
transformation trigger CLI options are implemented in C# only.

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
2. `Clink` stores links in an in-memory `BrowserStorage`.
3. Queries are passed into the Rust `QueryProcessor`.
4. The result includes formatted output plus a structured `links` snapshot.
5. React renders the snapshot and mirrors it into `doublets-web` `UnitedLinks`.

The committed lockfile currently pins `doublets-web` to `0.1.3`.

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

## CI Workflows

| Workflow | Scope |
|----------|-------|
| `.github/workflows/csharp.yml` | .NET restore, build, tests, package, and release. |
| `.github/workflows/rust.yml` | Rust formatting, clippy, file-size gate, tests, package, and release. |
| `.github/workflows/wasm.yml` | Rust core tests, wasm-pack tests, Vite build, artifact upload, and manual Pages deployment. |

Path filters keep most workflows focused on the parts of the repository they
own.

## External References

- NuGet `clink`: https://www.nuget.org/packages/clink
- crates.io `link-cli`: https://crates.io/crates/link-cli
- Rust `doublets`: https://docs.rs/doublets/latest/doublets/
- Rust `links-notation`: https://docs.rs/crate/links-notation/0.13.0
- npm `doublets-web`: https://www.npmjs.com/package/doublets-web
- WebAssembly local docs and implementation notes: [../js/README.md](../js/README.md)
