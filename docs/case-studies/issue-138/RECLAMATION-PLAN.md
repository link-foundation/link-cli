# Selected epoch-fenced object reclamation

Objective: implement and verify explicit reclamation across Rust, C# and JS/WASM,
preserving owned atomic reads and existing append-only modes. This batch covers
part of R1/R4/R6/R7/R8/R9/R10/R11/R12; the wider issue remains unfinished.

- [x] Read the complete issue and refresh all four comment/review endpoints.
- [x] Preserve actual missing-API failures against maintenance head 44e0fa8.
- [x] Model bounded concurrent publishers/collectors and coherent reader retry,
      including negative controls without epoch fencing/epoch keys/group refresh.
- [x] Add separately selected maintenance-capable store/options/result APIs.
- [x] Validate namespaces, complete snapshots and every option before mutation;
      require empty data namespaces before initialization of an empty catalog.
- [x] Add 65 independent address/epoch/pointer vectors consumed by all targets.
- [x] Fence publication and collection; preserve read-only dependencies and
      complete staging comparisons; never replay unknown mutations.
- [x] Validate every complete catalog/data snapshot before deletion; protect
      aliased live keys; compare exact object generations; bound u64 exhaustion.
- [x] Return owned coherent groups with one whole refresh/retry; reject reused
      generations, unchanged missing pointers, damaged data and transport errors.
- [x] Add independently selected zero-default bounded verified-object caching.
- [x] Exercise deterministic concurrency seams, resurrection, ABA, invalid
      providers/bounds and before/after loss at each mutation phase in all targets.
- [x] Reproduce JS composite errors that hid earlier acknowledged effects;
      preserve unknown outcome and original cause after later request failure.
- [x] Run initial real three-member native Rust/C#/Node/Bun/Deno handoffs,
      retained history/names/allocator holes, stale owners, reader refresh,
      response loss, cache outage, leader loss and majority loss.
- [x] Run full native/C# Debug+Release/actual-WASM/JS coverage suites and strict
      checks, rebuild the final WASM package and repeat the real-etcd integration.
- [x] Publish complete coverage denominators and curated before/after evidence.
- [x] Review all changes; commit and push only the prepared branch, inspect fresh
      exact-head CI, preserve failed logs and correct any actual failures.
- [ ] Continue the wider unfinished R1–R12 requirements in this same draft PR.

Initial focused suites pass 25 native Rust, 39 C# and 145 JS tests; all selected
JS modules measure 100% lines/branches/functions. Shared fixtures loop within
native tests, so counts cannot be compared directly. These focused checks do
not replace whole-project validation. The initial handoff used the already
built mirrored-root WASM engine package; final validation must rebuild it.

The first actual-etcd attempt demonstrated the documented JS close contract:
close joins and repeats an already failed commit's terminal promise. The example
now explicitly verifies that expected stale conflict on close; production retry
or cleanup behavior is unchanged. The corrected finite harness passes all five
initial runtime handoffs and three more after leader loss, ten before/after
mutation-loss cases, coherent reader refresh, cache outage and majority loss.

Current limitations remain explicit: namespace ownership is an application
contract; providers must never reuse generation/epoch identity after restore;
borrowed external roots are unsupported; collection needs complete bounded
listings; FNV does not authenticate hostile stores; epoch advancement contends
with writers; process exits/network partitions do not prove device power loss.

The complete sequential driver passes from 2026-10-26T21:49:03Z through
22:17:42Z: 1,057 native Rust plus six doctests, 656 actual WASM, 2,307 C# per
configuration, 518 JS and 221 repository checks. Bun/Deno each pass all159
selected cases. JS enforces complete 100% line/branch/function coverage. A fresh
bounded WASM package passes all eight native/Node/Bun/Deno real-etcd handoffs,
ten response-loss cases, coherent reader refresh and leader/majority loss.
reclamation.txt retains complete denominators, finite model counterexamples and
limits; reclamation-before.txt retains actual missing-API and regression failures.
Fresh exact-head CI follows publication; every wider unmet criterion stays active.

All nine workflows pass on published source `419f7c7`, created after that commit
at 2026-10-26T22:19:35Z. Both native Linux/macOS/Windows matrices and actual WASM
pass. `reclamation-ci.txt` retains exact run/job URLs and timestamps. The next
test-only contract audit has its own plan and requires new exact-head CI after
publication; broader acceptance criteria remain unfinished.
