# Causal reconciliation and bounded anti-entropy

This continues the unfinished R4/R5/R6/R7/R8/R9/R10/R11 strategy work.
Fixed-primary prefix copying and an atomic catalog remain useful selectable
strategies. Independent writers require a different conflict model and cannot
be presented as another acknowledgement threshold on the existing log.

- [x] Re-read Dynamo's causal versioning and Merkle reconciliation sections and
      Cassandra's repair/tombstone documentation. Inspect pinned MerkleTree,
      RepairJob and LocalSyncTask source; preserve hashes and exact seams.
- [x] Bound an independent causality model to two authors, five events, 120
      event permutations and 216 state triples. Verify retained concurrent
      siblings, observed resolution, deletion retention, merge laws and rejection
      of contradictory payloads sharing the same causal identity.
- [x] Specify canonical shared wire vectors for author identities, checked
      counters, causal contexts, values, names and deletion markers. Preserve
      incomparable versions; require explicit application resolution or a
      separately selected deterministic projection for the ordinary links API.
      Reject malformed/equal-clock contradictions before replacing live state.
      The initial causal-version1 boundary has 46 codec and eleven merge cases;
      see ../../protocol/causal-version-v1.md. Runtime parity is a separate step.
- [ ] Add failing production conformance tests before the new implementation.
      Include concurrent update/delete, stale reads and retries, duplicate
      deliveries, lost replies, restart, independently writable replicas,
      counter exhaustion, contradictory author histories and following edits.
- [ ] Implement selected local causal storage using native links/journals,
      retaining the bare cache path's costs. Persist mutation and local causal
      counter at the same publication boundary. Prevent two active local owners
      from reusing one author counter; do not invent global serializability.
- [ ] Implement bounded snapshot/Merkle comparison and differing-range transfer.
      Validate the snapshot identity throughout transfer, bound tree depth,
      range sizes, bytes and retries, verify received data, and preserve a retry
      path after uncertain publication. Transfer deletions and causal context.
- [ ] Supply native Rust, C# and actual WASM/JS parity with shared vectors,
      independent finite schedule oracles, physical restart/process probes,
      directory interoperability and unchanged optional-SQLite boundaries.
- [ ] Add scenarios, complexity/space limits and primary-source comparisons;
      run complete local suites and coverage without removing source, then
      inspect exact-head Windows/Linux/macOS and portable CI.

The initial retention design must keep tombstones and causal evidence until a
separately verified membership/reclamation protocol establishes safe removal.
A time-only expiry can resurrect a deleted value when an old replica returns.
Retaining evidence has a disclosed space cost; automatic reclamation is not
implied by a successful causal merge.

Existing AtomicJournalStore requires a global linearizable snapshot/CAS. An
eventually convergent multiwriter backend must not claim to implement that
contract globally. It may use such a store as a local durable publication
component. Local atomicity and causal convergence are distinct from global
serializability, cross-replica uniqueness, cascade constraints and consensus.
Applications selecting those guarantees must retain their coordinated strategy.

The current model is design evidence only. Production anti-entropy, durable
causal state, bounded transfer and all wider unmet criteria remain unfinished;
PR139 stays draft and unmerged. See anti-entropy-causality-model.txt.
