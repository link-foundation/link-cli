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

By default a competing write returns `busy` immediately, without waiting or staging
that operation. Earlier successful staged operations remain private in the session.
Roll back if the business operation needs all of those operations retried together.
By default nothing waits: reversed address orders can return busy to both sessions.
An application may roll back and retry with a new session and its own bounded
backoff policy, or select [bounded waiting](#bounded-waiting-and-deadlock-refusal).
Failed operations may have read the requested row; serializable read-set
validation retains that dependency.

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
range locks or distributed lock recovery.
[Savepoints](ENGINE-SAVEPOINTS.md) keep point-write ownership until the session ends.
See the [source observations](case-studies/issue-138/RESEARCH.md).

## Bounded waiting and deadlock refusal

Select a limit to make a competing write wait instead of answering at once. It
needs pessimistic concurrency, and construction refuses it otherwise.

| Surface | Construction option |
| --- | --- |
| Rust (native) | `lock_wait: Some(Duration::from_secs(1))` |
| C# | `LockWait = TimeSpan.FromSeconds(1)` |
| JS/TS using actual WASM | `lockWaitMs: 1000` |

| When the owner of the address... | The waiting write answers |
| --- | --- |
| rolls back, or its session closes | runs, and takes the address |
| commits a change to that address | `conflict`, the same as any stale write |
| does not finish within the limit | `busy` |
| waits for this session, directly or through others | `deadlock`, at once |

Each answer leaves earlier staged writes in the session, so the session stays
usable. After `deadlock`, roll back to let the other session finish: the engine
does not choose a victim for you beyond refusing the write that closed the cycle.
A limit of zero never waits. It differs from no limit only in the busy message.
Group commits wake waiters like ordinary commits. A commit whose journal outcome is
unknown wakes them with the reopen error; a failure outside a commit, such as
compaction, reaches a waiter no later than its limit.

A waiting Rust or C# thread gives up the engine mutex while it waits, so reads,
commits and other sessions continue. It blocks only its own thread. In these two
ports a session is single-threaded, so closing it from another thread during a
wait is misuse: C# then ends the wait with "Transaction is closed", and Rust's
`&mut` borrow rules it out.

JavaScript has one thread, so the Rust core never blocks there. `lockWaitMs` is
implemented by the client, and the wasm `Options` does not accept it: the request
runs inside a savepoint, and if a session refused it, the client restores the
savepoint, yields, and runs the request again when a session of the same engine
ends. That costs one savepoint per request, including reads, with `lockWaitMs`
selected. It has two visible results. A request of several writes applies all or
nothing across the wait, and `signal`/`deadline` can end the wait, with outcome
`not-sent`. The same session's later requests stay queued behind it.

Waiting is bounded and local. There is no fairness queue, so a waiter can lose an
address to a session that arrives later, and each wake rechecks all waiters.
Timeouts and cycles are per engine: sessions of other engines, other processes
and cluster coordinators do not participate. The wait limit applies to each write,
not to the whole session. Without the option an engine carries no wait state, and
the only added cost on acquisition is checking that it is absent. With it, the
engine keeps one edge per waiting session; each refusal walks at most the chain of
waiters behind the owner.

Compared with other engines, as documented by them:

| Engine | Waits | Deadlocks | How to get the same here |
| --- | --- | --- | --- |
| [PostgreSQL](https://www.postgresql.org/docs/current/explicit-locking.html) | Indefinitely unless a [`lock_timeout`](https://www.postgresql.org/docs/current/runtime-config-client.html) applies | Aborts one transaction; which one "should not be relied upon" | Pessimistic plus a long limit. Here the refused write is always the one that closed the cycle, and the session survives until you roll back. |
| [InnoDB](https://dev.mysql.com/doc/refman/8.4/en/innodb-deadlocks.html) | Up to `innodb_lock_wait_timeout` | Detection is on by default and rolls back a victim; with it off, the wait timeout resolves the deadlock | Pessimistic plus a limit. Detection cannot be switched off here; no victim is rolled back automatically. |
| [RocksDB](https://github.com/facebook/rocksdb/blob/main/include/rocksdb/utilities/transaction_db.h) | `transaction_lock_timeout`, 1 second by default; 0 never waits; negative has no timeout | `deadlock_detect` is off by default and returns busy with a bounded traversal depth | Pessimistic plus a limit. Detection is always on and walks the whole chain, at the cost above. |

Row-level locks of PostgreSQL and InnoDB cover ranges and shared modes that these
point-write addresses do not.

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
