# Native and WASM heap-doublets contracts

Continue R1/R7/R8 without changing the public storage API or excluding compiled
production sources from coverage. The verified `9ece2d1` complete linked-WASM
report contains `storage/doublets_storage.rs` with 0/264 lines and 0/47 functions
executed. Existing file-backed tests cannot exercise it in a browser runtime.

- [x] Inspect existing storage, upstream-decorator and transaction observer tests.
- [x] Preserve the verified complete-report baseline and its artifact identity.
- [x] Add shared contracts using the existing public heap-doublets adapter.
- [x] Check CRUD, patterns, recycling, owned-store access and typed errors.
- [x] Check uniqueness/cascade observations and transactional rollback.
- [x] Reproduce missing null-endpoint queries/search and incomplete duplicate queries.
- [x] Correct literal matching while retaining an available endpoint index.
- [x] Run only the changed native target locally with bounded build concurrency.
- [x] Run the same contracts in actual WASM; retain complete source denominators.
- [x] Check formatting, focused Clippy, source size and repository checks.
- [x] Preserve commands, source/report digests, outcomes and remaining gaps.
- [ ] Publish the atomic change and verify fresh exact-head CI after existing runs finish.

Heap-backed semantic checks do not establish filesystem durability, cross-process
locking, mapped resize safety or hardware power-loss recovery. The existing
native tests for those backends remain in place. The broader R1–R12 inventory
remains unfinished.

The initial eleven-test run fails three public contracts before production
changes: a zero-target query misses a live `(0, 0)` row; `search_link(0, 0)`
returns `None`; a concrete `(1, 1)` query returns one of two duplicates. Upstream
unit-store mutation skips index insertion for zero endpoints, and its concrete
pair lookup uses a single-result tree search. The adapter must translate these
differences to the existing `LinksStorage` semantics. Its optional filters are
literal, including a value equal to the upstream wildcard constant.

The fix uses at most one usable endpoint index and filters all candidate rows.
Queries containing only unindexed endpoint values scan the store. Ordinary
nonzero pair search keeps its upstream lookup; null/wildcard-value searches scan
without allocating a copy and stop at the first exact match. `get_or_create`
uses that exact search. The 125-pattern oracle compares row sets because index
traversal does not promise address order. Heap checks still leave filesystem
methods and generic instantiation placeholders visible in complete coverage.

All eleven native and twelve actual-WASM contracts pass, plus 24 related
native storage/decorator/observer checks. The actual-WASM file summary remains
255/294 lines and 28/38 branches; no denominator is dropped. See
[verification](DOUBLETS-PORTABLE-VERIFICATION.md) and its receipt for the
isolated immutable negative control, physical-byte check, toolchain and limits.
