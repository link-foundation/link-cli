---
'Foundation.Data.Doublets.Cli': minor
---

Reject complete corrupt journal entries and report replay failures. Add explicit
data/journal flush callbacks and checkpoint only after data reaches storage.

Add an optional staged links engine with configurable concurrency, isolation,
immutable revision retention, constraints and replaceable file/replicated logs.

Support independent journal branches and atomic three-way merge, with saved
merge bases for reopen/retry after uncertain I/O outcomes.

Compose atomic multi-shard transactions with replaceable journal stores and an
optional etcd HTTP adapter, with bounded I/O, linearizable snapshots, generation
validation, unknown outcomes and actual three-member failover integration.

Route immutable journals to independent data stores while atomically publishing
small catalog pointers; preserve cross-shard validation and unknown-outcome
discovery across independent consensus groups.

Add optional portable delta journal decorators with bounded checkpoint chains,
exact logical replay, corruption rejection and uncertain-write fencing.
Avoid unnecessary recovered-row conflict metadata and skip per-row generation
tracking for exclusive/whole-revision serializable strategies. Reclaim generations
after the final session validates, preserving optimistic ABA/branch conflicts.

Select serializable point-read validation independently of journal/history settings.
Permit disjoint point edits while checking missing rows, ABA, branch changes and
read/write cycles; scans retain conservative revision validation.

Preserve storage outcomes when optional store, query, server, transaction,
trigger or version console diagnostics fail. Retention no longer replays
rolled-back writes or archives them as committed changes; background retention
waits for pending transactions.

Retain observed partial writes when storage or user callbacks throw, undo
automatic operations and preserve explicit rollback state. Keep the original
absent state across repeated allocation/update callbacks.

Release owned data handles and partially initialized names mappings when a
constructor fails; leave borrowed data usable after failed construction.

Retain the complete observed undo batch before journal writes. Reject commit and
further writes after an append failure, roll back automatic failures, and expose
Rollback() on the decorator to retry cleanup when an implicit rollback fails.

Make the transaction rollback-only before undo starts, preserving the abort
decision when undo or rollback persistence fails until cleanup is retried.

Release CLI-owned mapped data, journal, version and trigger stores before the
entry point returns, including failure while constructing a later layer.

Reject exhausted legacy transaction sequence space before mutation and retain
complete pending undo when a cascade exceeds remaining sequence capacity.

Wait for active asynchronous checkpoints before transaction shutdown returns,
serialize concurrent shutdown callers and reject callback self-joins. Preserve
borrowed-store ownership and journal recovery for cancelled/failed checkpoints.

Select optional pessimistic point-write ownership with immediate busy errors,
independent isolation/read-set validation, stale/ABA rejection, branch protection
and cleanup on every session outcome. Keep engine-local locks out of cluster
workspaces and preserve existing optimistic/exclusive defaults.

Dispose complete facade graphs with borrowers before their dependencies, joining
asynchronous checkpoints before mapped stores close. Share nested/cyclic traversal
state, preserve detached references, and leave dependencies available when borrower
cleanup fails and must be retried.

Keep version-control admission and owner cleanup retry after failed transaction
rollback. Release admission when the inner transaction reaches a terminal state,
including a failure reported after rollback completes.

Finish committed version publication after checkpoint or metadata errors through
transaction/owner disposal and rollback. Keep write and metadata admission fenced
until publication succeeds, and avoid duplicate transition markers on retry.

Publish the final signed version sequence without wrapping. Initialize version
history from retained journal records, avoiding phantom versions across gaps.

Persist branch and parent names with an unambiguous versioned marker, retaining
legacy reads. Recover tag names containing equals signs without truncation.

Fence named writes while committed version publication is pending. Reject
public journal or version recovery while an open transaction owns live state.

Retain known committed synchronous checkpoints through failed persistence.
Retry them through Commit, CommitAsync, Rollback or transaction disposal before
releasing foreground admission or publishing version metadata. Fence row and
named writes during incomplete commit/rollback cleanup; preserve queued async
policy and borrowed-store shutdown semantics.

Recover the ordered committed suffix after an applied-marker gap. Persist the
complete redo and pending undo before closing that gap, so interrupted recovery
cannot overwrite later committed updates, deletions or recreated rows.

Checkpoint current inline effects when sized/chunked retention encounters an
older failed async checkpoint; preserve newer updates, deletion and recreation.

Retain uncertain commit attempts before marker append/flush, fence new writes
and resolve the decision through cleanup without undoing a possibly durable
commit. Avoid duplicate decision markers on retry and retain version publication
ownership when cleanup resolves an uncertain inner commit.

Exclude aborted and recovered pending records from initial version history and
its branch head. Reject existing sidecars that attribute a known uncommitted
record, and reject version construction/recovery over a live inner transaction.

Retain an implicit version owner for automatic writes through failed commit and
metadata publication. Retry committed work through cleanup, preserve ordinary
pre-commit undo (including failed undo retry), and fence all new mutations until
the original branch publication finishes. Preserve no-op and async policies.
