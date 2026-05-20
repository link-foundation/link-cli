# Issue 94 Case Study: Optional Transactions and Version Control Layers

Issue: <https://github.com/link-foundation/link-cli/issues/94>

Prepared PR: [#95](https://github.com/link-foundation/link-cli/pull/95)

> Scope of this case study: this folder captures evidence, restated
> requirements, prior-art analysis, and a multi-phase implementation plan
> for shipping an *optional* transactions decorator and an *optional*
> version-control decorator in both the C# and Rust implementations of
> `link-cli` (CLI + library). The case study is the deliverable for the
> first half of the issue. The actual code implementation is split into
> follow-up engineering work tracked from this same PR, because the issue
> body explicitly asks to first *"collect data related about the issue to
> this repository, make sure we compile that data to
> `./docs/case-studies/issue-{id}` folder, and use it to do deep case study
> analysis"* before the code lands.

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
   that creates a new transitions file starting from a point in time of an
   existing branch.

The work must be delivered in both **C# and Rust**, both as **CLI flags**
and as **public library APIs**, and must compose with the existing
named-types / pinned-types / persistent-transformation stack the same way
the existing decorators compose.

The issue specifically points to
[`linksplatform/Data.Doublets/.../UInt64LinksTransactionsLayer.cs`](https://github.com/linksplatform/Data.Doublets/blob/main/csharp/Platform.Data.Doublets/UInt64LinksTransactionsLayer.cs)
as a starting reference, while noting that it is *not finished* and we
"should do much better."

## 2. Restated requirements

Broken down into discrete, individually-testable requirements so that the
implementation pull request can check each one off:

### Transactions (R1–R10)

| ID  | Requirement |
|-----|-------------|
| R1  | Each link operation (Create, Update, Delete) is recorded as a *reversible transition* with enough information to recompute both the *before* and *after* state. |
| R2  | A `BeginTransaction()` API returns a `Transaction` handle that supports `Commit()`, `Rollback()`, and `Dispose()` (auto-rollback if dropped without commit). |
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
| R13 | `Branch(name, from?)` creates a new branch starting from the specified point (or the current head). Each branch is backed by its own transitions file that starts after the parent's branch-point. |
| R14 | `ListBranches()` / `CurrentBranch()` / `SwitchBranch(name)` let the caller enumerate and switch between branches. |
| R15 | `Tag(point, name)` and `ListTags()` create human-friendly references to specific points in the history (analogous to git tags). |
| R16 | The version-control layer composes correctly with the transactions layer above it (so write operations during version control time-travel are recorded back into the appropriate branch's log). |
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

On the Rust side `link-cli` does not yet have an explicit decorator
trait; storage is centered on `LinkStorage` plus `NamedLinks` /
`PinnedTypes` types. Adding a transactions layer to Rust therefore also
requires *introducing* a small "links decorator" indirection in `rust/src`
so the new layer can be stacked the same way it is in C#. This is called
out as a design choice in §6.

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
  the upstream reference. We will *replace* the binary file with a
  doublets store (per R5), but if we ever need a side-channel marker file,
  this is the established way.
- **`doublets` crate** (Rust) — already a dependency, provides the same
  storage primitives in Rust. We can stack a new decorator over it.

## 6. Solution plan

The plan is split into six self-contained steps. Each step is committable
and reviewable independently and is sized to fit a single follow-up
commit on PR #95.

### Step S1 — Establish the case study (this commit)

Create `docs/case-studies/issue-94/` with this README, evidence files,
and references. **Done in this commit.** Satisfies R22.

### Step S2 — Define the shared `ITransactionsLinks<TLink>` API surface

Add the API surface in both languages without an implementation. This
locks the design down and lets the test plan be written before the actual
storage code.

In C# (`csharp/Foundation.Data.Doublets.Cli.Library/Transactions/`):

```csharp
public interface ITransactionsLinks<TLink> : ILinks<TLink>
{
    ITransaction BeginTransaction();           // R2
    Task<ITransaction> BeginTransactionAsync(CancellationToken ct = default);
    IReadOnlyList<ITransition<TLink>> Log { get; }
    LogRetentionPolicy RetentionPolicy { get; }
    CommitMode CommitMode { get; set; }       // Sync vs. Async (R8)
}

public interface ITransaction : IDisposable
{
    Guid Id { get; }
    DateTimeOffset StartedAt { get; }
    bool IsCommitted { get; }
    bool IsRolledBack { get; }
    void Commit();                             // R2
    void Rollback();                           // R2
    Task CommitAsync(CancellationToken ct = default);
}

public readonly record struct Transition<TLink>(
    Guid TransactionId,
    long Sequence,
    DateTimeOffset Timestamp,
    Link<TLink> Before,
    Link<TLink> After);

public enum CommitMode { Sync, Async }         // R8

public abstract record LogRetentionPolicy
{
    public sealed record Infinite() : LogRetentionPolicy;                            // R6
    public sealed record Chunked(long ChunkSize, string ArchiveDirectory) : LogRetentionPolicy; // R6
    public sealed record Sized(long MaxBytes) : LogRetentionPolicy;                  // R6 + R7
}
```

In Rust (`rust/src/transactions/mod.rs`):

```rust
pub trait TransactionsLinks: LinksStorage {
    type Transaction: Transaction;
    fn begin_transaction(&self) -> Self::Transaction;
    fn log(&self) -> &dyn LogReader;
    fn commit_mode(&self) -> CommitMode;
    fn set_commit_mode(&mut self, mode: CommitMode);
}

pub trait Transaction: Drop {
    fn id(&self) -> u128;
    fn started_at(&self) -> SystemTime;
    fn is_committed(&self) -> bool;
    fn is_rolled_back(&self) -> bool;
    fn commit(self) -> Result<()>;
    fn rollback(self) -> Result<()>;
}

pub enum CommitMode { Sync, Async }
pub enum LogRetentionPolicy {
    Infinite,
    Chunked { chunk_size: u64, archive_dir: PathBuf },
    Sized { max_bytes: u64 },
}
```

The `LinksStorage` trait extracted on the Rust side is the new
indirection mentioned in §4. It is a minimal trait covering `create`,
`update`, `delete`, `each`, `get_link`, and `exists` — exactly the
methods that `LinkStorage` already implements. Existing call sites
re-route through the trait without behavior change.

Satisfies the *API* parts of R1, R2, R4, R6, R8, R18, R19.

### Step S3 — Implement transactions on top of a *doublets* log

In each language, implement the API by storing transitions as links in a
*second* doublets store. The store layout encodes one transition as a
small graph:

```text
(<transaction-id> :transaction-root)
(<sequence-id>    :sequence-of    <transaction-id>)
(<sequence-id>    :timestamp      <timestamp-link>)
(<sequence-id>    :before-source  <link-source>)
(<sequence-id>    :before-target  <link-target>)
(<sequence-id>    :after-source   <link-source>)
(<sequence-id>    :after-target   <link-target>)
```

The keys `:transaction-root`, `:sequence-of`, `:timestamp`,
`:before-source`, `:before-target`, `:after-source`, `:after-target` are
*named points* exactly the way `PersistentTransformationDecorator`
already represents `Type`, `Trigger`, `Once`, `Always`, `Condition`,
`Substitution` in the trigger sidecar (see
`csharp/Foundation.Data.Doublets.Cli.Library/PersistentTransformationDecorator.cs:242-258`).
This means the transitions log is itself queryable through the existing
LiNo query processor — which directly supports the issue's "log itself is
also doublets storage" requirement (R5).

The C# implementation derives from `LinksDisposableDecoratorBase<TLink>`
exactly like `UInt64LinksTransactionsLayer` does, and the Rust
implementation implements the new `TransactionsLinks` trait by wrapping
two `LinkStorage` instances (one for data, one for the log).

Implementation notes:

- `Commit()` (sync mode, R8) walks the in-memory transaction's transition
  list and synchronously writes each transition record into the log
  store, then *applies* it to the data store, then flushes both.
- `Commit()` (async mode, R8) writes the transition records into the log
  store synchronously, then enqueues the data-store application onto a
  background task. The transaction is "committed" as soon as the log is
  durable, mirroring SQLite's WAL commit semantics.
- `Rollback()` (R3) iterates the transitions in reverse and inverts each
  operation against the *data* store (create → delete by id, delete →
  recreate with prior source/target, update → update back). The log
  records the rollback as additional transitions tagged with the parent
  transaction's id so the history remains complete and reproducible.
- `Dispose()` on `Transaction` calls `Rollback()` if `IsCommitted ==
  false && IsRolledBack == false` (mirrors the upstream reference).
- The `Sized` retention policy (R7) only drops the oldest *applied*
  chunk: a transition is "applied" once its data-store write has
  succeeded. In async mode the "applied" set is exactly the prefix the
  background applier has caught up to.

Satisfies R1, R2, R3, R4, R5, R6, R7, R8.

### Step S4 — Recovery, durability, and async backpressure

On startup the transactions layer scans the log, finds the last fully
applied transition (the one whose data-store side-effect is observable),
and either:

- replays remaining log entries forward into the data store (async mode
  catch-up), or
- rolls back any log entries that belong to an *un-committed* transaction
  (transactions whose final `:commit` marker is missing).

In async mode the background applier signals backpressure to the writer
when the log gets too far ahead of the data store, by transparently
falling back to sync commits until the queue has caught up. This is the
canonical WAL recovery + checkpoint pattern from the PostgreSQL and
SQLite write-ups cited in §5.2.

Satisfies R10.

### Step S5 — Implement the `VersionControlDecorator`

On top of the transactions layer, add a separate decorator that adds:

- a `branches` named-points subgraph in the log store
  (`:branch <name>`, `:branch-head <sequence-id>`, `:branch-parent
  <parent-branch>`, `:branch-parent-point <sequence-id>`);
- a `tags` named-points subgraph (`:tag <name>`, `:tag-point
  <sequence-id>`);
- a `Checkout(point)` method (R12) that walks the data store back to the
  requested point by inverting transitions newer than the point and
  re-applying them when checking out a forward point;
- a `Branch(name, from?)` method (R13) that creates a new branch row in
  the log and *forks* the underlying log file into a new sidecar
  (`<db>.<branch-name>.transitions.links`) so further writes on that
  branch don't pollute the parent's log;
- a `SwitchBranch(name)` method (R14) that performs a `Checkout(point)`
  to the branch's head and points all subsequent writes at the branch's
  log;
- a `Tag(point, name)` / `ListTags()` API (R15);
- composition guarantees with the inner transactions layer (R16): every
  write during VC time-travel is recorded back into the *current
  branch's* log.

The decorator inherits from `LinksDecoratorBase<TLink>` in C# and
implements the same `LinksStorage` trait extracted in S2 in Rust, so it
can in turn be wrapped by `NamedTypesDecorator`,
`PinnedTypesDecorator`, and `PersistentTransformationDecorator` if a user
opts in. The order of composition is documented as:

```text
PersistentTransformationDecorator
└── PinnedTypesDecorator
    └── NamedTypesDecorator
        └── VersionControlDecorator       (optional, R11)
            └── TransactionsDecorator     (optional, R1-R10)
                └── UnitedMemoryLinks     (data store)
                            +
                ┌── named transitions store (doublets) (R5)
```

Satisfies R11, R12, R13, R14, R15, R16.

### Step S6 — CLI flags and library examples

CLI (`clink`) additions in both implementations:

- `--transactions <path>` — enable the transactions layer; `<path>` is
  the doublets log store (default: `<db>.transitions.links`).
- `--commit-mode sync|async` — choose sync or async commits (R8).
  Defaults to `sync` for safety.
- `--retention infinite|sized:<bytes>|chunked:<bytes>:<dir>` — set the
  retention policy (R6, R7).
- `--vc` — enable the version-control decorator (R11).
- `--branch <name>` — switch to a branch (creating it if `--branch-from
  <point>` is also passed) (R13, R14).
- `--checkout <point>` — time-travel the data store to a specific
  transition id, timestamp, or tag (R12).
- `--tag <name>=<point>` — create a tag (R15).
- `--list-branches`, `--list-tags`, `--log` — read-only inspection
  commands.

Library examples added under `examples/`:

- `examples/transactions-csharp/` — minimal C# program that opens a links
  store with the transactions decorator, begins a transaction, performs
  a few CRUD operations, and either commits or rolls back.
- `examples/transactions-rust/` — Rust equivalent.
- `examples/version-control-csharp/` and
  `examples/version-control-rust/` — branch, tag, and checkout demos.

Satisfies R9, R17, R18, R19, R21.

### Step S7 — Tests

For each language:

- Unit tests for commit, rollback, dispose-without-commit, nested
  transactions (asserting current "not supported" behavior with a clear
  error), and a stress test that performs random CRUD with random
  commit/rollback decisions and asserts that the data store ends in the
  same state as a reference Hash-based replay.
- Recovery tests: kill mid-write (via injected fault), reopen, assert the
  layer recovers to the last fully-committed state.
- Retention tests for `Sized` and `Chunked` policies, asserting that no
  un-applied transition is ever dropped (R7).
- Branch / tag / checkout tests that build a small history with two
  branches and assert that switching back and forth produces byte-identical
  database snapshots at every named point.
- Composition tests that stack `NamedTypesDecorator` /
  `PinnedTypesDecorator` / `PersistentTransformationDecorator` on top of
  the new layers and re-run the existing CRUD test suite, asserting no
  regressions.

Satisfies R20.

### Step S8 — Documentation

- Update `docs/REQUIREMENTS.md` to mark the optional transactions and
  version-control entries as *implemented*.
- Update `docs/ARCHITECTURE.md` with the new composition stack
  illustrated in S5.
- Update `docs/HOW-IT-WORKS.md` with a "Time travel and branching"
  section that walks through a small example.
- Update both `csharp/README.md` and `rust/README.md` with the new CLI
  flags and library APIs.
- Cross-link this case study from the documentation index.

Satisfies R21, R22.

## 7. Risks and trade-offs

| Risk | Mitigation |
|------|------------|
| Writing every transition into a *links* store, rather than a flat file, is slower than the upstream reference's `FileStream.Write(transition)`. | Acceptable for correctness; the issue explicitly asks for this. The log store can use the same `UnitedMemoryLinks` backend the main store already uses, so the overhead is well-understood. Async commit mode preserves the latency benefit of the flat-file approach for write-heavy workloads. |
| Branching requires forking the log file. If two branches diverge for a long time, the on-disk footprint is roughly `O(branches × transitions)`. | Documented limitation. Mirrors git's on-disk model. The `Chunked` retention policy can rotate inactive branches into archived chunks. |
| The Rust side currently has no explicit `LinksDecorator` trait, so adding the transactions layer requires extracting one. | The extraction is mechanical — the existing `LinkStorage`, `NamedLinks`, and `PinnedTypes` types already implement the same effective surface. We refactor in a single commit so the trait extraction is reviewable on its own. |
| The upstream reference rejects nested transactions. The issue does not ask for nesting, but a future user might. | Out of scope for this PR; we throw a clear `NotSupportedException` (C#) / `Err(TransactionsError::NestedNotSupported)` (Rust) and document it. |
| Time-travel via checkout has to *invert* all newer transitions when going back in time. If the log is very long this is O(n). | Documented as O(n); a `Snapshot(point)` API that materializes a checkpoint can be added later (event-sourcing style) if the linear cost becomes a problem in practice. |
| Auto-recovery on a crashed log is not in scope yet; the upstream reference also lacks it. | This case study calls it out (R10) and S4 provides a *correct* recovery story by validating commit markers on startup, but a full crash-stress test suite is deferred to a follow-up. |

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

## 9. Verification plan

- `dotnet build csharp/Foundation.Data.Doublets.Cli.sln -c Release`
  succeeds with the new project sources.
- `dotnet test csharp/Foundation.Data.Doublets.Cli.sln -c Release` passes
  all existing tests *and* the new transactions / version-control tests.
- `cargo build --manifest-path rust/Cargo.toml --release` succeeds.
- `cargo test --manifest-path rust/Cargo.toml` passes all existing tests
  *and* the new ones.
- `cargo fmt --check` and `cargo clippy --all-targets --all-features
  -- -D warnings` keep passing.
- The CI on PR #95 keeps passing across `ubuntu-latest`, `macos-latest`,
  and `windows-latest`.
- The new examples under `examples/transactions-*` and
  `examples/version-control-*` each run end-to-end via `dotnet run` /
  `cargo run` and demonstrate a committed transaction, a rolled-back
  transaction, and a checkout/branch round-trip.

## 10. Delivery plan on PR #95

This case study is the first commit on PR #95. The follow-up commits will
each correspond to one of the steps S2–S8 above, in order. The PR will
remain *draft* until S8 lands; only then will it be marked ready for
review.

Per the issue: *"Please plan and execute everything in a single pull
request, you have unlimited time and context, as context auto-compacts
and you can continue indefinitely, until it is each and every requirement
fully addressed, and everything is totally done."*
