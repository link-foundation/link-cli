# Measure optional checkpoint payloads

Build the web-target `clink-wasm` package, then run:

```sh
CLINK_WASM=/absolute/path/to/pkg node examples/journal-checkpoints/measure.mjs
```

The finite example checks exact current/history snapshots, Unicode names,
committed address gaps and following writes using actual WASM. It prints UTF-8
journal payload counts for default versus optional checkpoint publication.
The supplied in-memory atomic store is volatile and single-process; the output
is not a latency, network or disk benchmark. Use the existing cluster examples
with `--checkpoint` for an actual etcd publication recipe. See
[configuration and costs](../../docs/ENGINE-CHECKPOINTS.md).

Measured after 32 revisions of the supplied one-row/name workload, with committed
address gaps introduced at revision three. Counts match on Node, Bun and Deno
and all three selected state structures. Each byte count includes the UTF-8 JSON
array envelope, and excludes transport/disk framing.

| Logical encoding | History | Default records / bytes | Interval four records / bytes |
| --- | --- | --- | --- |
| snapshot | disabled | 32 / 1,606 | 1 / 52 |
| snapshot | last two | 32 / 1,606 | 2 / 103 |
| snapshot | all | 32 / 1,606 | 33 / 1,638 |
| changes | disabled | 32 / 1,724 | 1 / 52 |
| changes | last two | 32 / 1,724 | 2 / 103 |
| changes | all | 32 / 1,724 | 33 / 1,638 |

The complete output also counts bytes read and published over all 32 commits.
Retaining all history can increase current payload size; these measurements do
not predict larger workloads or latency.

`delta.mjs` copies retained checkpoints into physical delta journals with
replication in either composition order. Run it with the same `CLINK_WASM`:

```sh
CLINK_WASM=/absolute/path/to/pkg node examples/journal-checkpoints/delta.mjs
```

Its 36 cases cover three state structures, both logical encodings and three
history policies. Each reopens both replicas, preserves exact history and
Unicode names, and allocates address 41 after committed gaps 33 through 40.
The initial engine3 bytes are unchanged. Following snapshots can use delta1;
logical changes pass through and reset the complete-image compression base.
These volatile copies are independent databases; exporting does not coordinate
their future allocations with the original engine.

For persisted SQL-free pages, build the native examples as described in
[the page recipe](../../docs/ENGINE-NATIVE-PAGES.md), then run:

```sh
CLINK_WASM=/absolute/path/to/pkg bash examples/journal-checkpoints/run-paged-interop.sh
```

This uses dedicated temporary databases. An independently generated checkpoint
starts at revision 40 with 32 rows, committed allocator gaps 33 through 40, and
BOM/Unicode/NUL names. Each configuration passes the same files through Rust,
Node, Bun, C#, Deno and Rust again. Every provider preserves the exact checkpoint,
retained revision 40 and unavailable revision 39, allocates after the gaps, and
commits two independent sessions. Mirrored handoffs damage one closed data and
metadata copy before requiring the next provider to repair it. The full output
is saved under experiments/issue-138/checkpoint-paged-interop.log.

The native examples select this fixture only with
`--checkpoint-fixture=/absolute/path/to/engine-checkpoint-vectors.json`. They
seed an empty logical journal, require an existing journal's first record to
match exactly, and never replace an existing acknowledged prefix. This imports
an independent database; local atomic checkpoint replacement remains separate.

To compact the live engine through the portable two-bank journal, run:

```sh
CLINK_WASM=/absolute/path/to/pkg node examples/journal-checkpoints/compact.mjs
```

The same script runs with Bun or `deno run --allow-all`. Its 72 configurations
combine three state structures, two logical encodings, three retention policies,
optional physical delta compression and single or mirrored page banks. It checks
no-op compaction, pending reservations, pinned history, following commit groups,
fresh image reopening and repair of a damaged closed mirrored copy. Every
configuration finishes with another allocation and commit. The output is
deterministic and compared across all three runtimes in normal CI.

The devices in this example are volatile cloned images. They demonstrate codec
and engine composition, and do not establish filesystem or hardware durability.
Compaction requires exclusive ownership of the root and all banks; see
[publication, ownership and costs](../../docs/protocol/checkpoint-pages.md).
