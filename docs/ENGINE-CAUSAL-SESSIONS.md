# Ordinary mutable causal sessions

The selected session stages ordinary links and names in the existing private
memory engine, then publishes its complete final difference with one local
`CausalBatch`. Rust selects `causal-sessions`; C# exposes `CausalSession` over
`INamedTypesLinks<ulong>`; JS separately imports `./engine-causal-session` and
uses the actual Rust WASM workspace. The raw JS methods preserve the entire
u64 range as canonical decimal strings. Full parity with the existing u32
LiNo query transport remains unfinished and is tracked in the
[session plan](case-studies/issue-138/CAUSAL-MUTABLE-SESSION-PLAN.md).

Choose an explicit conflict policy and a fresh inclusive address reservation.
The reservation cannot intersect any retained causal identity, including a
deletion. The workspace's cursor is independent of the ordinary private
journal allocator: a remote visible row at u64::MAX does not prevent creating
a row in a lower reserved range. Explicit recreation of a known identity is
allowed; fresh identities outside the reservation reject. Independent writers
need disjoint reservations and uniquely fenced author lineages. This API does
not supply a distributed allocator.

```js
import { CausalSession } from '@link-foundation/link-cli-client/engine-causal-session';
const tx = await CausalSession.open(binding, batch, '100', '199', 'reject', {
  stateStructure: 'shared', constraintValidation: 'indexed',
  constraints: ['unique-names'],
});
try {
  const id = await tx.createLink('0', '2');
  await tx.setName(id, 'draft');
  const request = await tx.prepare('00000000000000000000000000000100');
  // Retain request to resolve an interrupted process's exact publication.
  const versions = await tx.commit(request.token);
} finally {
  await tx.close();
}
```

Rust uses `CausalSession::new(batch, first, last, policy, options)?`, the
existing `LinksStorage<u64>` trait and explicit name methods. C# uses
`new CausalSession(batch, first, last, policy, options)` and the standard
links/name methods, including callbacks and restriction matching. All ports
validate the initial projected rows and the final private commit with the
selected state representation and constraints. Raw deletion remains
non-cascading, duplicate pairs remain distinct unless the selected constraint
rejects them, and null, empty, Unicode, BOM and NUL names remain distinct.

Preparation freezes the original owner generation, retry token and complete
ordered final edits. Each changed identity observes every retained sibling,
including a losing deletion. A create followed by deletion and a name changed
back to its original value produce no edit. The empty batch still has an
exact receipt and consumes no author counter. Up to 64 final changed identities
are admitted. Initial/final constraint failures, stale bases, exhausted address
or counter ranges, invalid child-token ranges and oversized requests cannot
publish a partial batch. A valid token with a failing final validation leaves
the workspace frozen; dispose it and open a fresh session. An invalid token's
initial syntax check leaves the workspace usable.

In JS, `prepare` synchronously reserves the native handle before queued work
drains; later mutations reject. The handle is freed after preparation on both
success and failure. Concurrent identical commits share one pending attempt.
Unknown publication replies retain the exact original request; retry the same
token without rebasing or further mutation. After process exit, use
`batch.write(request.token, request.generation, request.edits)` with that retained
request. A successful cached retry returns owned values even after following
writes. Repeated rollback/close releases private state; it cannot undo a batch
already published. Prepared diagnostics remain owned and available for recovery.
Publication atomicity depends on the coherent two-key provider. Transfer of
individual remote leaves does not preserve the batch as an atomic group.

For V supplied versions, per-row counts r and maximum version size W,
projection admission and selection cost O(V log V + sum(r²W)) plus owned copies.
The default input bounds are 4096 versions, 8 MiB of canonical version bytes,
and 64 supplied versions per row. Each workspace also retains the original
visible rows, all retained observations and an ordinary engine snapshot/write
set. The selected flat/shared/sorted-runs representation determines mutation
costs; see the [engine structures](ENGINE-STRATEGIES.md). A fresh
workspace makes no real-store writes while staging.

Finishing builds an ordered final map and scans original/final identities,
costing O((R + F) log(R + F)) plus payload/observation copies for R initial and F
final rows. Rust's ordered lookup and C#'s hash lookup differ within that upper
bound. Name lookups scan the private/final rows. Each returned row collection
owns O(F) rows and their name bytes. Preparing and publishing also pay the
existing complete retained-journal replay/copy costs described for
[batches](ENGINE-CAUSAL-BATCHES.md); no incremental replay or throughput claim
is inferred. The JS native boundary sends only explicit operation JSON and
canonical decimal strings. Final edits reuse batch encoding without another
durable wire format, and the shared vectors are
[`causal-session-v1.json`](protocol/causal-session-v1.json).

The runnable physical examples are
[`session-interop.mjs`](../examples/native-lsm-js/session-interop.mjs),
[`session.rs`](../rust/examples/native_lsm/session.rs), and
[`CausalSessionInterop.cs`](../examples/cluster-csharp/CausalSessionInterop.cs).
With the compiled native examples and WASM package available, run
`NATIVE_LSM_LAYOUT=session python3 examples/native-lsm-js/run-interop.py` for
50 plain/mirrored complete-store handoffs, and
`NATIVE_CAUSAL_LAYOUT=session python3 examples/native-lsm-js/run-causal-faults.py`
for 20 finite, reaped process exits and 100 recovery/following sequences.
Readers and durable retries preserve every file hash. Operator marker recovery
happens only after the owner has exited and been reaped, preserving the markers
as evidence. These are application-crash experiments; hardware power-loss
certification remains unfinished. Complete project coverage, full query parity,
global allocation, membership/history reclamation and every other unmet issue
requirement remain tracked in the plan; PR139 stays draft and unmerged.
