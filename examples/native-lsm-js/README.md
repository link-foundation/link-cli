# Native LSM cluster consumers and file interchange

Build the current WASM package with `wasm-pack build --target web --out-dir
../../js/pkg rust/wasm`, then run from the repository root:

```sh
CLINK_WASM=js/pkg node examples/native-lsm-js/cluster.mjs
CLINK_WASM=js/pkg bun examples/native-lsm-js/cluster.mjs
CLINK_WASM=js/pkg deno run --allow-all examples/native-lsm-js/cluster.mjs
```

The example uses the actual WASM links engine and actual native files in temporary
directories. It checks five finite compositions: a direct journal store, an
atomic catalog over two data stores, selected bounded journal chunks, and
epoch-fenced reclamation with whole journals or selected chunks. Each runs three coordinated revisions with retained-history checkpoint
journals, rejects a stale group after a competing commit, compacts, closes and
reopens the physical stores, then reads the original link names. The reclamation
case also deletes obsolete immutable objects before reopening.

The file store is imported separately from
`@link-foundation/link-cli-client/engine-file-lsm-store`. Create its dedicated
parent directory first. The example closes every transaction before its stores
and removes temporary directories on completion. It does not require an external
coordinator or SQLite. See the [native ownership, finite bounds and platform
contract](../../docs/protocol/native-lsm-store.md#javascript-native-directory-selection).

C# supplies the matching [native facade and consumers](../cluster-csharp/README.md).
Build its example and the matching Rust facade before running the finite
complete-store handoff matrix:

```sh
dotnet build examples/cluster-csharp --configuration Release
cargo build --manifest-path rust/Cargo.toml --no-default-features --features native-pages,physical-lsm --example native_lsm
python3 examples/native-lsm-js/run-interop.py
NATIVE_LSM_LAYOUT=chunks python3 examples/native-lsm-js/run-interop.py
NATIVE_LSM_LAYOUT=reclaiming-chunks python3 examples/native-lsm-js/run-interop.py
```

Rust, C#, Node, Bun and Deno each write plain and fully mirrored stores. Every runtime
reads every writer's complete directory without changing file hashes, publishes
another guarded group, flushes and compacts, then returns ownership to the
original writer for another byte-preserving read. All 50 handoffs check exact
generations, tombstones, live empty journals and Unicode/NUL/BOM payloads.
Each subprocess exits before its successor acquires ownership. The script uses
temporary files and removes them after all processes exit. It does not load WASM
or SQL for these journal-provider checks.

The Rust executable defaults to `rust/target/debug/examples/native_lsm`; set
`RUST_NATIVE_LSM_EXAMPLE` when using another build profile. For a deliberately
selected runtime subset, set `NATIVE_LSM_RUNTIMES` to comma-separated names from
`rust,csharp,node,bun,deno`. The default checks all five. Native Rust cluster
consumers have their own [runnable instructions](../../rust/examples/native_lsm/README.md).

The second matrix runs another 50 handoffs over chunk manifests and immutable
objects in separate native catalog/data directories. Writers select two records
per chunk; advancing readers select three, verifying read compatibility across
write policies. Live empty journals, Unicode/BOM/NUL, stale groups, owned prior
snapshots and byte-preserving read-only reopening remain checked. This matrix
does not select deletion of chunk objects. See the [chunk contract and costs](../../docs/protocol/chunked-journal.md).

The third matrix runs 50 handoffs with epoch-fenced chunks2 manifests. Each
advancing reader collects after acquiring ownership, publishes under the new
epoch, rejects its old owned generation, then collects exactly the three retired
seed chunks. Read-only reopening preserves every physical file hash. The actual
WASM cluster consumer separately verifies retained checkpoint history. See the
[reclamation contract and restaging costs](../../docs/protocol/reclaiming-chunks.md).

The separately selected [causal Merkle example](../../docs/ENGINE-CAUSAL-MERKLE.md)
exchanges bounded proof messages between independently owned physical stores.
Build Rust with `native-pages,physical-lsm,causal-merkle`, build the C# cluster
example, then run `python3 examples/native-lsm-js/run-merkle-interop.py` from
the repository root. It includes finite before/after publication process exits,
explicit reaped-owner recovery, read/retry hash preservation and following edits.

The physical pull example also carries continuations with four-node/one-leaf
budgets and at most forty attempts, so the concurrent first leaf cannot starve
the second address during actual wire/file exchange.

The [local batch example](../../docs/ENGINE-CAUSAL-BATCHES.md) publishes two row
edits and their retry receipt through one coherent two-key CAS. Build Rust with
`native-pages,physical-lsm,causal-batches` and the C# cluster example, then run:

```sh
NATIVE_LSM_LAYOUT=batch python3 examples/native-lsm-js/run-interop.py
NATIVE_CAUSAL_LAYOUT=batch python3 examples/native-lsm-js/run-causal-faults.py
```

The matrices check both plain and mirrored directories, exact owned snapshots,
Unicode names, retained deletions, whole-batch retry after later edits, an empty
batch and process exits immediately before/after publication. Every child is
reaped before explicit stale-marker quarantine. Local batch atomicity does not
make independently transferred remote leaves atomic as a group.

Run `node examples/native-lsm-js/projection-cache.mjs` from the repository root
for an owned read-only causal cache over a temporary SQL-free physical store.
Bun runs the same file; Deno uses `deno run --allow-read --allow-write --allow-env` followed
by that path. The example retains concurrent live/deleted evidence, requires an
explicit visibility policy, resolves it through an observed batch write and
refreshes the cache. The old view remains unchanged. It closes the store and
removes its own temporary directory. See the
[projection guarantees and costs](../../docs/ENGINE-CAUSAL-PROJECTION.md).
The example selects required durable flush and therefore needs a platform where
the physical provider advertises that capability.

The selected permanent address allocator uses the same physical stores without
requiring a causal journal or an SQL layer. After building Rust's `native_lsm`
with `native-pages,physical-lsm,address-reservations` and the C# cluster example,
run `NATIVE_LSM_LAYOUT=allocation python3 examples/native-lsm-js/run-interop.py`
for the full five-runtime matrix. The `allocation-interop.mjs` recipe retains
exact author/token/count grants after following allocations and rejects changed
domains or token collisions. `NATIVE_CAUSAL_LAYOUT=allocation python3
examples/native-lsm-js/run-causal-faults.py` exercises finite process exits with
explicit marker recovery after reaping. Local focused verification uses
`python3 experiments/issue-138/address-allocation-native.py`.

The [allocation protocol](../../docs/protocol/address-reservations-v1.md)
explains immutable domain migration, ordinary causal workspace selection,
permanent abandoned gaps, finite receipts and the shared coordinator cost.
The real-etcd harness also runs `allocation-consensus.mjs` with actual WASM
creates and native Rust/C# grants. Author lineage fencing and causal history
reclamation remain separate requirements.
