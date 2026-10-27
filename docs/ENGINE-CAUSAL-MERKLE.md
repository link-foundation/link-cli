# Selectable causal snapshot comparison

Select Rust's `causal-merkle` feature and `engine::causal_merkle`, C#'s
`CausalMerkleSnapshot`/`CausalMerkleTransfer`, or JS/TS's independent
`./engine-causal-merkle` export. The component freezes owned visible causal
versions, compares SHA256 content trees and transfers only differing nonempty
source ranges through a fresh local `CausalReplica.receive`. Concurrent
versions and retained deletions remain data. A missing source value never
authorizes deleting a destination value.

```js
import { CausalMerkleSnapshot, pullCausal } from '@link-foundation/link-cli-client/engine-causal-merkle';
const source = await CausalMerkleSnapshot.create((await sourceOwner.snapshot()).versions);
let continuation = null;
let complete = false;
for (let attempt = 0; attempt < 4096; attempt++) {
  const progress = await pullCausal(source, destinationOwner,
    { maxNodeRequests: 32, maxLeafRequests: 4, continuation });
  if (progress.complete) { complete = true; break; }
  continuation = progress.continuation;
}
if (!complete) throw new Error('Selected synchronization attempt budget exhausted');
```

The example assumes each complete source leaf fits the selected byte budget.
Callers bound their overall attempts and snapshot lifetime for their workload.
The component itself makes one finite attempt and performs no automatic retry.
A remote adapter supplies `root`, `node(prefix)` and `leaf(address)` using the
[canonical root-bound proof frames](protocol/causal-merkle-v1.md). Bound raw
frames during reception before constructing strings; successful proof checking
cannot undo an unbounded transport allocation. Every response is verified
against the requested path and frozen source root before local publication.

A short attempt returns an opaque owned continuation. Pass it to the same
monotone destination journal and frozen source snapshot. It retains at most
241 pending paths of at most sixteen hex digits, plus root and destination
author, with no store handle or global registry. Public APIs cannot construct
its frontier; copies do not share mutable traversal state. A changed source
root is rejected before reading the destination; a different destination author
is rejected before publication. These checks do not make restoring an old
journal or cloning an author safe: the existing unique-author/no-rollback
contract still applies. Do not resume a cursor after replacing its destination
journal. A completed traversal returns no continuation. Discarding a cursor
and restarting is safe, but small repeated budgets can revisit concurrent
leaves forever; carrying the frontier ensures traversal advances.

Each attempt refreshes its destination snapshot, then merges verified leaves
through the fresh owner. Concurrent destination writes therefore survive.
Errors can leave earlier leaves committed; restart with fresh snapshots after
an error. An uncertain local CAS reply follows the local owner's existing
reread/retry contract. Completion means the selected source traversal finished;
it need not produce equal roots when the destination has additional values.
Use bidirectional exchange when both sides need their causal union.

Admission defaults and maxima are 4096 input versions and 8 MiB of canonical
version bytes, before pruning. Each address admits at most 64 input versions.
Node request defaults/maxima are 4096/65536, leaf requests 64/4096, and accepted
version bytes 8 MiB/8 MiB. All selected limits must be positive. An attempt can
fetch one additional bounded leaf before rejecting it for the byte budget;
that leaf remains pending and consumes no publication. Increasing that budget
may be necessary to fit a complete leaf. Per-frame maxima are 32768 bytes for
nodes and 1081344 bytes for leaves. Proof depth/fanout are fixed at sixteen.

For R addresses and V input versions with maximum wire length W, the tree has
at most 16R internal nodes plus R leaves. Retained tree data is O(16R + VW),
plus O(16R) path keys and container overhead. Construction hashes each retained
version wire and sixteen child hashes per internal node. Per-row causal
reconciliation also costs up to O(64²W); the local journal's historical replay
and publication costs still apply. A full unequal traversal makes at most
16R node requests and R leaf requests. Equal roots make zero requests; equal
subtrees skip their whole range. Node proofs add 480 raw sibling bytes per
prefix digit; a full leaf proof adds exactly 7680 raw sibling bytes. Hex and
JSON framing increase those wire sizes. Bounds describe this initial strategy,
not a throughput benchmark or an incremental persistent Merkle index.
A branch hash consumes exactly 529 preimage bytes; an empty-leaf hash consumes
16. A nonempty leaf consumes 26 framing bytes plus four length bytes and the
canonical wire bytes per version. Verifying a full leaf adds sixteen branch
hashes (8464 preimage bytes); verifying a node at prefix length d adds d + 1
branch hashes. These counts exclude canonical validation, copies and local
journal work.

For N node requests and L leaf requests, admitted response frames total at most
N × 32768 + L × 1081344 bytes. Defaults therefore permit 194 MiB of response
frames; the largest selected request counts permit 6272 MiB. Accepted version
bytes still stop at 8 MiB, and a real source/destination's admission can stop
earlier. These are finite worst-case wire limits, including rejected frames,
not a promise that the version-byte budget caps all network traffic. Lower
request budgets and continuations reduce each attempt's work without retaining
all those frames in memory.

On Linux with Rust, .NET 10, Python 3, Node, Bun and Deno installed, run the
finite physical exchange from the repository root:

```sh
CARGO_BUILD_JOBS=1 cargo build --manifest-path rust/Cargo.toml --no-default-features \
  --features native-pages,physical-lsm,causal-merkle --example native_lsm
dotnet build examples/cluster-csharp/ClusterExample.csproj --configuration Release
python3 examples/native-lsm-js/run-merkle-interop.py
```

Its bounded offline message bundle connects separate native physical owners;
it does not require SQLite or claim a production RPC service. Five writers and
five readers exercise plain/mirrored directories, concurrent siblings,
retained deletions, owned receipts, unchanged physical hashes on reads/retries,
reopening and following writes. Reaped process exits on either side of local
publication use the existing explicit stale-marker quarantine procedure.
Application process exits do not certify hardware power-loss behavior.
The harness uses POSIX resource limits and the native file provider's durable
namespace operations. It does not establish the same physical durability on
Windows, where the provider reports that capability unavailable.

SHA256 equality assumes collision resistance and canonical admission. Visible
content roots omit pruned history, local request receipts and membership state.
This selection does not supply global address allocation, ordinary links
projection, membership-aware evidence reclamation, automatic balancing or
serializable distributed transactions. Those and every other unmet requirement
remain active in the issue's case study.

The physical pull example also carries continuations with four-node/one-leaf
budgets and at most forty attempts, so the concurrent first leaf cannot starve
the second address during actual wire/file exchange.
