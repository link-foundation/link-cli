# Epoch-fenced immutable journal chunks (`chunks2`)

This format extends separately selected reclamation;
the append-only chunks1 namespace remains incompatible and unchanged.

Keep blob2 as the default write format. Explicit chunks select the same
JournalChunkOptions record/count/UTF-8-byte limits as append-only chunks1.
Read layouts independently of the current write policy. No automatic collection.

Live root records:

    chunks2, partition, total-records, chunk-count,
    object-key, object-generation, object-record-count,
    ...one triple per ordered chunk

Use the same canonical finite counts as chunks1: at most 4,096 objects,
65,536 records and 8,388,608 UTF-8 bytes per object; positive u64 generations;
and a u32 total. A live empty root
has four records and no objects. Absence is generation zero with no records.
The data key is the existing blob2-EPOCH-HASH address: lowercase fixed-width
sixteen-digit hexadecimal epoch and FNV64 hash, with unchanged blob2| UTF-8
length framing. Read references may have any epoch <= the observed catalog
epoch; reject every future epoch before data I/O. Decode every selected root
before fetching any chunk. Verify generation and address before caching an
object. Validate complete count and byte bounds again when an object cache
supplies its verified payload.

Before staging, validate/copy all updates, select routes and split all journals.
Capture all requested root generations together with __link_gc_epoch. Reject
stale workspaces before staging. Every staged key must use that captured epoch,
including unchanged prefixes. Reuse is possible only within the same epoch.
After a fence, restage the complete journal under current-epoch keys. Always
fetch and compare actual complete content when staging, irrespective of cache.
One catalog CAS compares all requested generations and the captured epoch root;
its updates leave that epoch root unchanged. Never retry a mutation on a lost
response. An epoch CAS conflict leaves unpublished current/previous objects.

Root discovery protects every ordered chunk, including duplicates. Reject
contradictory generations for an identical partition/key. Scan every configured
namespace completely and validate every referenced exact generation before
ANY destructive action. Preserve all protected keys across namespace aliases.
Only conditionally delete keys with epoch strictly below the collector's own
successfully published fence, using the exact generation observed in its scan.
Concurrent later fences do not widen that collector's deletion cutoff.

If any chunk is absent while fetching a group, obtain one fresh atomic root /
epoch snapshot. Require both a changed generation and changed pointer records
for the missing shard; otherwise fail closed. Discard the entire partially
assembled first group, including earlier cache hits, and fetch the fresh group.
Stop on a second missing chunk. Never splice old prefixes with fresh suffixes
or combine shards selected by different catalog snapshots. A cache may return
owned verified immutable contents for an old coherent root; it must never
supply a mutable provider/caller alias.

Costs: another inline catalog manifest, O(chunks) metadata, fresh complete
staging reads and full journal replay; after each collection fence all selected
write objects move into the new epoch. Complete bounded namespace discovery
remains a collection limit. This does not establish bounded replay, incremental
catalog scans, multi-catalog atomicity or horizontal throughput. A later safe
old-epoch reuse design needs a separately modeled publication/protection rule.

Select the policy independently of reclamation's scan and cache budgets:

```rust
ReclaimingPartitionedJournalStore::open_chunked(
    catalog, partitions, route, ReclamationOptions::default(),
    JournalChunkOptions { max_records: 2, ..Default::default() },
)?;
```

```csharp
ReclaimingPartitionedJournalStore.Open(catalog, partitions, route,
    new ReclamationOptions(), new JournalChunkOptions(MaxRecords: 2));
```

```js
await ReclaimingPartitionedJournalStore.open(catalog, partitions, route,
  { pageKeys: 1, cacheBytes: 65536, chunks: { maxRecords: 2 } });
```

Omitting chunk selection retains blob2 writing. Every upgraded reader accepts
both blob2 and chunks2; its selected write policy does not constrain reads.
Append-only blob1/chunks1 namespaces remain incompatible. Older reclamation
readers reject chunks2, so upgrade all readers before selecting this policy.
Epoch u64 exhaustion, whole-namespace admission limits and unknown mutation
outcomes retain the existing reclamation contract.

The independent oracle `experiments/issue-138/build-reclaiming-chunk-vectors.py`
generates `reclaiming-chunks-vectors.json`. Its eighteen vectors cover empty
journals, empty records, UTF-8 byte/count boundaries, BOM/NUL/supplementary
characters and epochs zero, one and u64 max. It imports no production code.
Finite production tests additionally cover retired prefixes, all cache masks,
second retirement, before/after lost replies, current-epoch staging collisions,
shared references, provider aliases, overlapping collectors, exact-generation
ABA guards, mixed legacy writers and following edits. The model's unsafe
old-epoch-reuse and missing-epoch-CAS controls must reproduce lost live chunks;
passing that model alone is not production verification.

Native file examples use `NATIVE_LSM_LAYOUT=reclaiming-chunks` for complete
Rust/C#/Node/Bun/Deno store handoffs. Distributed examples select
`RECLAMATION_CHUNKS=1` on the existing three-member etcd harness. Both selections
retain separate whole-journal controls; neither changes a default.
