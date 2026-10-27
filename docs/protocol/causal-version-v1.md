# Causal version records (implementation in progress)

This is the portable semantic boundary for the independently selected causal
replication strategy. It does not implement a replicated store, transport,
durability, serializable transactions or safe tombstone reclamation. Existing
coordinated engines keep their stronger global contracts.

A record is the UTF-8 encoding of compact JSON with this exact array shape:

`["causal-version1",ADDRESS,[AUTHOR,COUNTER],CLOCK,PAYLOAD]`

- ADDRESS is a positive u64 encoded as a decimal string, without leading zeros.
- AUTHOR is 32 lowercase hexadecimal digits, excluding the all-zero identity.
  It identifies a single persistent counter owner, rather than a hostname or
  an authentication credential. Cloning one author's storage into active
  independent writers is prohibited until a separate ownership protocol fences
  the old owner. Identity uniqueness is an explicit trust assumption.
- COUNTER is a positive canonical decimal u64. Each author must reserve counters
  and publish mutations at one durable atomic boundary. Counters may skip but
  must never wrap or be reused after restart or an uncertain response.
- CLOCK is a nonempty array of `[AUTHOR,COUNTER]` pairs in strictly increasing
  author order. At most 64 distinct authors are admitted. The operation's author
  must occur with its exact dot counter. Absent authors have counter zero;
  explicit zero entries are forbidden.
- PAYLOAD is null for a retained deletion, or `[SOURCE,TARGET,NAME]` for a row.
  Endpoints are canonical decimal u64 strings including zero. NAME is null for
  absent or lowercase hex of valid UTF-8 for a present name, including empty.
  A name admits at most 4096 decoded bytes, including NUL and Unicode scalars.

The complete record admits at most 65536 bytes. No whitespace, alternate JSON
escapes, trailing newline or extra field is canonical. All fields are ASCII;
the name encoding makes canonical output independent of JSON Unicode escaping
choices in Rust, C# and JS. Decode validates and reproduces these exact bytes
before accepting a record. The bound applies before JSON parsing and allocation
of decoded names. There is no digest or authentication claim in this codec.

Two clocks compare componentwise, treating missing authors as zero. A strict
dominating version replaces an observed older version. Incomparable versions
remain siblings, including concurrent update/deletion. Merging validates every
input before pruning; the same dot cannot have a different context or payload,
and identical clocks cannot name different operations. A merge admits at most
64 versions per input and 64 surviving siblings, compares at most 128 versions,
and returns canonical dot order without mutating either input. Empty/empty is
valid; mixed addresses reject. Retained evidence and any author lineage needed
to detect contradictions after pruning require storage protocol work beyond
this bounded semantic operation.

A subsequent edit must explicitly supply its observed siblings, join their
clocks and advance the selected author's persistent counter without overflow.
An ordinary one-value links read requires an explicitly chosen projection or
application resolution; receiving two siblings cannot silently discard one.
Deleting a value does not erase its clock. No time-only garbage collection is
safe when an older replica can return.

[Independent vectors](causal-version-v1.json) include integer limits, Unicode,
deletions, incompatible identities, malformed encodings and conflicts that
must reject even when another supplied version dominates them. The generator
does not import any production codec. Production parity, durable ownership,
restart and bounded anti-entropy remain active under
[the plan](../case-studies/issue-138/ANTI-ENTROPY-PLAN.md).
