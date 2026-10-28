# Optional delta journal plan

Full `engine1` revisions simplify recovery but repeat unchanged rows on every
commit. A storage decorator can reduce journal bytes without changing the
engine/session API or charging bare caches for compression. This is the next
bounded storage strategy in R5/R6/R11; it does not supply an LSM tree.

The source study reads RocksDB `db/log_writer.cc` at
`ea313605367281013c2c81158069e88411516357`: `AddRecord` (87–187) checks writer
errors, optionally compresses, emits fragments, flushes, and advances its sequence
only on success. `EmitPhysicalRecord` (308–359) checksums record type and payload
before appending. We reuse the separation between logical records and physical
encoding, explicit flush and error propagation. We do not copy its block format
or implementation. Primary references: [WAL overview](https://github.com/facebook/rocksdb/wiki/Write-Ahead-Log-%28WAL%29),
[file format](https://github.com/facebook/rocksdb/wiki/Write-Ahead-Log-File-Format),
and [pinned source](https://github.com/facebook/rocksdb/blob/ea313605367281013c2c81158069e88411516357/db/log_writer.cc).

- [x] Reproduce full-revision write amplification with finite, reusable inputs.
- [x] Specify a portable, checksummed delta envelope and independent wire vectors.
- [x] Write failing decorator tests before implementation.
- [x] Implement Rust, C# and universal JS adapters behind the same log contracts.
- [x] Preserve checkpoint fallback, exact logical replay, durability capability,
      single-writer ownership and uncertain-outcome fencing.
- [x] Test corruption, missing/wrong bases, malformed bounds, Unicode names,
      checkpoint intervals, append/flush faults and reopening after errors.
- [x] Run native and actual WASM integrations and measure coverage/byte reduction.
- [x] Document complexity, composition order, examples and recovery limits.

The implemented encoding keeps full `engine1` checkpoints and stores a changed byte span
of their ASCII row payload between checkpoints. Revision/allocator metadata are
separate, so their per-commit changes do not defeat compression. A delta names
the preceding record's checksum, carries bounded prefix/suffix lengths and the
new full record checksum, and has its own checksum. Use a full checkpoint when
the encoded delta is larger or the configured interval expires. Decoding must
validate bounds before allocating or slicing and fail closed on any complete
corrupt record. Normal engine validation still checks recovered rows and policies.

Single-span compression can save bytes for a localized edit but may copy the
span between distant changes. Engine cloning and encoding remain O(N), replay
still processes the retained log, and compression is not compaction. A physical
delta stream requires this decorator when reopening; the underlying log must
never be opened directly as a full-revision engine. Resources and fencing stay
owned by the caller. Cluster full-history transfer costs remain a separate gap.

The finite actual-WASM probe, `experiments/issue-138/journal-amplification.mjs`,
uses 1,000 rows and ten edits to one row. Baseline full records total 109,276
bytes; deltas total 10,697 (90.21% fewer), including the same 9,924-byte first
checkpoint. The last edit uses 77 versus 9,936 bytes. Both recover identical
logical history. These are record bytes, not a latency/throughput benchmark.
Native file-replica tests verify durable flush capability and exact history;
replicas can also compress independently while comparing logical prefixes.
The [wire specification](../../ENGINE-DELTA.md) documents both composition orders.
The independent Python oracle generates five valid sequences and 38 invalid
sequences; it imports none of the implementation. Tests first reproduced the
absent adapter in all three languages, then verified these shared sequences.

Self-review also reproduced a stale-cache error: after successful replay followed
by a failed reread, the initial implementation could append using its old base.
All three adapters now invalidate readiness before rereading. A retry must
validate the complete physical stream before another append; transient read
errors remain retryable. Native and JS regressions verify zero writes while the
stream is unavailable and successful append after recovery.
