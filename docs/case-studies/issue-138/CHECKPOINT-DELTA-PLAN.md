# Checkpoints through physical delta decorators

Requirement mapping: R1/R3/R6/R7/R8/R9/R12. Continue every broader unmet criterion.

- [x] Trace the physical codec in Rust, C# and JS. Its parser recognizes only
      engine1/engine2 and requires the first revision to be one, so an initial
      engine3 journal cannot compose with the independently selected decorator.
- [x] Add minimal shared-vector regressions in every implementation and capture
      their failures before changing production code.
- [x] Accept one initial checkpoint, preserve its exact wire bytes/checksum,
      and retain contiguous following revisions. Never accept a later checkpoint.
- [x] Verify following physical compression, changes pass-through, selected
      history/state structures, immutable snapshot pins and both replica orders.
- [x] Verify before/after append errors, failed flush and owner fencing with a
      checkpoint seed; reopening discovers the actual durable prefix.
- [x] Run actual-WASM contracts and Node/Bun/Deno binding composition, complete
      local suites, official coverage, native page interoperability and lint.
- [x] Update protocol/cost/release/evidence and PR descriptions, commit and push
      only the prepared branch, then inspect all fresh exact-head CI.

This extends the new logical checkpoint feature. It does not establish a defect
in older ordinary revision-one delta journals. Local atomic file replacement,
object reclamation and the wider acceptance inventory remain unfinished.

Before-code failures in all three implementations are preserved in
checkpoint-delta-before.txt. Five Rust, seven C# and four JS focused contracts
now pass. Complete fresh suites pass 980 native plus six doctests, 585 actual-WASM,
2,187 C# per configuration, 215 JS and 221 repository checks. Node/Bun/Deno
produce identical results for all 36 compositions using the freshly built WASM
package. Real etcd passes default/checkpoint/cache modes; all 48 existing physical
configurations pass 288 writers with mirrored repair. Those file fixtures begin
at ordinary revision one, so persisted initial-checkpoint interchange remains
separate work. checkpoint-delta.txt records exact coverage and payload costs;
the changed codec reaches 100% lines/branches in every measured implementation
and 100% reported native/WASM functions. Whole-project coverage remains below
target. Commit 68fa923 passes all nine fresh exact-head workflows, including
Rust and C# on Linux/macOS/Windows. checkpoint-delta-ci.txt preserves exact
heads, timestamps and run links. Persisted initial-checkpoint handoff is covered
by the following CHECKPOINT-PAGES-PLAN.md integration batch.
