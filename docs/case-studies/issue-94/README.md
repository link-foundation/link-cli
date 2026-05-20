# Issue 94 Case Study: Optional Transactions and Version Control Layers

Issue: <https://github.com/link-foundation/link-cli/issues/94>

Prepared PR: [#95](https://github.com/link-foundation/link-cli/pull/95)

> Scope of this case study: this folder captures evidence, restated
> requirements, prior-art analysis, the implemented design, and verification
> evidence for the optional transactions decorator and optional version-control
> decorator shipped by PR #95 in both the C# and Rust implementations of
> `link-cli` (CLI + library). The issue first asked for deep case-study data
> under `./docs/case-studies/issue-{id}`; this document now records both that
> analysis and the implementation that followed it.

## 1. Issue summary

The issue requests two new, *optional*, *composable* decorator layers that
sit on top of the existing links storage (the same storage that the named
links decorator, pinned types decorator, and persistent transformation
decorator already wrap):

1. **Transactions layer** — record each write as a reversible *transition*,
   support `commit`/`rollback`, infinite or chunked or size-limited log
   files, both *sync* and *async* commit modes, and persist the log as a
   doublets store so that the log itself is a links database (decorator on
   top of *at least* two underlying stores: one for data, one for the
   log/transitions).
2. **Version-control layer** — sit on top of the transactions layer to
   provide *time travel* to any point covered by the log, plus *branching*
   from a point in time of an existing branch.

The work must be delivered in both **C# and Rust**, both as **CLI flags**
and as **public library APIs**, and must compose with the existing
named-types / pinned-types / persistent-transformation stack the same way
the existing decorators compose.

The issue specifically points to
[`linksplatform/Data.Doublets/.../UInt64LinksTransactionsLayer.cs`](https://github.com/linksplatform/Data.Doublets/blob/main/csharp/Platform.Data.Doublets/UInt64LinksTransactionsLayer.cs)
as a starting reference, while noting that it is *not finished* and we
"should do much better."

## 2. Restated requirements

Broken down into discrete, individually-testable requirements so PR #95 can
check each one off:

### Transactions (R1–R10)

| ID  | Requirement |
|-----|-------------|
| R1  | Each link operation (Create, Update, Delete) is recorded as a *reversible transition* with enough information to recompute both the *before* and *after* state. |
| R2  | A transaction API opens an explicit write batch and supports `Commit()` / `Rollback()` semantics; C# exposes this as a disposable transaction handle, while Rust exposes `begin_transaction()`, `commit()`, and `rollback()` on the decorator. |
| R3  | A rolled-back transaction reverts every recorded transition in reverse order (delete-of-create → delete, create-of-delete → recreate-with-same-id, update → restore previous values), restoring identical state. |
| R4  | The transactions layer is implemented as a *decorator* over the existing `ILinks<TLink>` / `LinkStorage` surface, with the same public methods, so it composes with `NamedTypesDecorator`, `PinnedTypesDecorator`, and `PersistentTransformationDecorator`. |
| R5  | The transitions log itself is a *doublets store*, not a bespoke binary file (the log is "also doublets storage"); the layer therefore takes two underlying links sources at construction time — one for data, one for transitions. |
| R6  | Three log retention strategies are supported: **infinite** (default), **chunked** (archive older slices into rotating files), and **size-limited** (drop the oldest applied slice after verifying it was flushed to the data store). |
| R7  | A size-limited log must double-check that every transition it is about to discard has already been *applied* to the data store before deleting it from the log, to avoid losing un-applied work. |
| R8  | Two commit modes are supported: **sync** (a `Commit()` returns only once every transition is applied to the data store) and **async** (a `Commit()` returns as soon as the transitions are durably written to the log; application to the data store happens lazily). |
| R9  | The transactions layer is *optional* — existing CLI invocations and library users that do not opt in see no behavior change. |
| R10 | The transactions layer is recoverable: on startup it detects an incomplete shutdown (last-committed marker != last-written marker) and either replays/rolls-back to a consistent state or refuses to open with a clear diagnostic (matching `Data.Doublets`' current "Database is damaged" behavior, but with a documented recovery path). |

### Version control (R11–R17)

| ID  | Requirement |
|-----|-------------|
| R11 | A `VersionControlDecorator` sits on top of the transactions layer and exposes the same `ILinks<TLink>` / `LinkStorage` surface plus VC-specific operations. |
| R12 | `Checkout(point)` *time-travels* the data store to the state at a given transition (by id, by timestamp, or by a named tag), by replaying or rewinding transitions from the current head. |
| R13 | `Branch(name, from?)` creates a new branch starting from the specified point (or the current head). Each branch is represented by version-control metadata over the shared transitions timeline, so branch state remains a links database without copying the whole log. |
| R14 | `ListBranches()` / `CurrentBranch()` / `SwitchBranch(name)` let the caller enumerate and switch between branches. |
| R15 | `Tag(point, name)` and `ListTags()` create human-friendly references to specific points in the history (analogous to git tags). |
| R16 | The version-control layer composes correctly with the transactions layer below it: normal writes are attributed to the current branch, checkout/switch replay does not create new transitions, and explicit VC transactions attribute branch metadata only after commit. |
| R17 | The version-control layer is *optional* — existing CLI invocations and library users that do not opt in see no behavior change. |

### Cross-cutting (R18–R23)

| ID  | Requirement |
|-----|-------------|
| R18 | Both layers are implemented in **C#** and in **Rust** with feature parity (subject to the same documented C# / Rust parity rules already in `docs/REQUIREMENTS.md`). |
| R19 | Both layers are exposed via **CLI flags** (in `clink`) and via **public library APIs** (`Foundation.Data.Doublets.Cli` NuGet and the `link_cli` crate). |
| R20 | The implementation includes unit tests covering commit/rollback, sync/async modes, log retention strategies, recovery, time travel, branching, and composition with the existing decorators. |
| R21 | The implementation includes documentation (in `docs/` and in the CLI help text) explaining the model. |
| R22 | Case-study data is compiled to `./docs/case-studies/issue-94/` (this folder), with extracted issue/PR data, references to prior art, and an enumeration of requirements + proposed solutions. *(This requirement is satisfied by this README.)* |
| R23 | Everything is delivered in a single pull request (#95), incrementally committed so partial work is preserved. |

## 3. Evidence captured in this folder

```
docs/case-studies/issue-94/
├── README.md                              # This document.
├── github-data/
│   ├── issue-94.json                      # Raw issue payload at investigation time.
│   ├── issue-94-comments.json             # Comments at investigation time (empty).
│   ├── issue-94-timeline.json             # Issue timeline (labels, assignments).
│   └── pr-95.json                         # PR snapshot.
└── references/
    ├── UInt64LinksTransactionsLayer.cs    # Upstream C# reference cited by the issue.
    └── UInt64LinksTransactionsLayer.h     # Upstream C++ counterpart for cross-checking.
```

The two reference files were copied verbatim from
[linksplatform/Data.Doublets@main](https://github.com/linksplatform/Data.Doublets/tree/main/csharp/Platform.Data.Doublets)
so the case study remains analyzable even if the upstream files move or
change. They are *evidence*, not vendored dependencies — the link-cli code
will not import them.

## 4. Prior art in this repository

`link-cli` already ships several composable decorators that follow the
exact pattern the new layers must follow. They are the structural template
for the transactions / version-control implementations:

| Existing decorator | File | Role |
|--------------------|------|------|
| `SimpleLinksDecorator<TLink>` | [csharp/Foundation.Data.Doublets.Cli.Library/SimpleLinksDecorator.cs](../../../csharp/Foundation.Data.Doublets.Cli.Library/SimpleLinksDecorator.cs) | Bootstraps the primary links store plus a sidecar names store. |
| `NamedTypesDecorator` | [csharp/Foundation.Data.Doublets.Cli.Library/NamedTypesDecorator.cs](../../../csharp/Foundation.Data.Doublets.Cli.Library/NamedTypesDecorator.cs) | Adds named lookup on top of links. |
| `PinnedTypesDecorator` | [csharp/Foundation.Data.Doublets.Cli.Library/PinnedTypesDecorator.cs](../../../csharp/Foundation.Data.Doublets.Cli.Library/PinnedTypesDecorator.cs) | Maintains "pinned" type ids. |
| `PersistentTransformationDecorator` | [csharp/Foundation.Data.Doublets.Cli.Library/PersistentTransformationDecorator.cs](../../../csharp/Foundation.Data.Doublets.Cli.Library/PersistentTransformationDecorator.cs) | Stores triggers in a sidecar links store and applies them after writes. |
| Rust `NamedTypesDecorator` | [rust/src/named_types.rs](../../../rust/src/named_types.rs) | Rust counterpart of `NamedTypesDecorator`. |
| Rust `PinnedTypesDecorator` | [rust/src/pinned_types.rs](../../../rust/src/pinned_types.rs) | Rust counterpart of `PinnedTypesDecorator`. |

All four existing C# decorators inherit from
`Platform.Data.Doublets.Decorators.LinksDecoratorBase<TLink>` (or, for
disposable / file-backed flavors, `LinksDisposableDecoratorBase<TLink>`).
The upstream `UInt64LinksTransactionsLayer` also inherits
`LinksDisposableDecoratorBase<TLink>` — the same base — so the C# layer
already has a well-defined place in the existing composition stack.

On the Rust side `link-cli` storage is centered on `LinkStorage` plus
`NamedLinks` / `PinnedTypes` types. PR #95 introduces the small wrapper
indirection needed to stack the transactions and version-control layers in
the same order as C#.

## 5. Prior art and online research

### 5.1 The cited reference: `UInt64LinksTransactionsLayer`

[`UInt64LinksTransactionsLayer.cs`](references/UInt64LinksTransactionsLayer.cs)
already demonstrates several pieces of the requested design:

- a `Transition` value-type that carries a transaction id, a `Before`
  link, an `After` link, and a `Timestamp`;
- a `Transaction` nested type with `IsCommitted`, `IsReverted`, `Commit`,
  and `Dispose` (auto-revert on dispose if not committed);
- a background `TransitionsPusher` task that writes queued transitions to
  a file-backed log every `DefaultPushDelay` (≈ 100 ms);
- a first-line "last committed transition" marker on the log file used at
  startup to detect un-clean shutdowns;
- `Create` / `Update` / `Delete` overrides that wrap the inner links store
  and enqueue a `Transition` in the same write-handler callback the
  underlying store already exposes.

What it is *missing* — and which this case study calls out as
"do much better than the reference":

- **Nested transactions are explicitly thrown out**: the constructor of
  `Transaction` throws `NotSupportedException("Nested transactions not
  supported.")` when there is already a current transaction. The issue
  does not mandate nested transactions, but the reference's design has no
  story for them at all.
- **Async vs. sync commit mode is hard-coded async**: every commit
  enqueues onto the layer's `_transitions` queue, which the background
  pusher writes "in a while loop with Thread.Sleep(100 ms)". There is no
  way to ask `Commit()` to flush synchronously before returning.
- **The log is a binary file, not a links store**: the reference stores
  `Transition` structs straight into a file via `Platform.IO.FileHelpers`.
  The issue is explicit that the transitions store should itself be a
  doublets store, because that is the only way to compose it with
  decorators (named transitions, pinned transitions, time-travel views).
- **No log-retention strategy**: the file grows without bound. There is no
  chunking, no size limit, no "delete only if applied" check.
- **Auto-recovery is documented as not supported**: the constructor
  throws `NotSupportedException("Database is damaged, autorecovery is not
  supported yet.")` if the first/last markers don't match.
- **No version-control concept**: the reference has no notion of branches
  or tags; there is no `Checkout`, no `Branch`, no `Tag`. The issue is
  asking us to add an entirely new VC layer on top.

### 5.2 External research (theory and patterns)

The proposed design is informed by well-documented database and
versioning theory. Each citation here is referenced again in §6 next to
the specific decision it supports.

| Topic | Source | Relevance |
|-------|--------|-----------|
| Write-ahead logging (WAL) — log records, undo+redo info, recovery | [Wikipedia: Write-ahead logging](https://en.wikipedia.org/wiki/Write-ahead_logging) | Justifies storing both *before* and *after* in each transition. WAL is the textbook pattern for atomic, recoverable writes. |
| SQLite WAL — append-only log, COMMIT = mark + flush, rollback = don't append commit | [sqlite.org/wal.html](https://www.sqlite.org/wal.html) | Justifies sync vs. async commit and shows how *checkpointing* (transferring log to data store) is the deferred-application primitive. |
| PostgreSQL WAL — point-in-time recovery (PITR) | [postgresql.org/docs/.../wal-intro.html](https://www.postgresql.org/docs/current/wal-intro.html) | Justifies that *time-travel* is a special case of "replay the log up to a point", validating R12. |
| Event sourcing — events are the truth, state is derived, snapshots, replay | [martinfowler.com/eaaDev/EventSourcing.html](https://martinfowler.com/eaaDev/EventSourcing.html) | The conceptual basis for storing transitions as the authoritative timeline and deriving any historical state by replay. |
| Git — immutable objects, branches are pointers, checkout for time travel | [git-scm.com/docs/gitcore-tutorial](https://git-scm.com/docs/gitcore-tutorial) | Branching model for R13: each branch is a pointer + its own transitions slice. |
| Dolt — SQL DB with Git-style branching, diff, merge | [docs.dolthub.com/concepts/dolt](https://docs.dolthub.com/concepts/dolt) | Existence proof that Git-style version control over structured data is a workable product surface; informs the CLI naming (`branch`, `checkout`, `tag`). |
| MVCC — multiple versions, snapshot isolation, garbage collection | [Wikipedia: MVCC](https://en.wikipedia.org/wiki/Multiversion_concurrency_control) | Informs the "limited log" retention strategy: an applied transition is "garbage" once every consumer has caught up. |
| `sled` (Rust) — log-structured, atomic batches, transactions | [docs.rs/sled](https://docs.rs/sled/latest/sled/) | Existence proof that a thread-safe, log-structured store with batches and snapshots is achievable in pure Rust if we end up wanting to switch the Rust transitions log to a third-party storage backend. |

### 5.3 Existing components in the link-cli dependency tree

These are libraries already on the dependency surface and can be reused
rather than re-implemented:

- **`Platform.Data.Doublets.Decorators.LinksDecoratorBase<TLink>`** and
  **`LinksDisposableDecoratorBase<TLink>`** — the abstract base for any
  links decorator. `PersistentTransformationDecorator`,
  `NamedTypesDecorator`, and the upstream `UInt64LinksTransactionsLayer`
  all derive from one of these.
- **`Platform.Timestamps.UniqueTimestampFactory`** — already used by the
  upstream layer to produce monotonic timestamps for transitions.
- **`Platform.IO.FileHelpers`** — append-only file helpers already used by
  the upstream reference. PR #95 replaces the binary file with a
  doublets store (per R5); if a future side-channel marker file is needed,
  this remains the established helper.
- **`doublets` crate** (Rust) — already a dependency, provides the same
  storage primitives in Rust. We can stack a new decorator over it.

## 6. Implemented solution

PR #95 implements the case-study plan in C# and Rust. The implementation
keeps the layers opt-in: without `--transactions` or `--vc`, the existing
links storage path is unchanged and no transaction or version-control
sidecar is created.

### Transactions layer

The C# `TransactionsDecorator` and Rust `transactions::TransactionsDecorator`
wrap the data store plus a second doublets store used as the transitions
log. Each write records a `Transition` with transaction id, sequence,
timestamp, and full before/after link state. Explicit transactions expose
`BeginTransaction()`, `Commit()`, `Rollback()`, and rollback-on-dispose in
C#; Rust exposes the same lifecycle as `begin_transaction()`, `commit()`,
and `rollback()` methods on the decorator. Auto transactions still wrap
one standalone write.

The log is durable sidecar state:

- transition records, commit markers, rollback markers, and applied markers
  are persisted as names inside the transitions doublets store;
- recovery scans that sidecar on open, replays committed-but-unapplied
  transitions, and rolls back incomplete transactions;
- `sync` and `async` commit modes are available;
- `infinite`, `sized:<n>`, and `chunked:<n>:<dir>` retention policies are
  implemented, with sized/chunked retention only removing applied entries.

The C# recorder captures one transition per affected link in a logical write.
This matters for ACID atomicity because a delete can also update or remove
links that refer to the deleted link. `CreateAndUpdate(null, null)` continues
to log the existing create-plus-update sequence for compatibility with prior
checkout behavior.

### Version-control layer

The C# `VersionControlDecorator` and Rust
`version_control::VersionControlDecorator` sit above the transactions layer.
They add:

- `Branch(name, from?)`, `SwitchBranch(name)`, and `ListBranches()`;
- `Tag(name, seq?)`, `TryGetTag(...)`, and `ListTags()`;
- `Checkout(seq)` for rewind/replay time travel;
- branch attribution for normal writes;
- explicit version-control transactions that defer branch metadata until the
  inner transaction commits.

Branch metadata, tags, current-branch, applied sequence, and transition-to-
branch attribution are persisted in the version-control sidecar doublets
store. Branches are represented as metadata over the shared transition
timeline rather than copied per-branch transition files. Checkout and branch
switching apply or revert existing transitions without recording new writes.

### CLI and public APIs

Both implementations expose the requested CLI controls:

- `--transactions`, `--transactions-file`, `--commit-mode`, `--retention`,
  and `--log`;
- `--vc`, `--vc-file`, `--branch`, `--branch-from`, `--checkout`, `--tag`,
  `--list-branches`, and `--list-tags`.

The same functionality is available through the C# library types and Rust
modules, so callers can compose these layers directly without going through
the CLI.

### Verification added

The PR includes unit and integration coverage for:

- auto and explicit transaction commit/rollback;
- rollback-on-dispose and nested transaction rejection;
- update/delete reversal and recovery replay;
- sync/async commit modes;
- sized and chunked retention;
- branch creation, branch switching, checkout, tags, and metadata recovery;
- full-stack ACID rollback and commit/durability tests that run through the
  version-control layer on top of the transactions layer in both C# and Rust.

The C# durability coverage also reopens the data, transaction-log, and
version-control sidecar files after deterministic disposal of the file-backed
`NamedTypesDecorator` stores.

## 7. Risks and trade-offs

| Risk | Mitigation |
|------|------------|
| Writing every transition into a *links* store, rather than a flat file, is slower than the upstream reference's `FileStream.Write(transition)`. | Acceptable for correctness; the issue explicitly asks for this. The log store can use the same `UnitedMemoryLinks` backend the main store already uses, so the overhead is well-understood. Async commit mode preserves the latency benefit of the flat-file approach for write-heavy workloads. |
| Branches can diverge for a long time. | Branches share the transitions timeline and store only branch metadata plus new branch-specific transitions, avoiding full log copies. Retention policies still bound applied history where configured. |
| The Rust storage surface differs from the C# decorator hierarchy. | The Rust implementation wraps the existing `LinkStorage`, `NamedLinks`, and `PinnedTypes` behavior behind small transaction/version-control modules rather than importing a new storage framework. |
| The upstream reference rejects nested transactions. The issue does not ask for nesting, but a future user might. | Out of scope for this PR; we throw a clear `NotSupportedException` (C#) / `Err(TransactionsError::NestedNotSupported)` (Rust) and document it. |
| Time-travel via checkout has to *invert* all newer transitions when going back in time. If the log is very long this is O(n). | Documented as O(n); a `Snapshot(point)` API that materializes a checkpoint can be added later (event-sourcing style) if the linear cost becomes a problem in practice. |
| Crash recovery is hard to prove exhaustively. | Startup recovery is implemented and covered by reopen/replay tests. Full process-kill stress testing remains useful future hardening, but the current implementation no longer treats recovery as out of scope. |

## 8. Existing libraries we considered

| Library | Decision |
|---------|----------|
| `sled` (Rust) | **No** — full re-platform of storage, not a decorator. We borrow design ideas (`Tree::transaction`, atomic batches) but not the dependency. |
| `rocksdb` / `lmdb` | **No** — same reason. |
| `pijul` / `git2` (Rust) | **No** — version-control libraries but with their own data model. Our VC layer is over *links*, not files. |
| `LiteDB` (.NET) | **No** — alternative storage, not a decorator pattern. |
| `Platform.IO.FileHelpers` (already a dependency) | **Yes** — for side-channel markers if needed. |
| `Platform.Timestamps.UniqueTimestampFactory` (already a dependency) | **Yes** — direct reuse for monotonic timestamps in transitions. |
| `Platform.Data.Doublets.Decorators.LinksDecoratorBase<TLink>` (already a dependency) | **Yes** — direct reuse as the base class for the new C# decorators. |
| `doublets` crate (already a dependency) | **Yes** — direct reuse for the Rust transitions store. |

## 9. Verification

Local and CI verification for PR #95 covers both implementations:

- `dotnet build csharp/Foundation.Data.Doublets.Cli.sln`
- `dotnet test csharp/Foundation.Data.Doublets.Cli.sln`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --manifest-path rust/Cargo.toml`

The focused ACID suites are:

- `csharp/Foundation.Data.Doublets.Cli.Tests/TransactionsDecoratorTests.cs`
- `csharp/Foundation.Data.Doublets.Cli.Tests/VersionControlDecoratorTests.cs`
- `rust/tests/transactions_decorator_tests.rs`
- `rust/tests/version_control_decorator_tests.rs`

## 10. Delivery on PR #95

PR #95 contains the case study, implementation, tests, and documentation
updates for issue #94. It is ready for review once the latest local checks and
GitHub Actions checks pass after the final commits.

Per the issue: *"Please plan and execute everything in a single pull
request, you have unlimited time and context, as context auto-compacts
and you can continue indefinitely, until it is each and every requirement
fully addressed, and everything is totally done."*
