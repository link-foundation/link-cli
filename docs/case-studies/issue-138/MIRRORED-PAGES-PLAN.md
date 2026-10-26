# Independently selected native page repair

Required extension of NATIVE-PAGES-PLAN.md; matching Rust/C# implementation
and complete cross-language evidence remain open. This strategy duplicates a selected native page device. It is not an
InnoDB doublewrite buffer and does not repair legacy mapped Doublets files.

- [x] Specify format mode and independent damaged/stale-copy vectors first.
- [x] Add finite failing JS repair contracts before implementing the decorator.
- [x] Preserve the default single-copy provider and its public file format.
- [ ] Implement matching Rust, C# and JS/TS device decorators and native recipes.
- [ ] Test every write/sync/repair fault before and after its effect, repeated
      recovery, committed metadata/data corruption, short and stale valid copies,
      both-copy rejection, canonical record validation and following commits.
- [ ] Test all language/runtime file interchange, independent policy composition,
      owned handle cleanup and bounded process exits.
- [ ] Measure selected byte/read/write/sync costs, complete coverage and fresh CI.

Proposed mode: an immutable metadata flag identifies mirrored storage. A plain
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
with one page buffer. Synchronize repaired data before repairing metadata.
Partial repair failure must preserve the known good copy and require another
reopen. Repeated recovery must be idempotent, with no older acknowledged-group
fallback. Only after committed validation/repair may unpublished tails be cut.

Expected costs: twice the selected data/metadata persistence work and storage,
read validation proportional to history, one page buffer plus damaged-index
tracking, and repair work proportional to damaged pages. Replication remains
separate and retains its own fixed-membership acknowledgement costs. Mirroring
within one failure domain is not consensus, backup or proof against physical
power loss. The persistence model must state which copy/device failures it
covers, and tests must distinguish process exit from device power failure.

JS production evidence: mirrored-pages-js.txt. All 209 tests pass with enforced
100% production line/branch/function coverage in Node 26 and Node 24, strict
TypeScript passes, and 221 repository checks pass. Each of Node/Bun/Deno passes
32 actual-WASM configurations with real-file damage, repair and following commits.
The public exports are independent of the universal root and SQL provider.
The remaining language/interop boxes above are intentionally still open.
