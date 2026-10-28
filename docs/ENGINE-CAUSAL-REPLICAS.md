# Selectable local causal ownership

Select Rust's `causal` feature and `engine::causal_replica::CausalReplica`, C#'s
`CausalReplica`, or the independent JS/TS `./engine-causal-replica` export.
Supply a coherent local atomic journal provider and reserve one key for one
unique author lineage. The selected component persists causal mutations,
the author counter and retry receipts together at one local CAS boundary.
It retains concurrent versions and deletion markers for explicit resolution.

```js
import { CausalReplica } from '@link-foundation/link-cli-client/engine-causal-replica';
// atomicStore is a caller-owned coherent provider. Author and tokens are
// unique nonzero 32-character lowercase hex identities chosen by the caller.
const owner = new CausalReplica(atomicStore, 'replica', author,
  { requireDurableFlush: true });
const observed = await owner.read('1');
const version = await owner.write(requestToken, '1', observed,
  { source: '2', target: '3', name: 'application resolution' });
await anotherOwner.receive([version]);
const snapshot = await owner.snapshot();
```

`write` observes only the explicitly supplied versions. Passing an empty list
after a stale read can retain concurrent siblings; observing the current list
and choosing a payload resolves those versions. Passing null is a retained
deletion. Every operation freshly reads and validates the selected local
journal. Snapshots and returned versions own their nested state. Addresses and
endpoint values remain explicit; automatic globally unique address allocation
and an ordinary links projection/session facade remain unfinished.

After a thrown or malformed CAS reply, reread and retry the same request token.
The journal returns its original result if that mutation committed, including
after reopening and later edits. Changing a token's normalized request rejects.
A CAS false result reports a conflict. The component performs one CAS and no
automatic mutation retries. Two handles on one local coherent provider cannot
successfully publish the same author counter. Independent writable stores must
use different authors; backup rollback needs a new author or external fencing.

Writes validate and retain every supplied observation, including dominated
versions, before constructing the merged context. Reopening therefore still
rejects a contradictory older payload. Receipt retries also validate their
complete input before returning an existing result.

The caller owns and closes the physical provider. Rust returns it through
`into_store`; C# and JS retain the supplied provider. Selecting
`requireDurableFlush` rejects providers that do not advertise durable storage.
The SQL-free `FileLsmJournalStore` can supply that boundary on supported native
platforms. Its existing directory leases, constructor validation, uncertain
cleanup and platform capability rules still apply. An acknowledged journal CAS
is authoritative; a fresh open discovers it after a lost response.

The [wire protocol](protocol/causal-replica-v1.md) specifies shared canonical
records, exact counters and retained historical identity checks. Defaults and
hard maxima are 4096 entries and 8 MiB of ASCII record bytes. Each record has
an additional 1 MiB, five-level and 32768-separator preparse bound. Capacity exhaustion
rejects before CAS. Smaller selected limits and the underlying provider's own
admission bounds also apply. Replay retains historical dots and retry receipts;
worst-case history comparison costs O(H²W), with O(HW + R) retained space for H
versions, R records and maximum version size W. Each publication copies the
selected journal. It is a bounded initial strategy; unbounded operation,
indexed history and safe compaction/reclamation remain unfinished acceptance criteria.

Run the shared physical scenarios after building the native example with
`native-pages,physical-lsm,causal` and the C# cluster example:

```sh
NATIVE_LSM_LAYOUT=causal python3 examples/native-lsm-js/run-interop.py
python3 examples/native-lsm-js/run-causal-faults.py
```

The finite process experiment explicitly reaps each exited writer, verifies
that every runtime refuses stale ownership without changing bytes, and
quarantines the ownership markers before operator-authorized recovery. It
checks before/after publication, durable retries and following edits. Such
application process exits do not certify hardware power-loss behavior. The
scripts' actual validation status is recorded in the issue's verification log.

Local atomic publication and causal reconciliation are separate guarantees.
Global uniqueness, cascades, serializability and consensus require their
coordinated strategies. [Merkle snapshots/range transfer](ENGINE-CAUSAL-MERKLE.md) is a separate selected
component. Automatic repair and safe evidence reclamation remain under the [anti-entropy plan](case-studies/issue-138/ANTI-ENTROPY-PLAN.md).
