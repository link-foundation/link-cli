# Causal reconciliation source audit (2026-10-27)

Amazon's original [Dynamo paper, sections 4.4 and 4.7](https://www.allthingsdistributed.com/2007/10/amazons_dynamo.html)
separates causal version reconciliation from replica comparison. Per-object
vectors distinguish overwritten versions from concurrent siblings; an
application can resolve the observed siblings in a subsequent write. Merkle
comparison descends through differing ranges before transferring values.
This is broader than checking whether a follower contains a primary's prefix.

Cassandra's [repair documentation](https://cassandra.apache.org/doc/stable/cassandra/managing/operating/repair.html)
distinguishes full and incremental comparison. Its
[tombstone documentation](https://cassandra.apache.org/doc/latest/cassandra/managing/operating/compaction/tombstones.html)
describes resurrection when old replicas outlive retained deletion evidence.
Design inference: transfer must retain causal deletions, and safe reclamation
requires its own membership/coverage argument. A local timer cannot establish
that every surviving replica has observed a deletion.

The following source seams were downloaded and read at Cassandra commit
`05853eaeab006bd1e834331ac8b1dfdc464d1f31`; no implementation is copied:

| Pinned source | Observed seam | Consequence for this implementation |
| --- | --- | --- |
| [MerkleTree.java:205](https://github.com/apache/cassandra/blob/05853eaeab006bd1e834331ac8b1dfdc464d1f31/src/java/org/apache/cassandra/utils/MerkleTree.java#L205) | Comparison requires the same token range, skips equal hashes and descends through differences with depth/midpoint termination. | A selectable protocol needs stable range identity and explicit tree bounds. Equal roots permit skipping transfer only under the declared digest/data-validation assumptions. |
| [RepairJob.java:374](https://github.com/apache/cassandra/blob/05853eaeab006bd1e834331ac8b1dfdc464d1f31/src/java/org/apache/cassandra/repair/RepairJob.java#L374) | Pairwise differences create stream tasks; source/target direction depends on replica roles and pull-repair selection. | A mismatch is not an instruction to overwrite one whole replica. Reconciliation and transfer direction must preserve independently accepted versions. |
| [LocalSyncTask.java:80](https://github.com/apache/cassandra/blob/05853eaeab006bd1e834331ac8b1dfdc464d1f31/src/java/org/apache/cassandra/repair/LocalSyncTask.java#L80) | Selected ranges are requested/transferred through a repair stream plan; completion/failure is reported independently. | Snapshot validation, publication acknowledgement and retry ownership need explicit boundaries rather than being inferred from a network copy. |

The local source hashes and finite model receipt are preserved in
anti-entropy-causality-model.txt. The independent model checks 120 delivery
orders and 216 state triples for the explicitly listed two-author states.
It retains concurrent update/delete siblings, joins observed resolution and
rejects contradictory data sharing a causal identity. Its deletion negative
control demonstrates resurrection after discarding retained evidence.

Implementation decisions to verify in [ANTI-ENTROPY-PLAN.md](ANTI-ENTROPY-PLAN.md):

- Persist author counter and mutation together. Restart, lost replies and two
  active owners must not produce different values under one causal identity.
- Use canonical integer/string encodings across native and actual WASM/JS;
  preserve incomparable versions and require an explicit projection/resolution
  policy for a single-value ordinary links read.
- Validate a snapshot identity during bounded Merkle/range transfer. Bound
  depth, records, bytes and retries; receive/validate a complete candidate
  before atomic local publication. Keep tombstones without time-only expiry.
- Reuse native journals/pages/LSM as local persistence components. Preserve the
  existing global AtomicJournalStore linearizability contract: eventual
  multiwriter convergence must not masquerade as globally serializable CAS.
- Keep constraints requiring global coordination on their coordinated path;
  disclose selected metadata, hashing, retained-history and network costs.

These are source observations, a finite semantic model and an active plan.
Production causal storage, Merkle transfer, three-language parity and crash
verification remain unfinished. No availability/latency equivalence with
Dynamo, Cassandra or Accord is claimed, and all wider unmet criteria remain
active in PR139.
