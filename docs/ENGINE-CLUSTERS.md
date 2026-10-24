# Coordinated network engines

`ClusterEngine` composes the normal links and names API with an atomic journal
store. Independent application processes select named shards at transaction
start, work privately, and publish their changed journals in one atomic compare
and exchange. Every selected shard's generation is checked, including read-only
shards. This gives conservative serializable transactions within a selected
group. Select every shard on which an application invariant depends. References
are local to a shard; application-level cross-shard references include its name.

Rust implements `LinksStorage<T>` and `NamedTypeLinks`, C# implements
`INamedTypesLinks<T>`, and JS returns a `LinksClient` per selected shard. Commit
belongs to the containing group. The bare cache and local engine APIs retain
no network dependency. Network sessions currently support optimistic admission
and serializable validation; requests for other policies fail explicitly.

## Backends and costs

The replaceable `AtomicJournalStore` / `IAtomicJournalStore` contract requires
linearizable grouped reads, never-reused modification generations, atomic
comparison of every selected key, and atomic publication of all changed keys.
Zero denotes an absent key. A custom store must not silently weaken this contract.
The supplied `EtcdJournalStore` uses etcd's v3 HTTP JSON gateway. One Txn reads the
group; one Txn validates and publishes. Read failures try each configured endpoint
once. A mutation is sent once. No follower-local or stale cached reads are served.

This backend stores full journals in etcd keys. Transaction start transfers and
replays each selected shard's complete journal; commit transfers the complete
updated journals. Defaults bound requests to 1 MiB, responses to 8 MiB, selected
shards to 32, and names to 128 ASCII letters/digits/dots/underscores/dashes. Tune
byte bounds together with etcd's server request/quota limits. These limits are
checked in addition to engine journal integrity, allocator and constraint checks.
This is suitable for small datasets and coordination; it is not a high-throughput
bulk storage design. One consensus group replicates every journal and limits
aggregate write throughput. More workers and separate shard namespaces spread
processing; independent consensus groups and independently stored shard data
require a further protocol. Those broader scale-out options remain unfinished.

## Configuration and ownership

Rust enables HTTP/TLS dependencies only with Cargo feature `etcd`.
`EtcdJournalStore::with_agent` accepts a configured `ureq::Agent`; `headers`
accepts authorization headers. The default agent uses a finite ten-second global
deadline, HTTPS root validation and no redirects. Configure private trust roots,
client certificates and proxies through the agent. Cluster engines retain their
store through an `Arc<Mutex<_>>`; transactions own private engine handles.

C# accepts an optional `HttpClient`, enabling custom TLS, authentication headers
and proxies. The caller owns an injected client and the journal store; disposing
the store disposes only a client it created. `RequestTimeout` bounds the complete
request and streamed response (default ten seconds). Default redirects are disabled.
Cluster transactions are single-threaded and must be disposed. A shared custom
store must make its own operations thread-safe.

JS accepts `fetch`, `headers`, `timeoutMs`, `maxRequestBytes` and `maxResponseBytes`.
The universal entry point imports no Node APIs; the initialized WASM binding is
supplied by the caller. `await using` or `try/finally` closes transactions and frees
all workspaces. Close waits for an in-flight commit and propagates its outcome.
The store remains caller-owned. Browser deployments need appropriate CORS support
from a trusted gateway and must protect credentials.

## Failures and durability

A rejected comparison is a known conflict and publishes none of the group. Each
commit attempt ends its transaction. A lost response, HTTP error or malformed
response after sending commit can have an unknown outcome. Open a new transaction
to discover state; never automatically replay a mutation after an I/O error.
Ordinary uncommitted disposal publishes nothing. An applied commit can survive a
lost response; disposal cannot roll it back. Local validation errors occur before
any coordinator mutation. The local workspaces use volatile journals; the shared
store is the durability boundary.

`requireDurableFlush` checks the backend's declared capability. For etcd this
requires the operator to retain default durable fsync settings and reliable
storage. The adapter cannot verify disks, power-loss protection or a dishonest
custom adapter. With a healthy majority, etcd handles leader election and
membership; without a majority, fresh linearizable reads and commits fail. Use
etcd's own supported membership and backup/recovery tools, keeping engine keys
intact. Never expire/delete keys or roll back the coordinator while applications
are using its generations.

## Reproduce verification

The [Rust and C# examples](../examples/cluster-csharp/README.md) and
[JS example](../js/client/examples/cluster.mjs) use the same journal representation.
The bounded [integration runner](../experiments/issue-138/etcd-cluster.mjs) launches
three actual etcd members, uses JS and optional native writers, injects a lost
commit response, kills the elected leader, verifies a surviving-majority commit,
and then checks that majority loss refuses a fresh read. All children are killed
and reaped in `finally`. The JS CI conformance job runs this experiment.

Primary references: [etcd API](https://etcd.io/docs/v3.6/learning/api/),
[JSON gateway](https://etcd.io/docs/v3.6/dev-guide/api_grpc_gateway/),
[guarantees](https://etcd.io/docs/v3.6/learning/api_guarantees/),
[ProtoJSON defaults](https://protobuf.dev/programming-guides/json/).
The JSON protocol omits default `false` fields; adapters distinguish a valid
rejected comparison from an invalid response using the response header.
