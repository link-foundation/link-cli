# Coordinated network engines

Use an existing consensus system rather than introducing an unverified election
protocol. etcd's default Range is linearizable; a Txn reads multiple keys at one
revision and atomically checks modification revisions before writing several
journals. Modification revisions fence deletion/recreation (ABA). Primary sources:
[API](https://etcd.io/docs/v3.6/learning/api/),
[JSON gateway](https://etcd.io/docs/v3.6/dev-guide/api_grpc_gateway/),
[guarantees](https://etcd.io/docs/v3.6/learning/api_guarantees/).

Each transaction loads selected shard journals into independent private native
engines. Its normal links/names API operates on those workspaces. Commit validates
local constraints and publishes all changed journals with a single coordinator
CAS, including comparisons for read-only selected shards. Any changed generation
aborts the entire group. No cached engine serves later transactions. A response
lost after CAS is unknown; reopen/read discovers the committed state. Never
replay a mutation automatically after a transport error.

This first backend stores full journals in coordinator keys. It has explicit
request/response limits and is intended for small datasets. Processing and
namespace routing can spread across workers; one etcd group still replicates
all bytes and limits aggregate write throughput. Separate shard clusters and
cross-cluster recovery require an additional durable protocol and are not
implied by this backend. All cluster operations use conservative serializable
snapshot validation; unsupported read-committed/exclusive admission is rejected.
Shard addresses are namespaced, not globally interchangeable numeric addresses.

- [x] Generic Rust/C#/JS atomic journal store and coordinated transaction APIs.
- [x] Shared tests for cross-shard all-or-nothing publication, read-only dependency
      conflicts, independent shard concurrency, rollback and unknown outcomes.
- [x] HTTP(S) etcd adapters with authentication headers, bounds and safe read failover.
- [x] Actual three-member etcd integration, multiple language writers, leader
      termination/re-election, lost-response recovery and majority loss.
- [x] CI integration and examples, costs, deployment and ownership documentation.
- [ ] Independently stored shard data / multiple consensus groups and recovery.


## Observed protocol regression

The first actual-cluster conflict test failed because a rejected etcd Txn has
an omitted `succeeded` field (ProtoJSON's default false), while mocks initially
returned explicit false. Adapters now require a valid positive revision header
and accept an omitted/false comparison result. Both forms are distinct from
missing/corrupt responses, which remain indeterminate. The first failing log
and passing interop evidence are retained under `dev/log/issues/138/pulls/139`.

The real three-member run uses etcd v3.7.3. Its Linux amd64 release archive SHA256
is `5dfa9cc3b3ce3b4b491fbcc2057d3daa166bcc6676f743fcd20f8e5760cb9775`,
verified against the GitHub release asset digest and enforced by CI before use.
The bounded integration runner defaults to three 100 MiB Go memory targets and
16 MiB backend quotas; these are test settings, not production sizing guidance.
