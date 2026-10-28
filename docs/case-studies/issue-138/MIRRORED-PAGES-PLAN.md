# Independently selected native page repair

Required extension of NATIVE-PAGES-PLAN.md. Matching Rust, C# and JS/TS
implementations and complete cross-language local validation now pass. This
strategy duplicates a selected native page device. It is not an InnoDB
doublewrite buffer and does not repair legacy mapped Doublets files.

- [x] Specify format mode and independent damaged/stale-copy vectors first.
- [x] Add finite failing JS repair contracts before implementing the decorator.
- [x] Preserve the default single-copy provider and its public file format.
- [x] Implement matching Rust, C# and JS/TS device decorators and native recipes.
- [x] Test every write/sync/repair fault before and after its effect, repeated
      recovery, committed metadata/data corruption, short and stale valid copies,
      both-copy rejection, canonical record validation and following commits.
- [x] Test all language/runtime file interchange, independent policy composition,
      owned handle cleanup and bounded process exits.
- [x] Measure selected byte/read/write/sync costs and complete-run coverage.
- [x] Obtain fresh passing exact-head CI after all matching implementations.
      All nine workflows pass on b0612d0, including both native platform matrices.

Implemented mode: an immutable metadata flag identifies mirrored storage. A plain
reader must reject this mode rather than silently ignore the second copy.
Each native copy retains its own atomic exclusive-create ownership file; a
factory acquires both and never steals a stale owner. Both devices must report
successful data synchronization before either metadata decision is written.
Both decisions must be synchronized before acknowledgement. Single-copy costs
remain available by selecting the existing provider.

Recovery chooses the highest checksummed supported decision among redundant
alternating metadata, rejects identity or equal-generation conflicts, and uses
the root/previous-page checksum chain to select each exact expected data page.
A stale page with a valid local checksum is insufficient. Damage to one copy
can be repaired from the expected valid copy; damage to both rejects open.
An acknowledged metadata copy can survive corruption of its other copy.

No repair may modify either copy before the complete selected prefix and its
canonical doublet/UTF-8/count validation succeed. Track damaged indexes and
expected hashes, then reread the surviving pages and repair in ascending order
with one codec page buffer. The mirror also holds two candidate pages while
selecting the surviving copy. Synchronize repaired data before repairing metadata.
Partial repair failure must preserve the known good copy and require another
reopen. Repeated recovery must be idempotent, with no older acknowledged-group
fallback. Only after committed validation/repair may unpublished tails be cut.

Measured costs: twice the selected data/metadata persistence work and storage,
read validation proportional to history, one codec page buffer plus two candidate
pages and damaged-index tracking, and repair work proportional to damaged pages. Replication remains
separate and retains its own fixed-membership acknowledgement costs. Mirroring
within one failure domain is not consensus, backup or proof against physical
power loss. The persistence model must state which copy/device failures it
covers, and tests must distinguish process exit from device power failure.

JS production evidence: mirrored-pages-js.txt. All 209 tests pass with enforced
100% production line/branch/function coverage in Node 26 and Node 24, strict
TypeScript passes, and 221 repository checks pass. Each of Node/Bun/Deno passes
32 actual-WASM configurations with real-file damage, repair and following commits.
The public exports are independent of the universal root and SQL provider.
Rust production evidence: mirrored-pages-rust.txt. Complete suites pass 922
native checks (17 child entries are exercised by bounded parents) and 539
actual-WASM checks. Strict native/WASM Clippy, formatting and optional-SQL
guards pass. C# passes 15 focused contracts and 2,060 complete tests in each
configuration; see mirrored-pages-csharp.txt. Its initial fresh CI exposed
formatter differences and a Windows-only test read against an exclusive file
owner; mirrored-pages-csharp-ci-before.txt preserves those exact failures.
The correction passes all nine workflows on b0612d0; see
mirrored-pages-parity-ci.txt for exact SHA, timestamps and run URLs.

The shared-file recipe passes all 32 single/mirrored policy configurations and
192 writer invocations through Rust -> Node -> Bun -> C# -> Deno -> Rust. Every
mirrored handoff damages one closed data/metadata copy; each next provider
repairs it and preserves all twelve final revisions and exact prior names.
The same next-record flush after 0/1/32 prior groups performs two new page
writes, two metadata writes and four syncs, with no historical device reads.

Whole-project native Rust/C#/linked-WASM coverage remains below 100%; the
complete denominators are recorded in VERIFICATION.md. All wider acceptance
gaps remain active in REQUIREMENTS.md; this completed representation and its
selected repair strategy do not close legacy repair or other DBMS algorithms.
