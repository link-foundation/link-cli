# Pessimistic point writes

Select this engine-local strategy when competing writers should learn about a
reserved address before staging their write. Independent addresses may proceed
in overlapping sessions. The existing session links/names API is unchanged.

| Surface | Construction option |
| --- | --- |
| Rust | `concurrency: ConcurrencyControl::Pessimistic` |
| C# | `Concurrency = ConcurrencyControl.Pessimistic` |
| JS/TS using actual WASM | `concurrency: 'pessimistic'` |

Create, explicit creation of a missing address, update, delete and name changes
reserve that address until the session ends. A repeated write by the owner is
allowed. Ensuring an already visible row is a no-op and reserves nothing.
Automatic allocation shares the engine allocator, so allocated addresses also
conflict with another session's explicit creation of that address.

A competing write returns `busy` immediately, without waiting or staging that
operation. Earlier successful staged operations remain private in the session.
Roll back if the business operation needs all of those operations retried together.
There is no waiting queue, timeout or deadlock detector: reversed address orders
can return busy to both sessions. An application may roll back and retry with a
new session and its own bounded backoff policy. Failed operations may have read
the requested row; serializable read-set validation retains that dependency.

Acquisition checks changes since begin while holding the publication mutex,
including a delete/recreate that restores an identical row. A stale write fails
with `conflict` before staging. Read committed continues to refresh reads, while
its writes retain the existing begin-revision conflict rule. Snapshot reads
remain repeatable. Serializable validation independently selects whole-revision
or point-read checks; write locks alone do not prevent write skew. A scan still
checks the whole revision under serializable read-set validation.

Every commit attempt closes the session. Success, validation/constraint failure,
rollback, drop/dispose and an unknown journal outcome release its write ownership.
A poisoned engine still requires reopening its authoritative log before use;
releasing locks does not resolve an uncertain commit. Branch merges reject
changes to reserved addresses, while disjoint branch changes may publish.

The strategy allocates an owner and a list of distinct reserved addresses only
for selected sessions. The engine's address-to-owner map allocates entries only
for reserved writes. Rust acquisition uses a BTreeMap (O(log K)); C# uses a
Dictionary (expected O(1)), for K reserved addresses. Release visits the session's
reserved addresses. Other admission modes allocate no ownership entries or
per-session lock tracker. Pessimistic sessions retain per-address generations
while another session could be stale, even with whole-revision serializable
validation, to reject stale writes at acquisition. Final validation/rollback
reclaims generations. Full-state commit and history costs remain unchanged.

These locks belong to one engine shared by its handles. Independent processes,
independently opened engines and raw adapter writes do not participate.
`ClusterEngine` rejects pessimistic admission in every language: a private shard
workspace cannot provide distributed write locks. Its existing atomic coordinator
continues to use optimistic generation validation.

The strategy follows the acquire/validate/release seam observed in pinned
[RocksDB pessimistic transaction source](https://github.com/facebook/rocksdb/blob/ea313605367281013c2c81158069e88411516357/utilities/transactions/pessimistic_transaction.cc#L1151).
This implementation adds no RocksDB dependency or copied implementation, LSM tree,
range locks, waiting/deadlock machinery, savepoints or distributed lock recovery.
See the [source observations](case-studies/issue-138/RESEARCH.md).

Run the existing examples with both flags to show a busy competing write and two
independent serializable commits:

```sh
cargo run --manifest-path rust/Cargo.toml --example engine_strategies -- --pessimistic --read-set
dotnet run --project examples/engine-csharp -- --pessimistic --read-set
node js/client/examples/engine.mjs --pessimistic --read-set
```

Add a journal path and optionally `--delta` for the existing persistent-log recipe.
The JS example requires the existing compiled `js/pkg` WASM package. Test evidence
and limitations are maintained in the [verification record](case-studies/issue-138/VERIFICATION.md).
