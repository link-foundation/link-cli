# Candidate local batch composition

Status: the original design and finite experiment are now followed by production
batch ports in Rust/C#/JS and actual WASM. Focused contracts and 53 independent
wire vectors pass, as do 50 five-runtime physical handoffs and 100 recovered
retry/following checks after twenty reaped process exits. Complete local suites
finish at 2026-10-27T18:15:46Z; causal-batch.txt records every coverage denominator,
remaining gap, raw report hash and interrupted-driver rerun. Publication and
fresh platform CI remain required.
The ordinary causal-session API remains unfinished. CAUSAL-SESSIONS-PLAN.md
tracks the remaining work; ENGINE-CAUSAL-BATCHES.md describes the selected API.

## Publication and compatibility

Select a new batch owner over one coherent AtomicJournalStore and reserve two
distinct keys: an existing causal owner journal and a batch-receipt journal.
Read both at one coherent boundary. Run the existing CausalReplica against a
private in-memory staging provider initialized with the owner journal. Each
effective row edit invokes its existing write API. Publish the staged owner
journal and the complete appended receipt journal in one two-key CAS against
the original snapshot. Never expose the private provider's synthetic generation
as the external expected generation.

This preserves the existing causal owner grammar and historical validation.
Single-write and receive consumers can continue reading the owner key. The
batch component performs its own receipt work only when selected. Its constructor
must retain the caller's physical-store ownership and durability choice.

The candidate receipt journal contains compact canonical ASCII JSON arrays:

1. `["causal-batches1",OWNER_KEY,AUTHOR]` binds it to one existing lineage.
2. `["causal-batch1",TOKEN,BASE_GENERATION,START_COUNTER,EDITS]` records one
   successful request. EDITS contains `[ADDRESS,OBSERVED,PAYLOAD]` triples in
   strictly increasing numeric address order. Each address occurs once.

Addresses, generations and counters are canonical unsigned decimal strings;
addresses are positive. Authors and tokens use the existing nonzero lowercase
32-digit hexadecimal grammar. OBSERVED contains the canonical merged versions
of that row. PAYLOAD uses the existing causal-version payload array, including
hexadecimal UTF-8 names and null retained deletion markers. A receipt must encode
the original base generation; retrying with a changed base is a changed request.

Generation zero requires an empty receipt list. A nonzero generation requires
the exact matching header. A nonempty receipt lineage also requires an existing
matching owner header, including when its only batch has no edits. A first
empty batch publishes that owner header and its receipt together.

## Ownership and retries

For edit index i, derive its existing owner retry identifier by interpreting
TOKEN as an unsigned 128-bit integer and adding i, then formatting 32 lowercase
hexadecimal digits. Preflight the complete range; reject overflow. A fresh
batch's resulting dot counters must be START_COUNTER+i+1. A preexisting
single-write receipt or overlapping child-token range therefore rejects the
fresh batch even if that receipt's normalized request happens to match. Such
collisions affect availability, never permission to return an old mutation as
a fresh result. Callers should reserve disjoint token ranges for batches.

The receipt journal must reject duplicate batch tokens and malformed or
contradictory receipt ownership. Verify each nonempty receipt against its actual
single-write owner records: derived token, exact normalized request, original
author and expected contiguous counter range. Reuse an index built from the
already validated owner journal. Full supplied observation evidence remains in
the owner records even when its merged descriptor is smaller.

A matching receipt is checked before rejecting its now-stale base generation.
Validate all caller-supplied evidence against the owner's historical identities,
including dominated observations, as the existing write API does. Return owned
original versions without a CAS. Unknown receipts require the current owner
generation to match the original base; a later-row error, exhausted counter,
collision, record limit or stale generation must publish neither earlier edits
nor metadata. CAS false means conflict. Exceptions or malformed CAS replies
mean an indeterminate outcome. The caller rereads and retries the original
request; no automatic mutation retry is allowed.

## Admission and selected costs

The production candidate must admit at most 64 effective edits per batch and
check the entire counter range before publication. Existing owner limits remain
4096 records/8 MiB total and 1 MiB per record. Receipt limits are independently
bounded by the same record and byte maxima. Before receipt JSON allocation,
require ASCII, no escapes or object syntax, balanced quotes/arrays, at most
seven array levels and 32768 unquoted separators. All limits apply together;
64 edits each containing their own maximum-size observations need not fit.
Reject capacity exhaustion before real CAS. Zero-edit receipts retain request
identity and consume one bounded metadata record.

Staging reuses complete owner replay for each edit. For E edits, H retained
owner versions, R owner records and maximum encoded version size W, its initial
worst-case replay work is O((E+1)H²W); repeated staging publication also copies
the bounded owner journal. Receipt validation/indexing adds work proportional
to its complete bounded metadata and referenced owner records. Publication
copies both complete journals, up to their independent limits, before the
underlying provider's cost. These costs and retained receipts belong to the
selected batch component. Efficient indexed replay remains an open scaling
criterion and must not be inferred from a passing finite experiment.

## Evidence and remaining work

causal-batch-model.py independently passes 30 finite scenarios under 192 MiB
address-space and 8 MiB stack bounds. The eight test-first staging controls pass
Node, Bun and Deno against the actual existing CausalReplica, including exact
legacy records, a failing later edit, two concurrent staged requests, before/
after reply errors, following edits, retry and empty publication. The experiment
uses a trusted finite mock and a deliberately temporary receipt representation;
it does not implement the proposed wire admission or all ownership validation.

The production ports now consume the same canonical/malformed vector file.
Separate absent-API tests first failed; JS admission also reproduced mutation
of a caller's payload across its first asynchronous store read, then passed
after owning that input before the read. Full physical composition passes on
SQL-free plain/mirrored stores in all five runtimes. Complete suites pass 1226
native tests plus six doctests, 752 actual WASM, 2491 C# per configuration,
681 Node and 221 repository checks. JS enforces 100% production coverage; all
four C# causal components cover their executable lines and branches. Complete
project coverage remains below 100%; ordinary links/name APIs are unfinished.
Remote per-leaf transfer does not preserve whole local batches as an atomic
remote group. Global address uniqueness, cascades and distributed serializability
still require an appropriate coordinated strategy. Every wider unmet criterion
remains active, and PR139 stays draft and unmerged.
