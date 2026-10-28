# Selected author fencing verification

The [plan](AUTHOR-FENCING-PLAN.md) advances R4/R6/R7/R9 without completing the
wider requirements. The [protocol](../../protocol/author-fence-v1.md) specifies
canonical state, ownership, exact retry, admission, cost and provider assumptions.

Missing public APIs fail first in JS, native Rust and C#. The independent Python
model enumerates all six read/CAS interleavings of a takeover and old-owner
publication and supplies 22 shared canonical/admission fixtures. The first model
draft incorrectly allowed an old owner to bind a newly read epoch; correcting
the model to check the bound claim makes the refused post-takeover read explicit.
That was a model correction, not a claimed production regression.

Focused local checks pass seven Rust contracts, eight C# cases and nine tests
on each of Node, Bun and Deno. Node's new
production module covers every measured line, branch and function. The shared
vectors execute in all three implementations. Strict Clippy, type declarations,
formatting and small repository/release gates are recorded in the retained
[results](../../../dev/log/issues/138/pulls/139/author-fencing.txt). Complete suites remain in CI per the latest
maintainer feedback. These controls verify:

- Explicit predecessor comparison, original active-token retry and refusal of
  a superseded token, including before/after lost takeover replies.
- Same-base competing claims and takeover injected inside the final guarded
  data CAS, with no partial write of either data key.
- Ordinary causal batches resolve an earlier decided batch through the new
  guard, continue the author counter and allocate above the entire abandoned
  old block. The stale guard refuses and preserves data/receipt bytes.
- Read-only data CAS preserves the fence generation; malformed or over-budget
  requests cannot mutate provider state. Owned descriptors/caller arguments
  cannot alter a pending guarded publication.

Run `bash experiments/issue-138/run-author-fence-checks.sh` for focused native,
C# and strict JS checks. The independent model regenerates the shared vectors.
The linked Rust tests also run on actual WASM in CI. The existing bounded
three-member etcd harness now invokes
[author-fence-consensus.mjs](../../../examples/native-lsm-js/author-fence-consensus.mjs),
using actual-WASM staging, public Rust/C# claim CLIs, stale prepared publication,
burned reservation suffixes and continued author counters. **That new real-etcd
scenario, full actual-WASM execution and fresh platform CI remain pending.**

The guard requires all writers for one author to share a reserved ownership
key and uniquely owned active token. Its coherent provider includes the fence
comparison in the same data CAS. It does not prevent bypass writes or multiple
owners sharing the same claim. Explicit takeover is caller-authorized; TTL,
automatic leadership, permissions, membership/history reclamation, whole-project
100% and the broader acceptance inventory remain unfinished.
