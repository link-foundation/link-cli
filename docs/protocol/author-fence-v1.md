# Explicit author takeover fencing

Select `AuthorFence` over an existing coherent `AtomicJournalStore`. Reserve
one immutable ownership key per author in the same coordinator namespace as
that author's data. Every process publishing that author must use that key and
the guarded provider. Each owner lineage keeps a unique nonzero 32-digit
lowercase hexadecimal token; sharing an active token also shares its authority.

The optional guard composes with the ordinary cluster engine, causal owner,
causal batch/session and permanent address allocator APIs. Rust selects
`author-fencing`; C# exposes `AuthorFence`; JS/TS imports the separate
`@link-foundation/link-cli-client/engine-author-fence` entry point. Existing bare
and manually managed providers select no new work or default imports.

A claim is explicit application-authorized takeover. Supply the predecessor
epoch observed by the application and a new owner token. Exactly one competing
claim for that predecessor can publish. The caller decides when takeover is
permitted. There is no timeout, lease expiry, clock, election, failure detector,
background renewal, authentication or membership service.

## Canonical state and admission

One reserved key contains a complete permanent claim journal. Generation zero
means absent and requires an empty journal. A positive generation requires the
exact header `['author-fence1', AUTHOR]`, encoded as a compact JSON string array
with double quotes. Each subsequent entry is `['claim-author1', TOKEN, EPOCH]`,
also a compact JSON string array. All fields are strings, no spaces or escapes.
Epochs are canonical unsigned decimal strings starting at 1 and increasing by
one per record; they are independent of the provider's opaque modification
generation. Tokens are unique across the complete journal. The header fixes
the author permanently. Header-only present state has epoch zero.

Before JSON parsing, admit at most 256 bytes per record, 4096 total entries
including the header, and 1,048,576 bytes across all records. Accepted fields
are ASCII; non-ASCII records reject before JSON parsing, so admitted character
and UTF-8 byte counts agree. Selected options
can lower total entries (minimum two) or bytes (minimum one). Corrupt framing,
extra fields, numeric epochs, altered authors, duplicate tokens, missing or
nonsequential epochs and exceeded limits reject without mutation. All native
numeric fields preserve u64; JS uses decimal strings throughout. The independent
[shared fixtures](author-fence-v1.json) cover canonical state and rejection.

## Claim and retry

`claim(predecessor, token)` reads once and attempts at most one CAS:

1. Replay and validate the entire owned journal.
2. If the token exists, return its original owned descriptor only when it is
   still the latest claim and its original predecessor matches this request.
   Otherwise reject as fenced. A retry never revives a superseded owner.
3. A new token requires the current epoch to equal the requested predecessor.
   Append exactly one next-epoch claim. Exhausted generation or admission
   rejects before publication.
4. CAS only against the exact read generation. A conflict requires an explicit
   application decision. A provider exception or malformed JS publication reply
   leaves the outcome unknown; retry the original predecessor/token. There is
   no automatic mutation replay or newly generated retry token.

Retain the request before calling. A matching retry after a lost response
returns the same active claim without a second CAS. After a later takeover the
same request rejects without a write, including when its first reply was lost.
Claims and receipts are never pruned; the finite journal eventually fills.

## Guarded reads and writes

`bind(claim)` owns the descriptor and returns an AtomicJournalStore provider.
Binding is lazy: each actual operation validates that the bound claim is still
current. It permits 1..31 distinct ordinary data keys, excluding the reserved
fence key. One additional fence key fits the existing 32-key atomic group.

A read obtains the fence and selected data in one coherent snapshot, validates
the complete fence journal and returns the selected data. It may linearize
immediately before takeover. It does not promise that ownership cannot change
after returning.

A CAS first reads/validates the current fence, then adds that exact fence
generation to the same atomic comparison as all caller-selected data. Its fence
update is null/None, meaning compare without rewriting. A takeover between that
read and publication makes the complete CAS conflict; no data key is partially
published. An already superseded owner rejects before a mutation call. Caller
keys, expected frames and updates are captured before JS's first await.

The provider must honor coherent linearizable reads, every selected comparison,
non-reusable modification generations, atomic grouped publication and null
updates without rewriting. Its durable-flush capability passes through; the
selected constructor can require it. An inner provider exception during guarded
data publication retains that provider's unknown-outcome semantics. There is
no dispose of the caller's provider and no automatic removal of a fence.

```js
import { AuthorFence } from '@link-foundation/link-cli-client/engine-author-fence';
const ownership = new AuthorFence(physical, 'author-a-fence', author, {
  requireDurableFlush: true,
});
// Persist this exact request; a new token requests takeover, not retry.
const claim = await ownership.claim('0', ownerToken);
const guarded = ownership.bind(claim);
// Existing APIs receive guarded in place of physical.
const batch = new CausalBatch(guarded, 'author-a-owner', 'author-a-receipts', author);
```

Rust uses `AuthorFence::new(store, key, author, options)?`, `claim(epoch, token)?`
and consumes the coordinator with `bind(claim)?`; `into_store()` returns its
provider. C# uses `new AuthorFence(store, key, author, options)`, `Claim` and
`Bind`; its guarded `Store` property retains the caller-owned provider. No port
implicitly closes or elects an owner.

After takeover, the new guarded batch can resolve an earlier committed token
using its exact original base/edits, then continue the persisted author counter.
The stale guard cannot publish or resolve a receipt through the fenced path.
Fresh allocation needs a new grant: abandon every unused address in the prior
owner's block, including privately staged but unpublished rows. Fencing does
not itself make an old allocation block reusable or validate foreign writes.

Each operation transfers and replays the complete bounded fence journal and
owns its copied strings/claim descriptors. Guarded CAS adds one fence read and
one comparison, with no fence rewrite. This is O(retained claims) metadata work
and one additional key per group. It serializes takeover with publication;
ordinary data conflicts, storage costs and journal replay remain those of the
selected inner provider. Writers bypassing the guard, shared owner tokens,
multiple ownership keys for the same author, permissions, history reclamation,
hardware power loss and multi-catalog atomicity need separate guarantees.

[Verification and commands](../case-studies/issue-138/AUTHOR-FENCING-VERIFICATION.md)
retain the exact measured scope and pending CI scenarios.
