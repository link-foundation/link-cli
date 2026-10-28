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

The later [early-admission audit](AUTHOR-FENCE-ADMISSION-PLAN.md) reproduces a
real protocol mismatch in JS/C#: a 100-character non-ASCII record exceeds the
256-byte pre-parse budget but still reaches JSON parsing. Focused parser-call
and exception-type controls fail first. All three ports now reject non-ASCII
records before parsing; every canonical accepted record remains compatible.
The independent model now supplies 24 fixtures. Seven Rust, nine C# and ten
tests on each JS runtime pass; the JS module retains strict 100% measured
lines/branches/functions. The [admission evidence](../../../dev/log/issues/138/pulls/139/author-fence-admission.txt)
retains before/after output and digests. Complete fresh CI remains required.

The later [C# coverage audit](AUTHOR-FENCE-COVERAGE-PLAN.md) adds six public
contract cases without production changes. All fifteen focused cases now pass;
every author-guard source line and raw compiled branch executes in Debug
(87/87 lines, 124/124 branches) and Release (63/63, 124/124). CI applies that
strict component gate to the complete platform reports. The retained
[coverage evidence](../../../dev/log/issues/138/pulls/139/author-fence-coverage.txt)
includes the initial rejection and complete focused-report denominators.
Whole-project coverage remains below the acceptance target.

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
burned reservation suffixes and continued author counters. Both real-etcd runs
pass on `aae1422`, using both native claim ports and actual-WASM staging. That
head also passes 819 actual-WASM tests, 717 JS tests and 2,630 C# tests per
platform. Complete C# artifacts retain their digest-verified denominators:
Linux 8,470/8,685 lines and 7,202/7,709 branches; Windows 8,456/8,685 and
7,182/7,709; macOS 8,465/8,685 and 7,197/7,709. Later admission, physical
handoff and coverage changes still require fresh exact-head CI. The
[initial CI receipt](../../../dev/log/issues/138/pulls/139/author-fence-ci.txt)
retains raw log line references, artifact digests, the passing Rust Windows
control and the separate dependency freshness failure.

The [physical handoff plan](AUTHOR-FENCE-PHYSICAL-PLAN.md) extends verification
through actual SQL-free native stores. Its missing-recipe baseline fails before
the optional Rust subcommand exists. Two focused Rust -> C# -> Node -> Rust/C#
handoffs now pass for plain and fully mirrored stores. They check complete
retained claim snapshots, active-token retries, superseded reads/publications/
retries, two-key updates and unchanged fence generations during data CAS.
Every physical file hash is preserved by read-only reopening, including refused
operations and read-only CAS. The fifty-handoff five-runtime matrix is wired to
CI and remains pending. These clean-close controls are separate from abrupt
exit and hardware power-loss verification.

The subsequent [process-exit plan](AUTHOR-FENCE-PROCESS-PLAN.md) extends the
same recipes without production changes. Twelve focused exits pass across
Node, Rust and C#, checking each runtime separately in both mirror modes and
at both takeover boundaries. The original predecessor/token resolves either
outcome, a committed claim retains the prior data, and a third owner fences
both predecessors before publishing. Retained durable retries, refused opens
and read-only recovery preserve complete physical hashes. Every child is
reaped before explicit marker quarantine. The
[process evidence](../../../dev/log/issues/138/pulls/139/author-fence-process.txt)
retains the missing-recipe baseline, focused output, native builds and gates.
The full 20-exit/100-check five-runtime matrix is wired to CI and remains
pending; these controls do not certify hardware power loss.

The guard requires all writers for one author to share a reserved ownership
key and uniquely owned active token. Its coherent provider includes the fence
comparison in the same data CAS. It does not prevent bypass writes or multiple
owners sharing the same claim. Explicit takeover is caller-authorized; TTL,
automatic leadership, permissions, membership/history reclamation, whole-project
100% and the broader acceptance inventory remain unfinished.
