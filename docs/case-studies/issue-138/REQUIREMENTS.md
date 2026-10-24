# Acceptance inventory

No required sub-issues were returned by GitHub's sub-issue API. Initial issue
comments and all three PR comment/review endpoints were empty.

| ID | Issue requirement | Solution and verification plan | Status |
| --- | --- | --- | --- |
| R1 | Fully test ACID readiness | Failure injection at journal append/flush, data checkpoint, rollback and recovery; anomaly schedules and bounded concurrent histories; report exact persistence assumptions. | In progress: two lost-data regressions reproduced and corrected in Rust; corrupt-log regression reproduced in both native languages. |
| R2 | Configurable concurrency strategies | Exclusive writer admission and optimistic validation; read committed, snapshot isolation and conservative serializable validation. Same transaction/storage API. | In progress |
| R3 | Configurable version control | Existing branches/tags plus optional no-history, bounded and complete immutable revision history; pin active snapshots independently of retention. | Planned |
| R4 | Replaceable distributed synchronization decorators | Common log contract, local-only, asynchronous catch-up and synchronous replica acknowledgements; explicit unknown outcomes, prefix validation and retry tests. | Planned |
| R5 | Study competitor source and best strategies | Primary documentation and source survey covering SQLite, PostgreSQL, RocksDB, Redis, etcd and TiKV; costs/guarantee comparison and reusable component evaluation. | In progress; see RESEARCH.md |
| R6 | Same API / costs only for selected guarantees | Keep existing bare-store cache path; add optional control/history/journal/replication composition with explicit complexity and allocation costs. | Planned |
| R7 | Implement all supported languages | Native Rust and C# library surfaces, portable JS/TS surface and declared backend capabilities; shared wire vectors and scenario tests. | In progress |
| R8 | 100% test coverage, edge cases, unit + integration guarantees | Measure coverage by module/language; add deterministic faults, concurrent schedules, real process/filesystem checks; retain coverage artifacts and disclose uncovered branches. A test count is not coverage. | Unverified |
| R9 | Document each scenario | Runnable cache, durable store and replicated/sharded examples with failure semantics, ownership and persistence settings. | Planned |
| R10 | Horizontal cluster, shared and shared-nothing options | Existing TCP stores/advisory file locks plus replicated logs and independent shard composition. Consensus failover and cross-shard transactions need genuine protocol implementations, never an acknowledgement counter advertised as consensus. | Planned |
| R11 | Competitor comparisons/configuration parity | Map supported settings and list unsupported guarantees explicitly with primary references. | Planned |
| R12 | Deep case study / collect data / plan + execute one PR | This directory, preserved original issue, plan, requirements, source research and test evidence; PR 139 with positive closing reference. | In progress |

An exhaustive survey of all current/future successful DBMS is not a finite
verification target. The research inventory is explicit; unsupported algorithms
and unverified guarantees remain unfinished rather than being implied by tests
for a different design.
