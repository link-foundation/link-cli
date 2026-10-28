# Selected permanent address allocation verification

This advances the global fresh-identity portion of R4 and the composable API,
algorithm, verification and language parity requirements R6/R7/R9/R11. It does
not complete their wider acceptance criteria. The [plan](ADDRESS-ALLOCATION-PLAN.md)
and [protocol](../../protocol/address-reservations-v1.md) record invariants,
provider assumptions, finite bounds, ordinary session usage and explicit costs.

Before production implementation, the independent Python oracle enumerated
5,832 finite three-request schedules and all six complete two-writer read/CAS
interleavings, and generated 53 canonical/admission fixtures. The selected JS
API was absent; the first new test run failed with `ERR_MODULE_NOT_FOUND`.
This is a newly implemented capability, not a claimed baseline allocator bug.
The oracle imports no production implementation.

Focused local checks pass:

- Nine Node tests pass with 100% production lines, branches and functions in
  `engine-address-allocator.js`; the same nine pass in Bun and Deno.
- A review regression first catches a truthy non-boolean CAS reply being
  acknowledged by the draft. Only explicit true now acknowledges publication;
  false reports a known conflict and other replies retain unknown outcome.
- A canonical-field control rejects all five trailing line terminators in keys,
  domains, author/token identities, counts and generations before publication.
- Six Rust contracts and nine C# cases pass, including all shared fixtures.
  Typed native APIs preserve u64 values directly; JS uses canonical decimal
  strings. Returned grants and snapshots cannot mutate persisted receipts.
- Before/after-publication failures have one CAS attempt; matching retries
  resolve the original grant after following writes. Same-base competing
  attempts conflict and the explicit original retry gets a disjoint range.
- Different authors/counts cannot reuse a token. Changed domains, corruption,
  byte/entry limits, exhausted addresses and maximum generation refuse new
  grants without publication. Retained retries still fit after admission fills.
- A complete u64-width grant and a domain ending at maximum u64 exhaust without
  cycling. Unused addresses are burned rather than silently recycled.
- Two real plain/mirrored handoffs run Rust write, C# read, Node following grant
  and Rust/C# reopen. Read and retained-retry physical hashes stay unchanged.
- Four real Node process exits run before/after publication in both layouts.
  Each child is reaped, blocked opens preserve evidence, and explicit marker
  recovery permits exact retained retry and following disjoint grants.
- Strict focused Clippy, Rust formatting, selected C# formatting, TypeScript,
  source-size/release checks, ShellCheck, 221 repository checks and 36 release
  helper checks pass. The first repository gate detected a local root log
  directory; moving those preserved logs into the existing experiment evidence
  directory restored the gate without weakening it.

Reproduce the bounded local checks from the repository root:

```sh
bash experiments/issue-138/run-address-allocation-checks.sh
python3 experiments/issue-138/address-allocation-native.py
NATIVE_LSM_RUNTIMES=node NATIVE_CAUSAL_LAYOUT=allocation python3 examples/native-lsm-js/run-causal-faults.py
```

Build the native Rust example with
`native-pages,physical-lsm,address-reservations` and the Release C# cluster
example before the focused handoffs. Complete suites remain in CI per the
[latest maintainer feedback](https://github.com/link-foundation/link-cli/pull/139#issuecomment-6467849231).
The linked actual-WASM Rust tests, complete five-runtime physical/exit matrices,
strict whole-JS coverage and real three-member etcd scenario are wired into CI;
**their new execution results are pending**. The real-etcd scenario also uses
ordinary actual-WASM creates from part of a grant, proves its unused suffix
stays abandoned, drops a real allocation response, executes the public native
reservation CLIs and verifies exact retries after later grants.

Retained local results are in [address-allocation.txt](../../../dev/log/issues/138/pulls/139/address-allocation.txt).

A grant replays/copies/transfers the complete bounded retained journal and
requires one shared coordinator CAS. Local creates can amortize that call
across the block; larger blocks can abandon larger gaps. Receipts are permanent
and finite; admission eventually refuses new grants. No author lineage lease,
old-owner fence, permission model, membership/reclamation protocol, catalog
partitioning or hardware power-loss guarantee is implied. Existing manually
reserved sessions and bare stores retain their path and defaults.
