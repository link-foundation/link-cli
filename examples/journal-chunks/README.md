# Bounded journal chunk costs

Build the web WASM package, then run the same finite workload in each runtime:

```sh
wasm-pack build --target web --out-dir ../../js/pkg rust/wasm
node --max-old-space-size=256 examples/journal-chunks/measure.mjs
bun examples/journal-chunks/measure.mjs
deno run --allow-read --allow-env examples/journal-chunks/measure.mjs
```

`CLINK_WASM` selects a different built web package directory. The example uses
the actual Rust-backed links engine through WASM, two coordinated shards,
sixteen revisions, retained history and shared immutable rows. It compares
whole-journal writes with explicitly selected two/four-record chunks and a
separate zero/256KiB cache. Every result verifies exact retained revision names,
owned full replay and a following allocator edit.

The JSON reports exact UTF-8 **record payload** bytes at the catalog/data
provider seam, plus request and retained-object counts, for the sixteen commits
and their workspace reads. Later verification is excluded from those counters.
Keys, framing, containers, transport and physical storage are excluded. These
in-memory providers do not claim durability, network throughput or physical
write amplification. The native-file consumers and five-runtime handoffs are
in [the native LSM examples](../native-lsm-js/README.md).

One run of the finite workload produces these payload counts. CI compares the
complete JSON byte for byte across Node, Bun and Deno.

| Layout | Data written | Data read | Data read requests | Catalog written |
| --- | ---: | ---: | ---: | ---: |
| Whole journal, no cache | 16,530 | 31,086 | 94 | 1,038 |
| Two-record chunks, no cache | 2,958 | 31,086 | 304 | 3,924 |
| Two-record chunks, 256KiB cache | 2,958 | 19,232 | 206 | 3,924 |
| Four-record chunks, 256KiB cache | 4,922 | 20,940 | 142 | 2,372 |

All four use sixteen catalog CAS calls, thirty-two data writes and thirty-two
retained immutable objects. Payload bytes exclude the costs listed above;
different workloads and bounds need their own measurements.

Stable complete chunks reduce newly written data payload. Fresh staging still
reads every reference and compares complete contents; the optional read cache
does not weaken that validation. Smaller chunks increase catalog metadata and
can increase requests. This append-only example retains abandoned objects.
Epoch-fenced chunk reclamation, bounded replay and multi-catalog scaling remain
separate required work. See [the format and selected costs](../../docs/protocol/chunked-journal.md).
