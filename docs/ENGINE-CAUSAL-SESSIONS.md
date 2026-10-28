# Ordinary mutable causal sessions

The selected session stages ordinary links and names in the existing private
memory engine, then publishes its complete final difference with one local
`CausalBatch`. Rust selects `causal-sessions`; C# exposes `CausalSession` over
`INamedTypesLinks<ulong>`; JS separately imports `./engine-causal-session` and
uses the actual Rust WASM workspace. The raw JS methods preserve the entire
u64 range as canonical decimal strings. `tx.client()` supplies the ordinary
JS/TS links and query API with BigInt addresses over that same workspace. Rust
supplies the standard query processor and protocol through a checked named-store
borrow. C# supplies the ordinary processor and standard protocols through a
checked `QueryLinks` adapter. The [session plan](case-studies/issue-138/CAUSAL-MUTABLE-SESSION-PLAN.md)
tracks remaining physical query interchange and wider requirements.

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

The Rust `GenericQueryProcessor<T>` reuses the ordinary matching, substitution,
reference validation and change-reporting implementation; `QueryProcessor`
remains the established u32 facade. `CausalSession::named_links()` and
`CausalWorkspace::named_links()` supply `NamedTypeLinks<u128>` with checked u64
writes, so the query wildcard `u128::MAX` cannot alias a stored identity.

C# uses `AdvancedMixedQueryProcessor<UInt128>.ProcessQuery(tx.QueryLinks, text)`.
The cached adapter checks the live session on every operation and at query entry,
so a retained adapter rejects empty/null queries after preparation or disposal.
Its UInt128 wildcard stays outside the stored u64 range; every stored part uses
checked conversion. The ordinary owned preview uses the caller's fresh cursor
and retained identities, including tombstones. Recreating a retained maximum
identity does not move a lower fresh cursor. Explicit ensures retire only their
matching reservation. The adapter and pending queue are allocated lazily.

```rust,ignore
let query = link_cli::GenericQueryProcessor::<u128>::new(false);
query.process_query(&mut tx.named_links(), "() ((1 1) (draft: 1 1))")?;
// Query saves only finish staging; explicit publication still requires a token.
let versions = tx.commit("00000000000000000000000000000100")?;
```

For JS/TS, obtain `const links = tx.client()` before preparation. Ordinary
`create`, `each`, `count`, update/delete and name operations use full-width
BigInts; `await links.query('() ((100: 100 100))')` stages an ordinary query in
the reserved range. Requests through every client share the same owned queue.
Preparation synchronously ends admission, drains accepted requests and frees the
native handle. Afterwards the standard clients reject reads and writes; the
session's owned `rows`/name diagnostics remain available. Closing any client
closes the session. Client `batch` still sends sequential requests: a later
server rejection can leave earlier edits staged. Explicit session publication
is the atomic batch boundary, and rollback discards the whole private session.

Rust's `GenericLinksOperation<T>`, `execute_request_for` and
`LinksServer::serve_for` reuse the existing operations and server owner thread;
the established u32 names remain available. C# supplies `LinksOperation<T>`,
`LinksRestriction<T>`, `RemoteLinks<T>` and generic `LinksServer.Serve` and
`ExecuteRequest` overloads over the same existing algorithms, keeping the uint
facades. Serve `tx.QueryLinks` to stage private causal requests. Rust
`LinksClient::query_lossless`, C# `LinksClient.QueryLossless` and the selected
JS session client carry original query text in a `(query: text)`
envelope, preserving quoted numeric, wildcard and variable names through text
and binary transports. Use this only with a peer that explicitly supports it;
there is no fallback resend after an unknown outcome. Legacy direct document
queries remain available. Their string-only canonical model, and named reply
documents, do not retain reference kinds for later query replay. LiNo comments
follow the upstream token-boundary rules; quoted hashes stay content.
The [query protocol contract](protocol/causal-query-v1.md) describes the
envelope and shared finite vectors, including concrete zero-valued bindings.

Reference planning runs the same query path on owned private rows, then reserves
its actual allocation sequence without creating probe rows. The ordinary engine
holds its allocator mutex throughout that preview. Each session owns a pending
prefix; later validation passes and actual creation consume the same identities.
An explicit ensure retires its matching pending identity. Rejection or rollback
can leave gaps, as ordinary engine allocation already does. A preview copies the
visible rows and their names and repeats query execution; it is a correctness
cost, not a throughput optimization. Final ordinary constraints and explicit
publication remain authoritative. Existing recycling stores retain their old
allocation-probe fallback, including the separately documented prediction gaps.

The Rust checked causal borrow and C# checked adapter admit at most 1 MiB of query text, depth 64, 4096
syntax nodes, 4096 results, 16,384 bindings and 4 MiB of estimated owned binding
storage. Counts and key bytes are checked before Cartesian map/key cloning.
C# streams each pattern's candidate maps instead of first collecting an
unbounded intermediate list. Preview and adapter matching share the same
restriction implementation and check result counts before retaining each row.
Bare negative or overflowing numeric addresses reject before staging; quoted
numeric names remain names. Named output uses lossless notation literals,
including empty/control names and numeric names outside the stored range.
Rust and C# checked TCP workers apply these text byte/depth bounds before canonical model
conversion and clamp binary link/reference/node/string/depth admission before
decoding. JS preflights bounded query syntax before sending any batch member;
its node count also includes identifiers, so admission can be stricter than the
native AST count. The actual WASM entry point additionally limits the complete
outer JSON request to 1 MiB, including envelopes and escaping. A query that fits
the raw-text bound can exceed this encoded request bound and be refused without
reaching the workspace. Named query/export replies admit at most 1 MiB of
decoded identifier/reference bytes before accumulating the complete document;
notation and JSON escaping add encoding overhead. The owned query preview
checks the same named-change budget before real private edits are applied.
C# generic remote callbacks own read-only three-part rows, avoiding upstream
UInt128 list initialization. Each standard operation is one serialized round
trip; matching owns and sorts results. Checked limits are captured for worker
decoding when serving starts and the live adapter is checked again per request.
Protocol decoding, original-query envelopes, canonical formatting and reply
construction add copies proportional to their bounded input/output. Serving
and closing a client never publish the private session.

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
is inferred. The JS native boundary sends explicit operation JSON with canonical
decimal strings or bounded LiNo text. Parsing, canonical formatting and the JSON
wrapper add owned copies proportional to the encoded request. `each` materializes
and sorts matching rows, and named query replies perform existing name lookups.
Final edits reuse batch encoding without another
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
