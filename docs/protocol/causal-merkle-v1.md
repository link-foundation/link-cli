# Bounded causal Merkle snapshots, version 1

This protocol is under implementation. The independent model and shared
vectors establish design evidence; production ports and transfer verification
remain unchecked in [the plan](../case-studies/issue-138/CAUSAL-MERKLE-PLAN.md).

A snapshot owns the selected visible causal versions. Group by positive u64
address, reject contradictory dot identities across all groups, and merge each
group using causal-version1. Preserve incomparable versions and deletions;
sort each resulting leaf by author, then numeric counter. Input admission is
at most 4096 versions and 8 MiB of canonical ASCII version bytes, before
deduplication or pruning. Each group admits at most 64 input versions. Smaller
positive selected bounds may be used. Snapshot construction owns all inputs
before its first asynchronous hash operation. No tree or hashing allocation
belongs to an unselected bare engine or local causal owner.

The tree has sixteen levels and sixteen ordered children at each branch.
The sixteen hexadecimal digits of the u64 address choose the path, most
significant digit first. All hashes are SHA256 and transmitted as exactly
64 lowercase hexadecimal characters. Let D be ASCII `causal-merkle1` followed
by a zero byte. Integer fields below are fixed-width unsigned big endian:

- The empty leaf hash is SHA256(D || ASCII E).
- A nonempty leaf is SHA256(D || ASCII L || u64 address || u16 version count ||
  concatenation of u32 byte length and canonical causal-version1 wire bytes
  for each ordered version).
- A branch at depth d, from zero through fifteen, is SHA256(D || ASCII N ||
  u8 d || all sixteen ordered 32-byte child hashes).

Compute depth-specific empty hashes bottom-up. An empty root is the depth-zero
empty hash. A root identifies visible content, not a publication generation,
retry receipt set, membership epoch or complete pruned history. Hash equality
assumes collision resistance and correct canonical input validation.

A node response contains `root`, `prefix`, sixteen `children` hashes and a
`proof`. Prefix is zero through fifteen lowercase hex digits; leading zeros
are significant. The proof has one level per prefix digit, ordered from root
to parent. Each level contains the fifteen ordered sibling hashes with the
path child omitted. Verify the selected prefix, expected root, exact lengths
and canonical hashes before hashing the response's branch and folding its
proof back to the expected root. A matching textual root alone is insufficient.

A leaf response contains `root`, a canonical positive decimal `address`, its
zero through 64 `versions` and a sixteen-level sibling proof in the same order.
Every version must address that row, be canonical and form an already ordered,
deduplicated, nondominated causal leaf. The response may not silently normalize
duplicates, dominated versions, contradictory values or reordered data. The
leaf admits at most 1 MiB of canonical version bytes. Validate the complete
shape before hashing, then verify through all sixteen levels to the expected
root. An empty leaf is an absence proof; absence never authorizes deletion.
Raw sibling bytes occupy exactly 7680 bytes for a leaf, and at most 7200 for
a node. Canonical version byte limits and fixed proof dimensions bound each
message's retained data. Callers also bound raw transport framing before parsing.

The planned pull operation freezes one source root, compares root-bound nodes
with a frozen destination snapshot and descends only through mismatching,
nonempty source ranges. It validates each leaf before passing its versions to
the fresh local destination owner's receive operation. A concurrent destination
edit is reconciled rather than overwritten. Finite node, leaf and byte budgets
bound each attempt; no automatic network or mutation retry is implied. Partial
progress can remain after an error. A new attempt takes fresh roots and skips
already reconciled content. A changed source requires a new snapshot, rather
than mixing roots. Bidirectional exchange may be required for convergence.

Retries do not discard retained tombstones. This transport does not copy the
source's local request receipts, prove contradictions hidden in every pruned
history, fence independent author clones or establish global serializability.
Membership-aware reclamation, automatic balancing, ordinary-links projection,
whole-journal replay improvements and every other unmet requirement remain
unfinished. A caller releases a snapshot by releasing its owned object; the
component retains no global snapshot registry or physical store handle.
