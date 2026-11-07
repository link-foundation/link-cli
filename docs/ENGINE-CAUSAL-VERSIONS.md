# Selectable causal version semantics

Select Rust's `causal` feature, C#'s `CausalVersion` type, or the independently
imported JS/TS `@link-foundation/link-cli-client/engine-causal-version` module.
This component encodes, validates, merges and constructs causal row versions;
it performs no storage I/O. It is the first semantic component of the ongoing
[anti-entropy implementation](case-studies/issue-138/ANTI-ENTROPY-PLAN.md).

```js
import { CausalVersion } from '@link-foundation/link-cli-client/engine-causal-version';
const alice = '00000000000000000000000000000001';
const bob = '00000000000000000000000000000002';
const first = CausalVersion.advance('1', alice, '0', [],
  { source: '2', target: '3', name: 'original' });
const edit = CausalVersion.advance('1', alice, '1', [first],
  { source: '4', target: '5', name: 'edited' });
const deletion = CausalVersion.advance('1', bob, '0', [first], null);
const siblings = CausalVersion.merge([edit], [deletion]); // keeps both
const resolved = CausalVersion.advance('1', alice, '2', siblings,
  { source: '4', target: '5', name: 'application resolution' });
const wire = CausalVersion.encode(resolved);
```

The counters above belong to a finite in-memory example. A real store must
atomically persist its author counter and mutation, fence duplicate active
owners, and resolve uncertain publication before publishing a later mutation.
Supplying a counter to `advance` does not establish those storage guarantees.
The result uses one more than the larger supplied/observed local counter;
overflow rejects, without mutating the inputs. Joining observed siblings
resolves their causality; the application chooses the resulting payload.

Rust exposes owned `CausalVersion`, `CausalDot` and `CausalPayload` values in
`link_cli::engine::causal`; `encode`, `decode`, `merge` and `advance` return
`Result`. C# exposes the corresponding records under
`Foundation.Data.Doublets.Cli.Engine`, instance `Encode()` and static `Decode`,
`Merge` and `Advance` methods. Rust/C# use u64/ulong integers; JS/TS uses
canonical decimal strings so no address or counter loses precision.

The [shared protocol](protocol/causal-version-v1.md) bounds records, names,
authors and sibling counts, retains deletions, and distinguishes absent and
empty names. Merge validates all supplied identities before dropping dominated
versions. It returns owned values in deterministic dot order. It rejects a
contradictory dot/context/payload or equal-clock operation presented together,
even when a later supplied version dominates that contradiction. Contradictions
whose evidence has already been discarded require a separate lineage protocol;
unique, honest author ownership remains an explicit assumption.

With at most 128 input versions and 64 authors per clock, the portable
implementations have a conservative O(V² A²) comparison bound and retain
O(V(A + N)) owned input/output data, where N is admitted name bytes. Validation
also serializes bounded records. Selected Rust builds add the JSON dependency;
the bare engine feature set and unimported JS entry point incur no causal
metadata, hashing, locking, network or I/O cost. C# initializes codec state only
when this type is used. These are bounded semantic operations, not a claim of
optimal replica-transfer complexity.

Concurrent update and deletion stay as siblings until observed resolution.
Ordinary one-value links reads still need an explicit projection policy; global
serializability, uniqueness and cascade constraints retain their coordinated
engine. Durable causal storage, author fencing, snapshot/Merkle comparison,
bounded range transfer, automatic reclamation and process recovery remain
unfinished. Fixed-primary replication and global atomic journal stores keep
their existing APIs and guarantees.
