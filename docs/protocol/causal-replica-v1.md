# Local causal publication, version 1

This is the local durable-owner protocol for the separately selected causal
component. It uses a coherent `AtomicJournalStore` snapshot/CAS on one reserved
key. It does not make a multiwriter network globally linearizable. The bounded
implementation and real filesystem composition must each pass their own checks;
the protocol alone establishes no durability result.

All records are canonical compact ASCII JSON arrays. Version arrays below use
the exact [causal-version1](causal-version-v1.md) grammar. Authors and retry
identifiers are nonzero 32-character lowercase hexadecimal strings. Every
address and counter is a canonical unsigned decimal string. No whitespace,
extra fields, alternate escaping, reordered context, or unknown tag is accepted.

The selected key contains an ordered list of records:

1. `["causal-replica1",AUTHOR]` binds the complete lineage to one author.
2. `["causal-write1",TOKEN,OBSERVED,V]` records one local mutation and its retry
   receipt. OBSERVED retains all explicitly observed versions, sorted and
   deduplicated by dot, including dominated observations. Validate every
   observation against history before pruning; retain those identities through
   replay. All observations address V's row. Recompute V by joining OBSERVED,
   then advancing the durable global local
   counter by one. The counter is global to this local lineage, across rows;
   it is reconstructed from writes in record order. Mutation, counter and token
   become visible at the same local CAS boundary.
3. `["causal-receive1",VERSIONS]` records a nonempty batch of at most 64 versions,
   sorted numerically by address, then author bytes, then numeric dot counter.
   Dots are unique across the batch. Validate the entire incoming batch against
   retained history before publishing. A duplicate known delivery has no effect
   and performs no CAS. Receive does not increment the local author counter.

Nested versions are JSON arrays, rather than JSON-escaped wire strings. Names
remain hexadecimal UTF-8 within those arrays, so all three encoders produce
identical ASCII bytes. Generation zero requires an empty record list. A nonzero
generation requires the matching header; an empty nonzero list is erased state
and rejects. A header alone represents an owner with no mutations.

Replay retains every historical dot, including versions pruned from the visible
siblings. A dot identifies one operation across all addresses. Different content
for an existing dot, or different operations for one row with equal clocks,
rejects before publication. An unknown incoming dot from the local author or a
clock beyond its durable local counter also rejects. Equal known operations are
idempotent. Concurrent updates and deletion markers remain siblings; explicit
observations are required to resolve them. Deletions are retained indefinitely.

A token identifies the normalized request `(address, merged observed, payload)`.
Every supplied observation is checked against history even when it would be
pruned by normalization, including on a receipt retry. New writes durably retain
the full admitted evidence. A successful receipt retry adds no journal record.
Repeating the same request returns its original owned version without a CAS,
including after reopening and intervening edits. Reusing the token for a changed
request rejects. A CAS false result is a conflict; a thrown or malformed result
is indeterminate. No mutation automatically retries. After an uncertain reply,
the caller rereads and retries the same token; durable replay determines whether
that request already committed.

Each operation rereads and validates the whole selected journal. Defaults and
hard maxima are 4096 entries, including the header, and 8 MiB of ASCII record
bytes; callers may select smaller positive byte bounds and entry bounds of at
least two. Each record additionally admits at most 1 MiB, five nested array
levels and 32768 unquoted separators before JSON parsing. Non-ASCII bytes,
escapes, object delimiters and unmatched quotes/brackets reject before parsing. These bounds admit
the maximum 64-version batch, including 64-author clocks and 4096-byte names,
while bounding the parser's allocation shape. Capacity exhaustion rejects
before CAS. The underlying store's own
limits may reject a smaller journal. At most 64 visible siblings per row and 64
authors per version are inherited semantic bounds. Public snapshots own their
nested data. Callers retain ownership of the physical store and close it through
its normal API.

For H retained versions, R records and maximum version size W, replay retains
O(HW + R) bytes and scans history for equal-clock contradictions, with worst-case
O(H²W) comparison work. A publication copies O(RW) journal bytes before the
underlying store's own work. These deliberately finite bounds are not an
unbounded replay or throughput claim. Later indexing or compaction must preserve
historical contradiction evidence and retry receipts. Bare engines allocate no
causal metadata unless this component is selected.

Two active handles on the same coherent local store are fenced by CAS. Two
independent writable copies must have different authors: local CAS cannot fence
an offline clone. Backup rollback requires a new identity or external fencing.
External deletion or replacement of the reserved key is outside the contract.
Global uniqueness, cascade constraints, global serializability, Merkle/range
transport and safe reclamation require separate selected protocols.
