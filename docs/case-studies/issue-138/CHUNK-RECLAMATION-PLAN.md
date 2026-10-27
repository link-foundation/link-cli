# Epoch-fenced chunk reclamation

Objective: compose the bounded chunk policy with the separately selected
reclaiming journal store, retaining atomic publication, safe physical retirement,
owned coherent reads and matching native/WASM language surfaces. This extends
R1/R4/R5/R6/R7/R8/R9/R10/R11/R12; it does not finish their other open criteria.

- [x] Trace existing blob2 publication, epochs, complete namespace scans,
      aliases, exact-generation deletion and one whole-group reader refresh.
- [x] Model every chunk as a root reference. Demonstrate unsafe old-epoch chunk
      reuse and missing epoch CAS before choosing a production protocol.
- [x] Model prefix retirement during group reads, including cache hits and a
      second publisher/retirement during the single allowed whole-group retry.
- [ ] Specify a separate versioned chunk manifest with canonical finite counts,
      epoch-bound object keys and complete protected-reference discovery.
      Preserve blob2 defaults and reject incompatible append-only chunks1 roots.
- [ ] Reproduce the missing selected APIs and unsafe or incomplete behavior
      through minimum native/C#/JS tests before changing production code.
- [ ] Select chunking independently of the existing reclamation/cache settings.
      Validate all inputs before I/O and all catalog metadata before fetching
      chunks. Keep fresh complete staging comparisons and one catalog/epoch CAS.
- [ ] Preserve every live chunk, including duplicate/shared references, aliased
      namespaces, empty journals and concurrently staged current-epoch data.
      Validate all namespace listings before any conditional deletion.
- [ ] Verify coherent reader refresh if any chunk retires between fetching
      prefixes/suffixes. Discard all partial values and cache hits when refreshing
      a group; stop after the existing bounded retry.
- [ ] Exercise finite before/after mutation cuts, lost replies without automatic
      mutation retry, overlapping collectors, ABA, corruption, ownership and
      following edits in every language and actual WASM runtime.
- [ ] Verify actual physical file handoffs and consensus integration, including
      retained engine history, checkpoints, stale workspaces and real collection.
- [ ] Run complete relevant checks/coverage, document exact denominators and
      selected costs, review and publish an atomic useful step, then inspect
      fresh exact-head CI. Keep the PR draft until all wider criteria are met.

The independent `experiments/issue-138/model-chunk-reclamation.py` runs under a
192MiB process address-space limit and a 150,000-state bound. Its two unsafe
negative controls lose live chunks: reusing an earlier-epoch object after root
discovery, and publishing without comparing the observed epoch. Current-epoch
staging with epoch CAS preserves every live chunk across 879 complete states
for two writers/one collector, 999 for one/two and 32,666 for two/two. The model
does not establish production correctness, exact-generation deletion, reader
retry, provider failure handling or physical durability.

The separate 128MiB/100,000-state chunk-reader model first reproduces a fractured
prefix/suffix view when only a missing chunk is refreshed. Whole-group refresh
passes 1,601 complete states across all sixteen old-chunk cache masks, yielding
245 coherent terminal views and sixty bounded fail-closed terminals. Both
publishers, both retirement events, all four chunk fetches and the one retry
interleave. This remains design evidence, not a production reader test.

Proposed first policy: reuse chunks only within the writer's captured epoch.
A collection fence forces later writers to restage their complete journal in
the new epoch, even if an old chunk has identical content. Prefix reuse remains
available between fences. Reusing older keys safely would need a different
protection/publication protocol; do not silently introduce it as a cache shortcut.
Full journal replay, bounded complete listings and one catalog remain explicit
scaling costs. The append-only chunks1 strategy remains separately available.
