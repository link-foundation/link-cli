# Explicit immutable journal reclamation

Select `ReclaimingPartitionedJournalStore` when a coordinated deployment needs
to remove retired or abandoned immutable journal objects. It implements the same
atomic journal API used by `ClusterEngine`, with an explicit open operation and
an explicit collector. Rust, C# and JS/TS expose the same protocol; JavaScript
workspaces execute the Rust engine through WASM. The providers must implement
the optional [namespace maintenance contract](ENGINE-MAINTENANCE.md).

Use new, dedicated caller-owned catalog and data namespaces. Opening an empty
catalog first scans every data namespace and requires them all to be empty.
One conditional transaction initializes its epoch; a subsequent complete read
validates the result. Existing initialized catalogs are validated before use.
A legacy `blob1` catalog, malformed record or nonempty uninitialized namespace
rejects without conversion. Keep each partition name bound to its original
namespace. Scanning and CAS cannot acquire ownership against an uncoordinated
foreign writer.

```js
import { ClusterEngine, EtcdJournalStore, ReclaimingPartitionedJournalStore }
  from '@link-foundation/link-cli-client';
const provider = (name) => new EtcdJournalStore(endpoints, {
  prefix: `/my-owned-new-deployment/${name}/`,
});
const journals = await ReclaimingPartitionedJournalStore.open(
  provider('catalog'), { east: provider('east'), west: provider('west') },
  (shard) => shard === 'a' ? 'east' : 'west',
  { pageKeys: 128, maxKeys: 4096, maxBytes: 8388608, cacheBytes: 0 },
);
const engine = new ClusterEngine(binding, journals, {
  history: 'all', requireDurableFlush: true, journalCheckpointInterval: 1,
});
// Begin/edit/commit coordinated work through the existing engine API.
const result = await journals.collectGarbage();
// null means another collector won the epoch comparison; no deletion ran.
```

Rust uses `ReclaimingPartitionedJournalStore::open`, `ReclamationOptions`, and
`collect_garbage`. C# uses `Open`, `ReclamationOptions` and `CollectGarbage`.
Providers remain caller-owned in C#/JS; the Rust generic store owns the provider
values passed to it. No background thread, timer, scan or deletion is scheduled.
Existing `PartitionedJournalStore` and `blob1` objects retain their append-only
behavior. The selected JS codec/collector modules load only when `open` is called.

## Publication and collection

The catalog reserves `__link_gc_epoch`, containing `['gc1', canonical-u64]`.
Its modification generation is compared in every publication alongside all
selected shard generations. This consumes one existing transaction slot, so the
selected decorator accepts 1..31 distinct shards. Existing stores keep 32.

An atomic read captures the selected pointers and epoch together. A publisher
first validates every update/route, rejects stale caller generations before
staging, writes immutable data under that observed epoch, then attempts one
catalog CAS. Null updates still compare read-only shard dependencies. Epoch
competition rejects publication; it never silently restages or retries.

Object names are `blob2-<16 lowercase epoch hex>-<16 lowercase FNV hash hex>`.
The hash frames each record as its UTF-8 decimal byte length, a colon, and its
bytes, under `blob2|`. Empty, NUL, BOM, Unicode and newline records are preserved.
The key's epoch prevents content reuse across collection fences. A pointer is
`['blob2', partition, key, positive-object-modification-generation]`.
Hash collisions are checked by comparing complete staging contents; FNV is not
an authentication mechanism. Reads verify the generation and content address.
Independent shared [wire vectors](protocol/reclamation-v1.json) define canonical
spelling and boundary cases, including the final u64 epoch.

A collector advances the epoch once by conditional update and records its own
fence. Epoch exhaustion fails before mutation. It then scans the complete catalog
and every data namespace, enforcing key/byte bounds and validating every pointer
and protected object's listed generation before its first deletion. Catalog and
data listings need independent consistent revisions. The catalog snapshot must
be at least the collector's fence; pointers cannot reference a future epoch.
Concurrent publishers may already have staged newer data objects, which are kept.

Only objects absent from the protected roots and strictly older than this
collector's own fence are candidates. Every cut compares the exact listed
non-reusable modification generation. A false comparison skips that object.
Identically named live objects are conservatively protected across partition
names, including providers that alias a physical namespace. This can retain
extra objects; it cannot authorize deletion of an aliased live object.

The safety argument uses three ordering facts: advancing the epoch fences every
older publisher before root discovery; later publishers cannot introduce an
older-epoch key; and conditional deletion cannot remove a replacement with a
new generation. Overlapping collectors each use their own fence. A newer epoch
observed while scanning never expands an older collector's deletion permission.

## Readers, recovery and costs

Reads return owned complete journals. If a captured object has been retired,
one complete catalog refresh and one complete group retry are permitted only
when both its catalog generation and pointer changed. All first-attempt values,
including cache hits, are discarded. An unchanged missing pointer, generation
reuse, altered data/hash, malformed response or transport error fails. A second
retirement ends the bounded retry. Readers never combine shards from two roots.

Owned engine workspaces and verified cache values survive physical retirement;
their later publication still compares fresh catalog generations. Retained
revision/branch history lives inside the current journals and survives collection.
No borrowed physical roots are exposed. A future API exposing such roots needs
an explicit durable retention protocol before they can be collected safely.

Unknown initialization, staging, publication, fence or deletion outcomes end
the attempt without mutation replay. Reopen/read fresh state to discover the
authoritative outcome. A failed collector may already have acknowledged cuts;
it does not roll them back. Its result counts only acknowledged deletions.
Providers must preserve non-reusable generations and epoch identity through
restore; resetting either requires a fresh prefix and explicit migration.
JavaScript composite mutation errors conservatively report `outcome: 'unknown'`
and retain the original `cause`: a later unsent or refused provider request
cannot erase earlier successful steps. Caller input validation precedes I/O.

Collection costs one epoch CAS, complete bounded scans, an in-memory protected
root set and one conditional request per candidate. Limits apply separately to
each namespace, using maintenance byte accounting; catalog scans include pointer
payloads and data scans are keys-only. Defaults are 4096 keys, 8 MiB and 128 keys
per page; page size is 1..256. Exceeding a bound aborts before any cut. Raising
bounds is an explicit memory/network cost. Staging, full-journal publication and
workspace replay retain their existing costs. Epoch contention can abort writers.

The independently selected cache defaults to zero. Positive budgets account
UTF-8 record bytes plus one byte per record and one per object, with bounded LRU
eviction. Container overhead is additional. Cached reads still require a fresh
catalog; data-group outages can be tolerated only for already verified current
objects. Out-of-protocol mutation cannot be detected by an existing cache hit.

## Runnable native and WASM handoffs

Select an independent [bounded chunk policy](protocol/reclaiming-chunks.md) to
reuse unchanged prefixes between collection fences. Rust supplies
`open_chunked(..., ReclamationOptions, JournalChunkOptions)`, C# supplies the
five-argument `Open(..., options, chunks)` overload, and JS supplies
`{ chunks: { maxRecords: 2 } }` in its existing options. Cache and scan limits
remain separate. Omitted chunk selection keeps whole-journal blob2 writes.
Upgraded readers accept both formats, including live empty chunk journals.
Every referenced object is protected during collection. After a fence, writers
restage the complete journal under the captured new epoch. Full staging reads,
replay and bounded complete namespace scans remain explicit costs.

Each example appends one coordinated revision to shards `a`/`b`, publishes
retained-history checkpoints, collects retired objects and checks all retained
names, a deleted allocator reservation and an owned stale-workspace conflict.
Run sequentially against the same owned prefix to verify wire interoperability:

```sh
cargo run --manifest-path rust/Cargo.toml --features etcd \
  --example journal_reclamation -- http://localhost:2379 /new-reclamation-demo/
dotnet run --project examples/journal-reclamation-csharp -- \
  http://localhost:2379 /new-reclamation-demo/
# Build rust/wasm with wasm-pack --target web --out-dir ../../js/pkg first.
node js/client/examples/journal-reclamation.mjs \
  http://localhost:2379 /new-reclamation-demo/
```

The JS example also runs with Bun and `deno run --allow-net --allow-read
--allow-env`. `CLINK_WASM` selects another built module directory. The example
binaries and integration harness select the chunk layout with the explicit
`RECLAMATION_CHUNKS=1` environment variable; use a new dedicated prefix for a
separate demonstration. Each handoff checks the exact number of retired
references from the previous catalog snapshot, including retained-history chunks.
Handoffs are bounded to 32 revisions. The finite three-member integration harness
also tests response loss before/after every mutation phase, whole-group reader
refresh, cache/data outages, leader loss and majority loss. Etcd supplies consensus
and its declared durable storage; process/network fault probes do not establish
physical device power-loss behavior. See the [case-study plan](case-studies/issue-138/RECLAMATION-PLAN.md)
for current implementation and verification evidence.
