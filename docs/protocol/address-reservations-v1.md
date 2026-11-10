# Permanent address reservations, version 1

`AddressAllocator` is a separately selected range service over the existing
linearizable atomic journal-store contract. Rust enables `address-reservations`
and imports `engine::address_allocator`; C# uses `Engine.AddressAllocator`; JS/TS
imports `@link-foundation/link-cli-client/engine-address-allocator`. Its portable
Rust contracts also run inside the actual WASM test executable. No storage work,
background tasks or allocation receipts are added to bare stores.

One reserved key binds an immutable inclusive unsigned 64-bit domain. Every
fresh writer in that domain must use the same key and provider. Initialize the
domain above **every identity ever issued**, including removed rows, tombstones,
reserved blocks and outstanding offline work. Scanning currently visible rows
alone cannot establish that boundary. Never delete or replace this key, change
its domain or restore an older coordinator independently of its consumers.

The caller supplies a nonzero 32-character lowercase hexadecimal author and
request token and a positive count. Tokens are unique across all authors in this
key. A matching author/token/count returns the original inclusive range without
a write, even after later grants or exhaustion. Reusing a token with another
author or count fails before CAS. Grants are permanent: rollback, unused suffixes,
process exit and failover do not recycle addresses. The allocator does not
reserve or fence the author's causal counter lineage.

## Canonical journal

Provider generation zero means absence and requires zero entries. Any positive
generation requires this exact first entry, followed by zero or more grants:

```text
["address-reservations1",FIRST,LAST]
["reserve-addresses1",AUTHOR,TOKEN,COUNT,GRANT_FIRST,GRANT_LAST]
```

Every capitalized field is a JSON string. Numeric strings are canonical decimal
u64 values (`0` or a nonzero leading digit followed by decimal digits); domain
first and count must be positive. All other fields use the identity grammar
above. There is no whitespace, escaping, extra field or alternate JSON encoding.
The first grant starts at domain first; each following grant starts immediately
after the preceding inclusive end. Every width must equal count, every range
must fit the bound, and tokens cannot repeat. Exhaustion has no next address
and never wraps to zero. A complete-domain grant of `18446744073709551615`
addresses from one is valid.

A replay admits at most 4,096 entries, 256 bytes per entry, and 1,048,576 aggregate
entry bytes before JSON parsing. Options can reduce entries to 2..4,096 and bytes
to 1..1,048,576. Accepted content is ASCII, so byte and character lengths agree.
Malformed or oversized journals fail without repair or publication. Generation
and entry arrays returned by providers must follow their existing coherent
snapshot contract; observations returned by the allocator are owned copies.

A new request reads once, validates the complete journal and bounds, constructs
one appended candidate and attempts **at most one CAS**. The provider checks the
observed generation and atomically replaces the key, assigning a never-reused
positive generation. Generations are opaque and may jump. Maximum u64 generation
refuses a new grant. A matching retained retry needs no new generation or space.
A conflict or transport exception returns control to the application. After an
uncertain outcome, retain and retry the exact original author/token/count; a
new token could abandon another block. No automatic transport replay occurs.
`requireDurableFlush` rejects providers without an explicit durability capability.

## Ordinary session selection

Reserve a block before opening a causal workspace. Persist the exact request
when its receipt or reply must survive application restart. Never hand the same
block to separate owners that can allocate concurrently. A retained receipt is
an allocation result, not a lease or proof that an old process stopped.

```js
import { AddressAllocator } from '@link-foundation/link-cli-client/engine-address-allocator';
import { CausalSession } from '@link-foundation/link-cli-client/engine-causal-session';
const allocator = new AddressAllocator(journals, 'fresh-addresses', '1',
  '18446744073709551615', { requireDurableFlush: true });
const grant = await allocator.reserve(author, allocationToken, '64');
const tx = await CausalSession.open(binding, batches, grant.first, grant.last, 'reject');
try {
  const link = await tx.client().create(0n, 0n);
  await tx.client().setName(link.index, 'allocated');
  await tx.commit(batchToken);
} finally { await tx.close(); }
```

Rust uses `AddressAllocator::new(store, key, first, last, options)?.reserve(author,
token, count)?`, then `CausalSession::new` with the returned `first`/`last`.
C# uses `new AddressAllocator(store, key, first, last, options).Reserve(author,
token, count)` and `new CausalSession(batches, grant.First, grant.Last, policy)`.
The existing provider remains caller-owned; Rust can recover its owned handle
with `into_store()`. Ordinary workspace staging and batch retry rules remain in
[the causal session contract](../ENGINE-CAUSAL-SESSIONS.md).

## Executable verification and costs

The independent Python oracle checks 5,832 finite schedules and all six
read/CAS interleavings of two writers, and generates the [53 shared fixtures](address-reservations-v1.json).
Focused production tests exercise exact retries before/after lost replies,
following writes, same-base conflicts, owned snapshots, full-u64 exhaustion,
token/domain collisions, bounded admission and byte-preserving refusals.
`bash experiments/issue-138/run-address-allocation-checks.sh` runs those checks
with bounded local resources. Complete suites and coverage remain in CI.

`NATIVE_LSM_LAYOUT=allocation python3 examples/native-lsm-js/run-interop.py`
checks complete files in plain and mirrored directories across Rust, C#, Node,
Bun and Deno. `NATIVE_CAUSAL_LAYOUT=allocation python3
examples/native-lsm-js/run-causal-faults.py` exits before/after atomic publication,
reaps each process, verifies stale ownership refusal, preserves evidence, then
explicitly recovers markers and retries the original request in another runtime.
Read/retained retry hashes must remain unchanged. These are application-crash
experiments; they do not certify hardware power-loss behavior.

The existing real three-member etcd harness additionally runs
`examples/native-lsm-js/allocation-consensus.mjs`: it consumes part of a reserved
block through ordinary actual-WASM links operations, retains the unused gap,
drops a real commit response, runs public Rust/C# reservation examples, races
two writers and checks retained retries after following grants. Native Rust's
`address_allocation` example accepts endpoints, prefix, author, token, count;
C#'s cluster example selects the same CLI with `--reserve-addresses`.

One grant reads, replays, copies and sends O(retained journal) bytes, plus one
coordinator CAS. Local creates within a block need no allocation-service call.
Receipts occupy O(grants) space, and finite admission eventually refuses new
blocks. This version deliberately supplies no receipt pruning or recycled
ranges. Larger blocks reduce coordinator calls and can abandon larger gaps.
All writers share one allocation bottleneck. Author fencing, authorization,
membership, safe history reclamation, stale-owner recovery, multi-catalog atomic
publication, automatic balancing and uncoordinated external writes need separate
protocols. Selecting this service does not establish those remaining guarantees.
