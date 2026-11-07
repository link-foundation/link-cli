// A links network kept as a cache (issue #138): the first cases are the twins of
// rust/tests/cache_links_tests.rs and of the C# CacheLinksDecoratorTests.cs; the
// rest cover what only JavaScript has (calls in turn, outcomes that are unknown).

import assert from 'node:assert/strict';
import test from 'node:test';
import {
  AbortError,
  AddressRangeError,
  ANY,
  CacheLinks,
  LimitExceededError,
  LinksDecorator,
  RemoteError,
  UnsupportedError,
} from '../src/index.js';
import { all, make, MemoryLinks } from './memory-links.mjs';

function world(options = {}, memory = new MemoryLinks()) {
  const time = { now: 1000 };
  return { memory, time, cache: new CacheLinks(memory, { ...options, clock: () => time.now }) };
}

const addresses = async (links) => (await links.each()).map(({ index }) => index);
/** What is there, by source: the store below reuses the addresses it freed. */
const sources = async (links) =>
  (await links.each()).map(({ source }) => source).sort((a, b) => (a < b ? -1 : 1));

/** A store that fails a write with `failure`, once it is told to. */
class Failing extends LinksDecorator {
  failure = null;

  #fail() {
    const failure = this.failure;
    this.failure = null;
    if (failure) throw failure;
  }

  async delete(index, options) {
    this.#fail();
    return this.links.delete(index, options);
  }

  async create(source, target, options) {
    this.#fail();
    return this.links.create(source, target, options);
  }

  async update(index, source, target, options) {
    this.#fail();
    return this.links.update(index, source, target, options);
  }
}

test('a cache with no options is the store below', async () => {
  const { cache, time, memory } = world();
  const link = await cache.create(1n, 1n);
  time.now = Number.MAX_SAFE_INTEGER;
  assert.deepEqual(await addresses(cache), [link.index]);
  assert.equal(await cache.timeToLive(link.index), null);
  assert.equal(await cache.purgeExpired(), 0);
  assert.equal(cache.links, memory);
});

test('options are checked', () => {
  const memory = new MemoryLinks();
  for (const options of [
    { maxLinks: -1 },
    { maxLinks: 1.5 },
    { maxLinks: '1' },
    { timeToLive: -1 },
    { timeToLive: Infinity },
    { timeToLive: 1n },
    { clock: 1 },
  ])
    assert.throws(() => new CacheLinks(memory, options), TypeError);
  assert.ok(new CacheLinks(memory));
});

test('the system clock reads now, so a long deadline has not passed', async () => {
  const cache = new CacheLinks(new MemoryLinks());
  const { index } = await cache.create(1n, 1n);
  assert.equal(await cache.expire(index, 3_600_000), true);
  assert.ok((await cache.timeToLive(index)) > 3_000_000);
});

test('a link goes at its deadline and not before', async () => {
  const { cache, time } = world({ timeToLive: 10 });
  const { index } = await cache.create(1n, 1n);
  assert.equal(await cache.timeToLive(index), 10);
  time.now += 9;
  assert.equal(await cache.timeToLive(index), 1);
  assert.equal(await cache.purgeExpired(), 0);
  time.now += 1;
  assert.equal(await cache.purgeExpired(), 1);
  assert.equal(await cache.timeToLive(index), null);
  assert.deepEqual(await addresses(cache), []);
});

test('every read and write sees what came due as gone', async () => {
  const reads = [
    (cache) => cache.count(),
    (cache) => cache.each(),
    (cache) => cache.create(9n, 9n),
    (cache, index) => cache.getName(index),
    (cache) => cache.getByName('a'),
    (cache, index) => cache.setName(index, 'b'),
    (cache, index) => cache.removeName(index),
    (cache) => cache.persist(9n),
    (cache) => cache.expire(9n, 1),
    (cache) => cache.delete(77n).catch(() => {}),
    (cache) => cache.update(77n, 1n, 1n).catch(() => {}),
  ];
  for (const read of reads) {
    const { cache, time, memory } = world({ timeToLive: 5 });
    const { index } = await cache.create(1n, 1n);
    await cache.setName(index, 'a');
    time.now += 5;
    await read(cache, index);
    assert.equal(memory.links.has(index) && (await memory.getByName('a')) === index, false, `${read}`);
  }
});

test('the rest of the contract answers from the store below', async () => {
  const { cache, memory } = world();
  const { index } = await cache.create(1n, 1n);
  assert.equal(await cache.setName(index, 'a'), index);
  assert.equal(await cache.getName(index), 'a');
  assert.equal(await cache.getByName('a'), index);
  await cache.removeName(index);
  assert.equal(await cache.getName(index), null);
  assert.equal(await cache.count(), 1n);
  memory.exportLinks = async () => '(1: 1 1)';
  assert.equal(await cache.exportLinks(), '(1: 1 1)');
});

test('a deadline can be replaced, taken away and refused for what is absent', async () => {
  const { cache, time } = world();
  const { index } = await cache.create(1n, 1n);
  assert.equal(await cache.expire(77n, 5), false);
  assert.equal(await cache.persist(77n), false);
  assert.equal(await cache.persist(index), false);
  assert.equal(await cache.timeToLive(77n), null);

  assert.equal(await cache.expire(index, 5), true);
  assert.equal(await cache.expire(index, 50), true);
  assert.equal(await cache.timeToLive(index), 50);
  time.now += 5;
  assert.equal(await cache.count(), 1n);
  assert.equal(await cache.persist(index), true);
  assert.equal(await cache.timeToLive(index), null);
  time.now += 500;
  assert.equal(await cache.count(), 1n);
});

test('what is no address or no time is refused', async () => {
  const { cache } = world();
  await assert.rejects(cache.expire(1n, -1), TypeError);
  await assert.rejects(cache.expire(1n, 1n), TypeError);
  await assert.rejects(cache.expire(-1, 1), AddressRangeError);
  await assert.rejects(cache.persist(-1), AddressRangeError);
  await assert.rejects(cache.timeToLive(-1), AddressRangeError);
  await assert.rejects(cache.count(1), TypeError);
  await assert.rejects(cache.each(1), TypeError);
  await assert.rejects(cache.create(-1, 1), AddressRangeError);
});

test('an expiry of zero deletes at once', async () => {
  const { cache } = world();
  const { index } = await cache.create(1n, 1n);
  assert.equal(await cache.expire(index, 0), true);
  assert.equal(await cache.count(), 0n);
});

test('what comes due takes along what is built on it and counts it', async () => {
  const { cache, time } = world();
  const base = await cache.create(1n, 1n);
  const built = await cache.create(base.index, base.index);
  const other = await cache.create(3n, 3n);

  await cache.expire(base.index, 5);
  await cache.expire(built.index, 50);
  time.now += 5;
  assert.equal(await cache.purgeExpired(), 2);
  assert.deepEqual(await addresses(cache), [other.index]);

  // The deadline of what went along went too.
  time.now += 100;
  assert.equal(await cache.purgeExpired(), 0);
});

test('deadlines that tie go in the order they were given', async () => {
  const { cache, time } = world();
  const [first, second, third] = [
    await cache.create(1n, 1n),
    await cache.create(2n, 2n),
    await cache.create(3n, 3n),
  ];
  await cache.expire(second.index, 5);
  await cache.expire(first.index, 5);
  await cache.expire(third.index, 3);
  time.now += 3;
  assert.equal(await cache.purgeExpired(), 1);
  // Replacing one of the tie finds it among them.
  await cache.expire(second.index, 2);
  time.now += 2;
  assert.equal(await cache.purgeExpired(), 2);
});

test('capacity evicts the least recently used link', async () => {
  const { cache } = world({ maxLinks: 2 });
  const first = await cache.create(1n, 1n);
  const second = await cache.create(2n, 2n);
  // A read by address uses it, and a scan or a count does not.
  await cache.each([first.index]);
  await cache.each();
  await cache.each([ANY, ANY, 2n]);
  await cache.count([second.index]);
  await cache.create(3n, 3n);
  assert.deepEqual(await sources(cache), [1n, 3n]);

  // A read by source and target uses it as well.
  await cache.each([ANY, 1n, 1n]);
  await cache.create(4n, 4n);
  assert.deepEqual(await sources(cache), [1n, 4n]);
});

test('updating a link counts as using it', async () => {
  const { cache } = world({ maxLinks: 2 });
  const first = await cache.create(1n, 1n);
  await cache.create(2n, 2n);
  await cache.update(first.index, 5n, 5n);
  await cache.create(3n, 3n);
  assert.deepEqual(await sources(cache), [3n, 5n]);
});

test('a link in use is not evicted and a full cache refuses', async () => {
  const { cache } = world({ maxLinks: 2 });
  const base = await cache.create(1n, 1n);
  const user = await cache.create(base.index, base.index);
  // The user is the one to go, and what it used is free again.
  await cache.create(3n, 3n);
  assert.deepEqual(await sources(cache), [1n, 3n]);

  await cache.delete(user.index);
  await cache.create(base.index, base.index);
  await assert.rejects(cache.create(base.index, 2n), (error) => {
    assert.ok(error instanceof LimitExceededError);
    assert.equal(error.outcome, 'not-sent');
    assert.match(error.message, /holds 2 links/);
    return true;
  });
  assert.equal(await cache.count(), 2n);
});

test('what a new link refers to is not evicted for it', async () => {
  const { cache } = world({ maxLinks: 2 });
  const first = await cache.create(1n, 1n);
  await cache.create(2n, 2n);
  const user = await cache.create(first.index, 9n);
  assert.deepEqual(await addresses(cache), [first.index, user.index]);
  assert.equal(user.target, 9n);
});

test('a link freed of its users can be evicted again, in the order it was used', async () => {
  const { cache } = world({ maxLinks: 3 });
  const base = await cache.create(1n, 1n);
  const user = await cache.create(base.index, base.index);
  await cache.create(3n, 3n);
  await cache.delete(user.index);
  await cache.create(4n, 4n);
  await cache.create(5n, 5n);
  // `base` was used before the others, and being freed does not make it new.
  assert.deepEqual(await sources(cache), [3n, 4n, 5n]);
});

test('updating moves what a link uses', async () => {
  const { cache } = world({ maxLinks: 3 });
  const [old, fresh] = [await cache.create(1n, 1n), await cache.create(2n, 2n)];
  const user = await cache.create(old.index, old.index);
  await cache.update(user.index, fresh.index, 6n);
  await cache.create(4n, 4n);
  assert.deepEqual(await sources(cache), [fresh.index, 2n, 4n].sort((a, b) => (a < b ? -1 : 1)));
});

test('a link two others use is held until both are gone', async () => {
  const { cache } = world({ maxLinks: 3 });
  const base = await cache.create(1n, 1n);
  const one = await cache.create(base.index, 7n);
  const two = await cache.create(8n, base.index);
  await cache.delete(one.index);
  await cache.create(9n, 9n);
  await cache.each([two.index]);
  // `base` is the least recently used, but `two` still uses it: `(9 9)` goes.
  await cache.create(10n, 10n);
  assert.deepEqual(await sources(cache), [1n, 8n, 10n]);

  await cache.delete(two.index);
  await cache.create(11n, 11n);
  await cache.create(12n, 12n);
  assert.deepEqual(await sources(cache), [10n, 11n, 12n]);
});

test('a link naming itself is no use of itself', async () => {
  const { cache } = world({ maxLinks: 1 });
  const first = await cache.create(1n, 1n);
  assert.equal(first.index, 1n);
  const second = await cache.create(2n, 2n);
  assert.deepEqual(await addresses(cache), [second.index]);
});

test('a cache holding nothing refuses everything', async () => {
  const { cache } = world({ maxLinks: 0 });
  await assert.rejects(cache.create(1n, 1n), LimitExceededError);
  assert.equal(await cache.count(), 0n);
});

test('links already there are the first to go and have no deadline', async () => {
  const memory = new MemoryLinks();
  const first = await make(memory, 1n, 1n);
  const second = await make(memory, 2n, 2n);
  const { cache, time } = world({ maxLinks: 2, timeToLive: 5 }, memory);
  assert.equal(await cache.timeToLive(first), null);
  const third = await cache.create(3n, 3n);
  assert.deepEqual(await addresses(cache), [second, third.index].sort());
  time.now += 5;
  assert.deepEqual(await addresses(cache), [second]);
});

test('an update that merges into a duplicate is followed', async () => {
  const memory = new MemoryLinks();
  const update = memory.update.bind(memory);
  memory.update = async (index, source, target) => {
    const changes = await update(index, source, target);
    memory.links.delete(index);
    return [...changes, { before: changes[0].after, after: { index: 0n, source: 0n, target: 0n } }];
  };
  const { cache } = world({ maxLinks: 2 }, memory);
  const { index } = await cache.create(1n, 1n);
  assert.equal((await cache.update(index, 2n, 2n)).length, 2);
  assert.equal(await cache.expire(index, 5), false);
});

test('a store that keeps links unique answers with one it has', async () => {
  const memory = new MemoryLinks();
  const create = memory.create.bind(memory);
  memory.create = async (source, target) => {
    const [found] = await memory.each([ANY, source, target]);
    return found ?? create(source, target);
  };
  const { cache } = world({ maxLinks: 3 }, memory);
  await cache.create(1n, 1n);
  await cache.create(2n, 2n);
  await cache.create(1n, 1n);
  await cache.create(3n, 3n);
  await cache.create(4n, 4n);
  // The one asked for again was used last.
  assert.deepEqual(await sources(cache), [1n, 3n, 4n]);
});

test('links the store below made itself are read and not counted', async () => {
  const { cache, memory } = world({ maxLinks: 5 });
  const link = await cache.create(1n, 1n);
  const made = await make(memory, link.index, link.index);
  assert.equal(await cache.expire(made, 5), false);
  // The update of a link never counted, and what its delete reports, change nothing.
  await cache.update(made, link.index, 3n);
  await cache.delete(link.index);
  assert.deepEqual(await addresses(cache), []);
});

test('links reported as null links are no links', async () => {
  const memory = new MemoryLinks();
  const remove = memory.delete.bind(memory);
  memory.delete = async (index) =>
    (await remove(index)).map(({ before }) => ({
      before,
      after: { index: 0n, source: 0n, target: 0n },
    }));
  const { cache, time } = world({ timeToLive: 1 }, memory);
  await cache.create(1n, 1n);
  time.now += 1;
  assert.equal(await cache.purgeExpired(), 1);
});

test('calls run in turn and a failure does not stop the next', async () => {
  const failing = new Failing(new MemoryLinks());
  const { cache } = world({ maxLinks: 1 }, failing);
  failing.failure = new RemoteError('refused');
  const [refused, made] = await Promise.allSettled([
    cache.create(1n, 1n),
    cache.create(2n, 2n),
  ]);
  assert.equal(refused.status, 'rejected');
  assert.equal(made.status, 'fulfilled');
  const [other, count] = await Promise.all([cache.create(3n, 3n), cache.count()]);
  assert.equal(other.source, 3n);
  assert.equal(count, 1n);
});

test('an unknown outcome reads the store again and keeps the deadlines', async () => {
  const failing = new Failing(new MemoryLinks());
  const { cache, time } = world({}, failing);
  const kept = await cache.create(1n, 1n);
  const used = await cache.create(2n, 2n);
  const user = await cache.create(used.index, used.index);
  const gone = await cache.create(4n, 4n);
  await cache.expire(kept.index, 50);
  await cache.expire(gone.index, 50);

  // The delete got through, but the answer did not.
  const remove = failing.links.delete.bind(failing.links);
  failing.delete = async (index) => {
    delete failing.delete;
    await remove(index);
    throw new AbortError('aborted while it ran', { outcome: 'unknown' });
  };
  await assert.rejects(cache.delete(gone.index), AbortError);

  assert.equal(await cache.timeToLive(gone.index), null);
  const stray = await failing.links.create(9n, 9n);
  await failing.links.update(stray.index, 9n, 9n);
  assert.equal(await cache.timeToLive(kept.index), 50);

  // A link the failed request may have made arrives without a deadline.
  const seen = await failing.links.create(10n, 10n);
  failing.delete = async (index) => {
    delete failing.delete;
    await remove(index);
    throw new AbortError('aborted while it ran', { outcome: 'unknown' });
  };
  await assert.rejects(cache.delete(stray.index), AbortError);
  assert.equal(await cache.timeToLive(seen.index), null);
  assert.equal(await cache.expire(seen.index, 5), true);

  // What counts as used was read again as well.
  await cache.delete(user.index);
  time.now += 50;
  assert.equal(await cache.purgeExpired(), 2);
  assert.deepEqual(await addresses(cache), [used.index]);
});

test('a failure with no outcome counts as unknown, and a failed read leaves it so', async () => {
  const failing = new Failing(new MemoryLinks());
  const { cache } = world({}, failing);
  await cache.create(1n, 1n);
  failing.failure = new Error('not a links error');
  await assert.rejects(cache.create(2n, 2n), /not a links error/);

  const each = failing.links.each.bind(failing.links);
  failing.links.each = async () => {
    failing.links.each = each;
    throw new RemoteError('no read');
  };
  await assert.rejects(cache.count(), RemoteError);
  assert.equal(await cache.count(), 1n);

  failing.failure = null;
  await assert.rejects(cache.update(77n, 1n, 1n), RemoteError);
  failing.failure = undefined;
  await assert.rejects(
    new CacheLinks({
      capabilities: failing.capabilities,
      each: async () => [],
      create: async () => {
        throw null;
      },
    }).create(1n, 1n),
    (error) => error === null,
  );
});

test('the capabilities say what the cache offers', async () => {
  const { cache } = world();
  const { capabilities } = cache;
  assert.equal(capabilities.backend, 'cache(memory)');
  assert.equal(capabilities.bulk, 'sequential-non-atomic');
  assert.equal(capabilities.triplets, false);
  assert.ok(capabilities.operations.includes('create'));
  assert.ok(!capabilities.operations.includes('query'));
  assert.equal(cache.require(), cache);
  assert.throws(() => cache.require('atomicTransactions'), UnsupportedError);
});

test('text and triplets are refused, as a cache cannot follow them', async () => {
  const { cache } = world();
  await assert.rejects(cache.query('() ((1 1))'), UnsupportedError);
  await assert.rejects(cache.requestText('(count)'), UnsupportedError);
  await assert.rejects(cache.request([]), UnsupportedError);
  await assert.rejects(cache.createTriplet(1n, 1n, 1n), UnsupportedError);
});

test('a batch runs through the cache', async () => {
  const { cache, memory } = world({ maxLinks: 1 });
  const { completed, results } = await cache.batch([
    { op: 'create', source: 1n, target: 1n },
    { op: 'create', source: 2n, target: 2n },
  ]);
  assert.equal(completed, true);
  assert.equal(results.length, 2);
  assert.equal(await memory.count(), 1n);
  assert.deepEqual(await all(cache), await all(memory));
});

test('the documented example works', async () => {
  const cache = new CacheLinks(new MemoryLinks(), { maxLinks: 1000, timeToLive: 60_000 });
  const { index } = await cache.create(1n, 1n);
  assert.ok((await cache.timeToLive(index)) <= 60_000);
  assert.equal(await cache.persist(index), true);
  assert.equal(await cache.timeToLive(index), null);
  assert.equal(await cache.expire(index, 5_000), true);
  assert.notEqual(await cache.timeToLive(index), null);
});
