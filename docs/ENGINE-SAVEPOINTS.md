# Savepoints

A savepoint lets a staged session undo part of its work and carry on, instead of
abandoning the whole transaction. It is available on every staged session and on
each shard of a cluster transaction.

```rust
let mut session = engine.begin()?;
session.create_link(0, 0)?;
let savepoint = session.savepoint()?;
session.create_link(0, 0)?;
session.rollback_to_savepoint(savepoint)?; // Only the first link remains.
session.commit()?;
```

```csharp
using var session = engine.Begin();
session.CreateLink(0, 0);
var savepoint = session.Savepoint();
session.CreateLink(0, 0);
session.RollbackTo(savepoint); // Only the first link remains.
session.Commit();
```

```js
const session = engine.begin();
await session.create(0n, 0n);
const savepoint = await session.savepoint();
await session.create(0n, 0n);
await session.rollbackTo(savepoint); // Only the first link remains.
await session.commit();
```

## What each operation does

| Operation | Rust / C# / JS | Effect |
| --- | --- | --- |
| Take | `savepoint` / `Savepoint` / `savepoint` | Remembers the staged writes and returns a token. Savepoints nest. |
| Roll back to | `rollback_to_savepoint` / `RollbackTo` / `rollbackTo` | Restores those writes and removes later savepoints. This savepoint stays usable. |
| Release | `release_savepoint` / `ReleaseSavepoint` / `releaseSavepoint` | Keeps all writes and removes this savepoint and later ones. |

A token from another session, or one that was already removed, is refused with
`unknown savepoint` and changes nothing. All three operations fail on a closed
session. Committing or rolling back the session removes its savepoints.

## What rolling back does not undo

Only staged rows and names are restored. The rest stays as it was, on purpose:

- **Observed reads stay validated.** Later work may depend on what the session
  read, so serializable validation still checks reads made before the rollback.
- **Point-write ownership stays until the session ends.** With pessimistic
  concurrency, an address written after the savepoint stays busy for others.
- **Allocated addresses stay retired.** A discarded create leaves a gap, as a
  complete rollback does.
- **Query reservations stay consumed.** A reserved address used by a discarded
  create is not handed out again.

A write that was discarded no longer takes part in write-conflict validation,
because it will not be published. A session rolled back to an empty write set
commits without publishing a revision.

## Cost

Sessions that never take a savepoint pay nothing beyond one empty list. Taking a
savepoint copies the staged write set, so it costs O(staged writes) in time and
memory, and so does rolling back to it. Taking a savepoint before each of `n`
writes therefore costs O(n²). Release savepoints you no longer need.

## Limits

- A savepoint is private to one session and is not recorded in the journal.
- On a cluster transaction, a savepoint covers one shard. It does not cover the
  group, so take one on each shard that needs it.
- JS tokens are `bigint` values from `savepoint`. Sessions from older bindings
  refuse all three calls with `UnsupportedError`.

## How this compares

| System | Same | Different |
| --- | --- | --- |
| [SQLite](https://www.sqlite.org/lang_savepoint.html) | Rolling back keeps the transaction open and the savepoint on the stack. Release removes it and later ones. | Savepoints are named, and a savepoint outside a transaction starts one. Tokens are used here, and a session is always required. |
| [PostgreSQL](https://www.postgresql.org/docs/current/sql-rollback-to.html) | The savepoint stays valid and later ones are destroyed. | PostgreSQL names savepoints. |
| [RocksDB](https://github.com/facebook/rocksdb/blob/ea313605367281013c2c81158069e88411516357/utilities/transactions/pessimistic_transaction.cc) | Savepoints are available on pessimistic transactions. | RocksDB releases locks taken after the savepoint. Ownership is kept here, so others stay busy for longer. |
