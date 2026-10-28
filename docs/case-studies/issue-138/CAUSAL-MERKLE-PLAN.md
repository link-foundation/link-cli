# Bounded causal snapshot comparison and range transfer

This continues ANTI-ENTROPY-PLAN.md after local causal publication. The local
owner and its complete verification remain a separate atomic step. This plan
does not claim that a model or an equal digest provides a production transport.

- [x] Revisit the primary Dynamo paper and pinned Cassandra MerkleTree source.
      Separate mismatch discovery from causal reconciliation/publication.
- [x] Define an independent finite sparse radix-tree model over positive u64
      addresses. Bind leaf bytes, address, node depth and child positions to
      domain-separated SHA256 digests. Include retained deletion versions.
- [x] Compare every pair of finite snapshots against a direct differing-address
      oracle. Reject mutated data, wrong addresses, malformed proofs and mixed
      snapshot roots. Preserve the frozen snapshot while the live source changes.
      The resource-bounded model passes 64 snapshot pairs and 48 proof groups;
      it imports no production implementation. This is structural model evidence,
      not canonical admission or production transfer evidence.
- [x] Specify canonical cross-language vectors and explicit limits: depth 16,
      fanout 16, at most 64 siblings per address, bounded total versions/bytes,
      bounded node/proof/leaf messages and bounded outstanding snapshot lifetime.
      Eight roots, four complete leaf proofs and three node proofs are generated
      independently. Correct an initial model's cross-address dot reuse before
      accepting the vectors; the protocol requires globally distinct dots.
- [x] Add failing native Rust/C#/JS contracts before implementation and share
      the native contracts with the actual-WASM harness.
      Use existing hashing components where appropriate; keep hashing and tree
      allocation absent from the bare engine and local owner unless selected.
- [x] Implement owned immutable snapshot indexing and root-bound comparison.
      Descend only mismatching child hashes. Validate every returned address,
      root and canonical leaf before applying it to the fresh local owner.
      Nine production contracts include all 64 direct causal-union comparisons.
      Focused native Rust/C#/Node/Bun/Deno and actual-WASM tests pass; complete
      current-source validation finishes 2026-10-27T15:59:59Z. Finite failing controls expose
      both single-node restarts and single-leaf starvation when a concurrent
      destination sibling keeps the joined first leaf hash unequal. Opaque
      owned root/author-bound continuations carry at most 241 pending paths;
      every budget stop retains the unprocessed path. Public JS cursors and
      their prototype are frozen against method replacement.
- [x] Verify interrupted/reordered/duplicate range delivery, changed source
      snapshots, concurrent destination edits and before/after local publication
      loss. Retry without discarding accepted concurrent siblings or deletions.
- [x] Compose actual native filesystem owners across all five runtimes and
      preserve directory ownership, reopen, receipt and unchanged-byte checks.
      The implemented finite harness uses four-node/one-leaf budgets and at
      most forty attempts, plus real reaped before/after-publication exits.
      Fifty plain/mirrored message handoffs, twenty reaped exits, one hundred
      unchanged-byte stale-owner refusals and one hundred recovered cross-runtime
      retry/reopen/following handoffs pass in causal-merkle-physical-handoffs.log.
- [x] Add scenarios and exact space/CPU/network costs. Complete suites pass
      1219 native tests plus six doctests, 745 actual-WASM tests, 2484 C# tests
      per configuration, 666 Node tests and 221 repository checks. JS enforces
      100% production lines/branches/functions. Each C# causal component covers
      all executable lines/branches in Debug and Release. Full native/C#/linked-
      WASM coverage remains below 100%; causal-merkle.txt retains all denominators.
- [ ] Inspect fresh exact-head platform CI, including actual Windows/Linux/macOS
      archives and the complete native physical exchange job.

The initial candidate is a sparse sixteen-way tree over the sixteen hexadecimal
digits of a u64 address. Internal hashes include depth and all child positions;
leaf hashes include address and canonical version bytes. Empty subtrees have
depth-specific precomputed hashes. Frozen content roots identify the selected
visible causal state, independently of local retry receipts. A range response
must bind to that root throughout transfer. A newer source root starts another
bounded comparison rather than silently mixing snapshots.

This is a design inference from the source audit, not Cassandra wire or repair
compatibility. Hash equality assumes SHA256 collision resistance and correct
canonical validation; it cannot establish hardware durability, author fencing,
Byzantine agreement, global serializability or safe evidence reclamation.
Current visible version transfer also does not communicate all historical retry
receipts or promise discovery of every contradiction hidden in pruned histories.
Local historical identity checks remain in force for every received version.

Reconciliation must merge through the fresh destination owner. Missing a value
from one snapshot is not a deletion; a retained causal deletion marker is data.
No TTL-based deletion reclamation is part of this step. Whole-journal local
replay/publication costs, ordinary-links projection, automatic balancing,
multi-catalog transactions, all project coverage gaps and every other unmet
R1-R12 criterion remain active. PR139 remains draft and unmerged.
