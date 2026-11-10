# Conflict metadata cost plan

R6 requires paying for selected guarantees. Source inspection finds that open
populates a modification-generation map for every recovered row, and each
commit updates it even under exclusive admission or whole-revision serializable
validation. No session predates opening; those initial entries cannot invalidate
any session. Exclusive admission prevents concurrent publishers, while
serializable validation already rejects any changed database revision.

- [x] Add failing deterministic metadata-cost tests in Rust and C# before edits.
- [x] Avoid recovered-row metadata and generation tracking for modes that do not
      use it; preserve optimistic read-committed/snapshot conflict detection.
- [x] Reclaim generations when the last session ends, after its validation.
- [x] Test overlapping writes, disjoint commits, delete/recreate ABA, branch
      merges, last-session conflict/read-only/rollback/drop and reopening.
- [x] Verify actual WASM behavior through the existing cross-runtime framework.
- [x] Run focused tests, full native suites, formatting/Clippy and coverage.
- [x] Document the exact lifetime and remaining full-state costs; commit/push
      this independently useful optimization to PR 139 and review its CI.

The map must retain tombstones while a possibly stale session remains. An ABA
schedule (delete then recreate with identical values) must still abort a writer
whose base predates those changes. Clearing before validating the last committer
would silently lose that guarantee, so cleanup belongs after its commit attempt.
Branches may publish while sessions are active and must use the same tracking
policy. A session's independent immutable snapshot does not require generations
once the session has ended. Options remain fixed for the lifetime of an engine.

The deterministic before run retains 128 entries for 128 seeded rows: all four
Rust tests and all ten C# cases fail. After the change, opening/replaying and
nonoverlapping writers retain zero; a live conflicting schedule tracks just
its one changed address. Deleting and recreating that row with identical values
still invalidates the stale writer. Read-only completion, conflict, rollback,
drop and branch merge exercise the same lifecycle. C# stores no dictionary until
tracking is required and releases its backing storage when the last session ends.
Rust clears its B-tree nodes at the same boundary. A continually overlapping set
of sessions can still accumulate metadata; this is not oldest-reader pruning.

The full Rust suite passes 431 tests. Actual WASM conformance passes the ABA,
disjoint-write, branch-conflict and reopened-session schedules in Node, Bun and
Deno. The coverage suite runs an additional native-in-WASM schedule (16 total
tests); binding coverage remains complete, while linked-core coverage is not.
See VERIFICATION.md for final native/C# measurements and the PR evidence log.
