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
