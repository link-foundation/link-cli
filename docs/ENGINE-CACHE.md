# A store kept as a cache

`CacheLinks` makes any links store behave as a cache. Links can come due after a
time, and the store can be kept to a number of links by evicting what was used
least recently. It has the same API as the store it wraps, and a store that does
not select it pays nothing.

```rust
use link_cli::{CacheLinks, CacheOptions, LinkStorage, NamedTypeLinks};

let options = CacheOptions { max_links: Some(1000), time_to_live: Some(60_000) };
let mut cache = CacheLinks::new(LinkStorage::in_memory(false), options);
let link = cache.create(1, 1); // Comes due in a minute.
cache.persist(link)?; // Now it never does.
cache.expire(link, 5_000)?; // Now it does in five seconds.
assert!(cache.time_to_live(link)?.is_some());
```

```csharp
var cache = new CacheLinksDecorator<uint>(store, new CacheOptions(MaxLinks: 1000, TimeToLive: 60_000));
var link = cache.CreateAndUpdate(1u, 1u); // Comes due in a minute.
cache.Persist(link); // Now it never does.
cache.Expire(link, 5_000); // Now it does in five seconds.
Assert.NotNull(cache.TimeToLive(link));
```

```js
const cache = new CacheLinks(links, { maxLinks: 1000, timeToLive: 60_000 });
const { index } = await cache.create(1n, 1n); // Comes due in a minute.
await cache.persist(index); // Now it never does.
await cache.expire(index, 5_000); // Now it does in five seconds.
assert.notEqual(await cache.timeToLive(index), null);
```

Both options are optional, and with neither the cache only forwards and tracks.
Times are milliseconds. All three languages accept a clock, for tests or for a
logical time: `CacheLinks::with_clock` in Rust, the `clock` argument in C# and the
`clock` option in JS. WebAssembly has no system clock, so use `with_clock` there.

## What each operation does

| Operation | Rust / C# / JS | Effect |
| --- | --- | --- |
| Give a deadline | `expire` / `Expire` / `expire` | Replaces any earlier deadline. Zero deletes the link now. False when there is no such link. |
| Take it away | `persist` / `Persist` / `persist` | False when the link had none. |
| Read it | `time_to_live` / `TimeToLive` / `timeToLive` | The milliseconds left. Empty when there is no deadline or no link. |
| Delete what is due | `purge_expired` / `PurgeExpired` / `purgeExpired` | Returns how many links went, cascades included. |

The `time_to_live` option applies only to links created through the cache. Links
already in the store have no deadline and count as the least recently used, in the
order of their addresses.

## The two rules differ on purpose

**A deadline is a promise.** A link that comes due is deleted with whatever the
store's own delete takes with it, so links built on it may go too.

**Capacity only has to free a place.** It evicts the least recently used link that
no other link refers to, and takes nothing else along. It never evicts the source
or target of the link being written. When nothing can go, the write is refused and
the store stays as it was. In JS the refusal is a `LimitExceededError` with outcome
`not-sent`. In C#, a link is made in two steps, so capacity is kept when `Update`
says what the link refers to, or by the next write if that never comes. Until then
the new link is counted over the limit, and a refused link is deleted again.

A point read or write counts as use. A scan or a count does not, so a report over
the whole store does not change what gets evicted.

## When a link goes

There is no worker and no timer, so a cache costs nothing while nobody calls it.
Every call deletes what is due before it answers, so a caller going through the
cache never sees a link past its deadline. To free space while nobody is calling,
run `purge_expired` on your own timer.

A failed delete of something due is returned by Rust's `purge_expired`, `expire`,
`persist` and `time_to_live`. Rust reads have no way to report a failure, so they
panic, as the store's own `create` does when it fails.

## Costs

The decorator keeps one entry per link and a usage count per referred address, in
memory. Opening it reads the whole store once: at construction in Rust and C#, at
the first call in JS. Each point operation adds O(log n) to keep two ordered sets
(Rust, C#) or sorted arrays with O(n) insertion (JS).

## Limits

- **Deadlines live in memory.** A store opened again has links without deadlines,
  and its order of use starts over.
- **All writes must pass through the cache.** Another writer to the same store is
  not followed. A link the store below makes by itself is read but not counted.
- **It is not synchronized in Rust or C#.** Use one instance from one thread at a
  time. JS calls are served in turn.
- **JS refuses `request`, `query` and triplets**, with `UnsupportedError`, because a
  cache cannot follow writes it cannot see. `batch` works but is not atomic.
- **The limit is links, not bytes.**
- **The clock is trusted.** A step back delays what comes due.

A cache with a time limit is not a transaction log. For durable or isolated data,
see [the strategies](ENGINE-STRATEGIES.md).

## Compared with Redis

Checked against the Redis pages for
[`EXPIRE`](https://redis.io/docs/latest/commands/expire/) and
[key eviction](https://redis.io/docs/latest/develop/reference/eviction/).

| Redis | Here | Difference |
| --- | --- | --- |
| `EXPIRE key seconds` | `expire(link, milliseconds)` | Same replacement of an earlier deadline. A zero deletes in both; a negative time is refused here, where Redis deletes. No `NX`, `XX`, `GT`, `LT`. |
| `PERSIST` | `persist` | Same. |
| `TTL` | `time_to_live` | Redis tells "no deadline" and "no key" apart with `-1` and `-2`. Here both are empty. |
| Passive expiry on access, plus periodic sampling | Every call deletes everything due, and `purge_expired` | No background work, so nothing is freed while nobody calls. |
| Absolute timestamps kept in RDB and AOF | Memory only | Deadlines do not survive a restart here. |
| `maxmemory` in bytes | `max_links` in links | Bytes are not measured. |
| `allkeys-lru`, approximated by sampling | Exact order of use | Exact, at the cost of more memory per link. In-use links are skipped, because a link has others built on it where a key does not. |
| `noeviction` errors on writes | The same refusal, only when nothing can be evicted | Eviction cannot be turned off separately: leave `max_links` empty to have no limit. |
| `allkeys-lfu`, `allkeys-lrm`, `allkeys-random`, `volatile-*` | Not offered | Only least recently used. |
