# LSM composition implementation notes (proposal)

This is unimplemented design work. Do not describe the model or sorted table
primitive as a finished LSM, or mark the wider issue complete.

Use the existing atomic journal API so `ClusterEngine`, named links, immutable
object routing and optional maintenance can select the new local store. Keep
every existing implementation/default. Expose a separate JS entry and a Rust
optional feature; default core paths must not import this component or SQLite.

Proposed portable `PhysicalLsmJournalStore<W,P>` takes a caller-owned exclusive
replaceable WAL, a caller-owned exclusive table namespace/provider and options.
Rust uses `ReplaceableTransitionLogStore`; C# uses `IReplaceableCommitLog`; JS
requires synchronous read/append/flush/replace. Reject unsupported replacement
and invalid options before I/O. Caller supplies WAL persistence/size bounds;
the native factory configures the actual checkpoint-page limits. The existing
log API returns a complete snapshot: do not claim pre-allocation read bounds for
arbitrary custom log providers that do not enforce their own read limits.

Proposed `LsmTableProvider` operations: declared durable-flush capability;
create/open a positive u64 ID as an exclusive page device; explicitly close a
returned device; list a complete bounded set of IDs; delete one owned ID and
sync its namespace. No path reuse, automatic stale-lock theft, partial listing
or caller-owned WAL close is permitted. Rust associated device type implements
`PageDevice`; the existing sorted table does all physical record validation.

The store owns the leases returned by its provider and closes all of them on
close or failed construction, collecting errors without hiding the first error.
C# serializes calls and rejects provider reentry; JS rejects synchronous reentry
and thenables. Rust mutable methods enforce single-owner access. Any uncertain
mutation fences the store until actual reopening. Do not transparently resend
a CAS, table creation, checkpoint replacement or delete after an error.

Proposed versioned record shapes, to freeze with an independent oracle before
production implementation:

* WAL checkpoint:
  `["native-lsm1",globalGeneration,coveredGeneration,lastReservedTableID,manifest,buffer]`.
  Generations and IDs are canonical decimal u64 strings, never JS numbers.
  Initial counters are zero. `coveredGeneration <= globalGeneration`; every
  selected table row has a positive generation at most `coveredGeneration`,
  and every buffered row is newer than `coveredGeneration` and no newer than
  `globalGeneration`. The greatest retained row generation equals the global
  generation, except for the initial empty state. Tombstones count as rows.
  `lastReservedTableID` increases durably before physical file creation; selected
  IDs are positive and no greater than this counter. Using the last reserved ID
  permits reserving u64::MAX without an overflowing next-ID sentinel.
* Manifest entries bind `[id,nativeIdentityHex,digest,keyCount,storedBytes]`.
  IDs are unique and sorted by reservation order; identity is exactly 16 bytes
  of lowercase hex; digest is canonical u64. Count/bytes have finite bounds.
* Buffered rows and ordinary WAL groups use sorted unique
  `[key,generation,entriesOrNull]` rows with the sealed-table key/u64/Unicode
  rules. A null payload is a tombstone; an empty array is a live empty journal.
* Ordinary record: `["put",nextGlobalGeneration,rows]`. Nonempty groups increment
  global generation exactly once. Every changed row gets this generation.
  All selected expected generations, including read-only keys, must match before
  any publication. Bound groups to at most 32 changed rows. No updates and
  conflicts allocate no generation and do no I/O. Generation exhaustion rejects
  changed CAS before I/O; reads and maintenance can continue. Exhausted table
  IDs reject the next table reservation before mutation.

An atomic checkpoint replacement preserves the selected manifest and all
buffered acknowledged values while incrementing `lastReservedTableID` durably. Only
then may a table use the reserved ID. A crash during creation leaves an
unselected orphan whose name is never reused. After the complete table is
synchronized and reopened/validated, atomically replace the WAL checkpoint to
select the table, advance `coveredGeneration` to `globalGeneration` and remove
the covered buffer. After that durable decision,
close and delete old unselected table files. Compaction follows the same order.

Before exposing recovery results, validate complete bounded WAL semantics and
all selected sealed tables, including their native identities/digests/counts.
Never fall back to an obsolete table after a selected table is absent/damaged.
Reject unknown formats, impossible generation relationships, duplicate file IDs,
equal-generation conflicting values, and malformed or oversized state before
cleanup. Scan the complete provider namespace before deleting any orphan.
Only IDs at most `lastReservedTableID` and outside the manifest are
eligible; an unreserved or foreign file must not be deleted automatically.

Retain bounded memtable payloads and per-key metadata (generation, tombstone,
logical byte cost and owning table ID), not a complete disk payload map. Open
may transiently merge bounded payloads to validate consistency and capacities.
Reads use the buffer or the selected table's indexed physical point lookup.
Flush changes ownership metadata for buffered keys; compaction rebuilds the
bounded full index. Tombstone generations survive compaction and count against
the total-key bound. Deleting a missing key is a no-op; recreating a deleted key
gets a higher generation. Maintenance scans return a complete owned snapshot
or an error; live empty journals must not disappear from listings.

First configuration should explicitly bound total retained keys/UTF-8 logical
bytes, buffered keys/bytes, run count, WAL records/bytes and namespace entries.
Charge key bytes, eight generation bytes and one byte per journal record in
addition to its UTF-8 payload. Container overhead is additionally bounded by
record count. Default total 4096 keys/8 MiB, buffer64 keys/1 MiB, four runs and
bounded WAL are proposals, not current API promises. Table encoding can expand
control/Unicode characters differently in each JSON writer: preflight actual
encoded table size, or select a proven conservative 64 MiB physical-table cap
under an 8 MiB logical cap. Do not assume identical escaping or encoded byte
cost across Rust/C#/JS.

Before appending a CAS group, preflight the complete input, resulting logical
capacity and single-group admission. Flush/checkpoint first if necessary to
keep buffering/replay finite. A group too large for the selected buffer must
reject before mutation. Append one complete group, flush, then update the
visible buffer/index and acknowledge. A failed flush leaves discovery to reopen.
Use a full merge into one sealed table when the run limit is reached; this is
bounded foreground compaction with explicit amplification, not background
leveled compaction. Preflight merged contents before reserving/creating files.

Native directory facade should use an exclusive namespace fence in addition
to the existing root/bank/table file owners. Close children and WAL before
releasing the namespace fence; any cleanup error retains that fence. A stale
fence requires explicit external fencing, consistently with existing native
page providers. Controlled process-exit tests can remove stale owners only
after observing child exit. Namespace names are canonical `sst-<u64>` IDs with
known native metadata suffixes. Reject symlinks/foreign occupied names before
creation or cleanup. Parent-directory synchronization belongs to the provider.

Test before implementation: minimum missing-API controls; group atomicity and
stale read guards; no-op/conflict no I/O; max generation/ID; owned values;
buffer flush and actual file reads; repeated WAL checkpointing/compaction;
all before/after mutation cuts; truncated/unknown/mixed WAL and table vectors;
orphan nonreuse and invalid-list preservation; every cleanup failure; actual
native process exits; 25 native-file writer/reader pairs; existing cluster
links/history/allocator behavior; strict optionality and actual WASM controls.
The finite `model-lsm-publication.py` oracle currently passes 12 publication
cuts and two selected-table damage controls. It is not production verification.
