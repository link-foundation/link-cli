# Physical LSM composition experiment

This is an isolated implementation experiment under `experiments/issue-138`,
not a production store entry, native-file result or completed acceptance
criterion. `PHYSICAL-LSM-PLAN.md` retains all required production work.

- [x] Preserve an actual JavaScript missing-production-API failure before the
      experiment; keep behavioral fixtures importing that production path.
- [x] Compose the real checkpoint-page log, immutable sorted table and semantic
      WAL codec using finite durable images and explicit namespace effects.
- [x] Exercise group guards, owned values, tombstones/live empty journals,
      repeated buffering/WAL replacement/compaction, selected-file damage,
      reservation nonreuse, counter exhaustion and cleanup failures.
- [x] Exercise every before/after mutation in one compaction trace and one
      group-write trace that includes compaction; reopen after every cut and
      continue with another write/publication.
- [x] Preflight the prospective manifest and following group together before
      maintenance starts. Retain a regression for this design correction.
- [x] Run the thirteen controls in Node, Bun and Deno; save full outputs.
- [x] Reproduce and correct hidden conflicting historical generations beneath
      a newer value; preserve a valid identical-history control.
- [x] Reproduce and reject a durable provider's non-durable returned device
      before sealing/publishing it; retain fencing, cleanup and following use.
- [x] Verify separate read-only checkpoint discovery against committed prefixes,
      ignored tails, missing tables and damaged decisions in all three JS runtimes.
- [x] Execute matching Rust/C# missing-API fixtures on the codec-only baseline;
      all three language controls fail with their named missing-store errors.
- [x] Run the exact Node-only CI commands locally and include the finite
      composition and read-only discovery controls in the JS workflow.
- [ ] Implement portable production stores and all matching fault controls.
- [ ] Supply native namespaces with complete bounded listings, exclusive
      ownership, read-only selected-prefix discovery before cleanup, durable
      table names and observable cleanup errors.
- [ ] Verify actual files/process exits, all five native writer/reader pairs,
      cluster integration, complete coverage and fresh exact-head CI.

Reproduce the experiment with
`bash experiments/issue-138/run-lsm-store-prototype.sh`. The driver redirects a
generated copy of the preserved core fixtures to the isolated prototype, removes
that generated copy on exit, and uses the additional prototype controls. Node
and Deno have 128 MiB heaps/512 KiB stacks; all inputs and fault traces are finite.
The prototype retains only row metadata for disk values; point reads fetch their
actual payload pages. Full foreground compaction is bounded and synchronous.

Each runtime passes 72 before/after compaction cuts and 80 before/after group
write cuts, plus the other controls. This is not physical power-loss evidence,
an actual-WASM result, a Rust/C# store result or a native directory guarantee.
`dev/log/issues/138/pulls/139/lsm-store-prototype.txt` preserves measured scope.

Run `bash experiments/issue-138/run-lsm-readonly-inspection.sh` for six separate
controls in Node/Bun/Deno. The experiment validates the selected physical WAL
prefix without initializing empty files, completing staged repairs or truncating
an uncommitted suffix. A missing selected table is then rejected before the WAL
changes. This establishes the discovery order for the future native owner; it
does not add a production discovery API or native filesystem implementation.
