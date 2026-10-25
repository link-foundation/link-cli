# Optional immutable journal object cache

`PartitionedJournalStore` can retain verified immutable journal objects locally.
The default budget is zero. Enable a payload budget independently of engine
isolation, validation, history, revision encoding, physical journal and routing.
The catalog and data stores retain their existing atomic journal interfaces.

| Language | Configuration |
| --- | --- |
| Rust | `PartitionedJournalStore::new(catalog, partitions, route)?.with_object_cache(65536)` |
| C# | `new PartitionedJournalStore(catalog, partitions, route, cacheBytes: 65536)` |
| JS/TS | `new PartitionedJournalStore(catalog, partitions, route, { cacheBytes: 65536 })` |

Rust accepts `usize`. C# rejects negative budgets; JS requires a nonnegative safe
integer. Changing the Rust builder's budget drops its previous cache. Append
`--cache` to a partitioned [cluster example](ENGINE-CLUSTERS.md) to select a
65,536-byte payload budget and repeat committed reads in all three languages.

Every read first obtains a fresh, linearizable catalog snapshot. Cache identity
includes the configured partition name, immutable object address and exact object
generation. A hit returns the current catalog generation and an independent
record array. Caller changes cannot alter retained records. A miss reads the
object, verifies its generation and content address, then retains an owned copy
if it fits. Failed reads and invalid objects are never cached. An oversized
object remains readable and does not evict previously retained objects.

The budget counts one byte per object, one per record, and each record's UTF-8
byte length. Thus `[]` costs 1, `[""]` costs 2, `["é"]` costs 4 and `["😀"]`
costs 6. Positive overhead bounds even empty objects and records. Least recently
used objects are evicted until the new object fits; hits promote an object.
Zero allocates no LRU container or retained object copy. C# coordinates cache
metadata under a local gate and performs external I/O outside it; overlapping
C#/JS misses can download the same object twice but account for one retained
copy. Rust's mutable store access serializes fills.

This budget describes retained payload accounting, not total process memory.
Keys, hash tables, LRU nodes, allocator capacity, temporary downloads and
caller-owned copies add memory. Eviction can retain container capacity. Rust
clones string bytes on a hit; C#/JS copy arrays of immutable strings. Each engine
begin still decodes/replays its complete selected journal. A hit saves the data
request, transfer and address verification, not that replay or every allocation.

Catalog comparison and publication always stay fresh. Staging always rereads
and compares complete object contents, including for an already cached object;
it never trusts a cache hit to accept a hash collision. Rejected publication
keeps the original catalog state. A lost publication response still needs a
fresh catalog read to discover the outcome. The cache adds no distributed lock,
new durability guarantee, wire format or automatic mutation retry.

Keep every partition name bound to the same storage namespace and never rewrite
or delete its immutable objects. A hit reuses previously verified contents: it
can succeed during a data-store outage, and cannot detect later out-of-protocol
tampering with that object. The catalog must still be available. FNV addressing
does not authenticate hostile stores. Old readers can retain pointers after
publication; no garbage collector or reclamation proof is supplied. The cache
does not bound persistent old/orphan objects or reduce complete updated-journal
writes. Those storage and scaling requirements remain open.

The Rust implementation reuses [`lru` 0.19.0](https://github.com/jeromefroe/lru-rs),
with default features disabled to use standard hashing. Its ordering is bounded
by this wrapper's payload accounting. C# uses standard `Dictionary` and
`LinkedList`; JS uses standard insertion-ordered `Map`. No native package enters
the universal JS entry point. See the [source review](case-studies/issue-138/RESEARCH.md).

Portable Rust contracts also execute in actual WASM. Rust, C# and JS test fresh
catalogs, ownership, default/zero/oversized budgets, exact UTF-8 costs, empty
objects, partition/generation separation, LRU promotion, failed fetch/repair,
staging checks and rejected/uncertain publication. Each language compares 768
reads against a finite independent ordering/cost oracle. C#/JS additionally
force overlapping misses and verify retained accounting. The real-etcd harness
uses separate catalog/east/west consensus groups and request counters: a second
WASM engine begin reads the catalog once with zero repeated data requests.
Run `bash experiments/issue-138/validate-object-cache.sh` after setting up its
documented validation tools and downloading the etcd binary.
