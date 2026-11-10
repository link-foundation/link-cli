// A links network kept as a cache (issue #138): links can expire, and the network
// can be kept to a number of links by evicting the ones used least recently. It has
// the links contract of any other store, so only a store that selects it pays for
// it. See docs/ENGINE-CACHE.md. The twins are `link_cli::CacheLinks` in Rust and
// `CacheLinksDecorator<TLinkAddress>` in C#.
//
// The two rules differ on purpose. A deadline is a promise, so a link that comes due
// is deleted with whatever the store's own delete takes with it. Capacity only has
// to free a place, so it evicts a link nothing else uses and takes nothing else
// along. When every link is in use the create is refused.
//
// There is no worker. Everything due is deleted by the next call that touches the
// store, or by `purgeExpired` when a caller wants to run it on a timer. Deadlines
// live in memory: a store opened again has links without one.
//
// Calls run one at a time, in order. A write below whose outcome is unknown (the
// connection broke, or it was aborted) makes the next call read the store again:
// the links still there keep their deadlines, and a link that request did make has
// none.

import { LinksDecorator } from './client.js';
import {
  checkRestriction,
  COMPOSED_OPERATIONS,
  isAny,
  refuseTriplets,
  requireGuarantees,
  runBatch,
} from './composed.js';
import { LimitExceededError, UnsupportedError } from './errors.js';
import { toAddress } from './operations.js';

const CACHED_OPERATIONS = [
  ...COMPOSED_OPERATIONS,
  'exportLinks',
  'subscribe',
  'unsubscribe',
  'receiveNotification',
  'onNotification',
];

function refusedText(what) {
  return new UnsupportedError(
    `a cache cannot follow the writes of ${what}: send LiNo text to the store below (\`links\`), or use the methods of the cache`,
  );
}

function checkMilliseconds(value, what) {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < 0)
    throw new TypeError(`${what} is a number of milliseconds, not below zero`);
  return value;
}

function isNone(link) {
  return link === null || link.index === 0n;
}

function referencesOf({ index, source, target }) {
  const references = target === source ? [source] : [source, target];
  // A link that refers to itself uses nothing else, and the null link is no link.
  return references.filter((reference) => reference !== index && reference !== 0n);
}

const soonestFirst = (a, b) => a.at - b.at || a.stamp - b.stamp;

/** Items in order; where one goes or is found is searched for. */
class Ordered {
  items = [];

  #position(item) {
    let [low, high] = [0, this.items.length];
    while (low < high) {
      const middle = (low + high) >>> 1;
      if (soonestFirst(this.items[middle], item) < 0) low = middle + 1;
      else high = middle;
    }
    return low;
  }

  add(item) {
    this.items.splice(this.#position(item), 0, item);
  }

  remove(item) {
    const position = this.#position(item);
    if (this.items[position] === item) this.items.splice(position, 1);
  }
}

/** `links` kept as the options tell; see the top of this module. */
export class CacheLinks extends LinksDecorator {
  #maxLinks;
  #timeToLive;
  #clock;
  #now = 0;
  #uses = 0;
  /** Every link: its place among the used and its place among the deadlines. */
  #entries = new Map();
  /** How many other links refer to an address. */
  #usages = new Map();
  /** The links nothing else uses, least recently used first. */
  #evictable = new Ordered();
  /** The links with a deadline, soonest first. */
  #deadlines = new Ordered();
  /** Whether the store has to be read before anything else: at first, and after an unknown outcome. */
  #stale = true;
  #turn = Promise.resolve();

  /**
   * @param links The store below. Every write has to go through this cache for it to know which links exist.
   * @param options.maxLinks How many links the store may hold, reached by evicting.
   * @param options.timeToLive The milliseconds a link created here has before it comes due.
   * @param options.clock Milliseconds, `Date.now` by default. A step back delays what comes due.
   */
  constructor(links, { maxLinks = null, timeToLive = null, clock = Date.now } = {}) {
    super(links);
    if (maxLinks !== null && (!Number.isSafeInteger(maxLinks) || maxLinks < 0))
      throw new TypeError('maxLinks is a whole number of links, not below zero');
    if (typeof clock !== 'function') throw new TypeError('a clock is a function');
    this.#maxLinks = maxLinks;
    this.#timeToLive = timeToLive === null ? null : checkMilliseconds(timeToLive, 'timeToLive');
    this.#clock = clock;
  }

  get capabilities() {
    const inner = this.links.capabilities;
    return Object.freeze({
      ...inner,
      backend: `cache(${inner.backend})`,
      operations: inner.operations.filter((operation) => CACHED_OPERATIONS.includes(operation)),
      bulk: 'sequential-non-atomic',
      triplets: false,
    });
  }

  require(...guarantees) {
    requireGuarantees(this.capabilities, guarantees);
    return this;
  }

  /** Runs `work` when the calls before it are done. */
  #queued(work) {
    const run = this.#turn.then(work);
    this.#turn = run.catch(() => {});
    return run;
  }

  /** Runs `work` in turn, on a store with nothing due. */
  #inTurn(options, work) {
    return this.#queued(async () => {
      await this.#purge(options);
      return work();
    });
  }

  /** A write below; one whose outcome is not known leaves the store to be read again. */
  async #writing(write) {
    try {
      return await write();
    } catch (error) {
      if (error?.outcome !== 'not-sent' && error?.outcome !== 'failed') this.#stale = true;
      throw error;
    }
  }

  async #refresh(options) {
    if (!this.#stale) return;
    const links = await this.links.each([], options);
    const found = new Set();
    this.#usages.clear();
    this.#evictable.items = [];
    for (const link of links) {
      found.add(link.index);
      for (const reference of referencesOf(link))
        this.#usages.set(reference, (this.#usages.get(reference) ?? 0) + 1);
      if (!this.#entries.has(link.index)) this.#entries.set(link.index, this.#entry(link.index));
    }
    for (const [index, entry] of this.#entries) {
      if (!found.has(index)) this.#forget(index);
      else if (!this.#usages.has(index)) this.#evictable.add(entry.used);
    }
    this.#stale = false;
  }

  #entry(index) {
    this.#uses += 1;
    return { used: { at: this.#uses, stamp: 0, index }, due: null };
  }

  #forget(index) {
    const entry = this.#entries.get(index);
    // A link the store below made itself was never counted.
    if (entry === undefined) return;
    const { used, due } = entry;
    this.#entries.delete(index);
    this.#evictable.remove(used);
    if (due !== null) this.#deadlines.remove(due);
  }

  #touch(index) {
    const entry = this.#entries.get(index);
    if (entry === undefined) return;
    if (!this.#usages.has(index)) this.#evictable.remove(entry.used);
    this.#uses += 1;
    entry.used = { ...entry.used, at: this.#uses };
    if (!this.#usages.has(index)) this.#evictable.add(entry.used);
  }

  #addUsage(index) {
    const usages = this.#usages.get(index) ?? 0;
    this.#usages.set(index, usages + 1);
    const entry = this.#entries.get(index);
    if (usages === 0 && entry !== undefined) this.#evictable.remove(entry.used);
  }

  #removeUsage(index) {
    const usages = this.#usages.get(index);
    if (usages === undefined) return;
    if (usages > 1) {
      this.#usages.set(index, usages - 1);
      return;
    }
    this.#usages.delete(index);
    const entry = this.#entries.get(index);
    if (entry !== undefined) this.#evictable.add(entry.used);
  }

  #setDeadline(entry, at) {
    if (entry.due !== null) this.#deadlines.remove(entry.due);
    this.#uses += 1;
    entry.due = at === null ? null : { at, stamp: this.#uses, index: entry.used.index };
    if (entry.due !== null) this.#deadlines.add(entry.due);
  }

  /** A change as the store reports it. */
  #changed({ before, after }) {
    if (!isNone(before)) for (const reference of referencesOf(before)) this.#removeUsage(reference);
    if (!isNone(after)) for (const reference of referencesOf(after)) this.#addUsage(reference);
    if (isNone(before)) this.#created(after.index);
    else if (isNone(after)) this.#forget(before.index);
    else this.#touch(before.index);
  }

  #created(index) {
    const entry = this.#entry(index);
    this.#entries.set(index, entry);
    if (!this.#usages.has(index)) this.#evictable.add(entry.used);
    if (this.#timeToLive !== null) this.#setDeadline(entry, this.#now + this.#timeToLive);
  }

  /** Deletes a link below and follows what that took along. */
  async #deleted(index, options) {
    const changes = await this.#writing(() => this.links.delete(index, options));
    for (const change of changes) this.#changed(change);
    return changes;
  }

  async #purge(options) {
    await this.#refresh(options);
    this.#now = this.#clock();
    let deleted = 0;
    for (;;) {
      const soonest = this.#deadlines.items[0];
      if (soonest === undefined || soonest.at > this.#now) return deleted;
      const changes = await this.#deleted(soonest.index, options);
      deleted += changes.filter(({ after }) => isNone(after)).length;
    }
  }

  async #makeRoom(kept, options) {
    if (this.#maxLinks === null) return;
    while (this.#entries.size >= this.#maxLinks) {
      const victim = this.#evictable.items.find(({ index }) => !kept.includes(index));
      if (victim === undefined) {
        throw new LimitExceededError(
          `the cache holds ${this.#maxLinks} links and every other one is in use`,
          { outcome: 'not-sent' },
        );
      }
      await this.#deleted(victim.index, options);
    }
  }

  /** Deletes what is due now; resolves to how many links that deleted, cascades included. */
  async purgeExpired(options) {
    return this.#queued(() => this.#purge(options));
  }

  /** Gives `link` a deadline in place of any it had; false when there is no such link. A time of zero deletes it now, as Redis `EXPIRE` does. */
  async expire(link, milliseconds, options) {
    const address = toAddress(link, 'link');
    checkMilliseconds(milliseconds, 'an expiry');
    return this.#inTurn(options, async () => {
      const entry = this.#entries.get(address);
      if (entry === undefined) return false;
      this.#setDeadline(entry, this.#now + milliseconds);
      await this.#purge(options);
      return true;
    });
  }

  /** Takes the deadline of `link` away; false when it had none. */
  async persist(link, options) {
    const address = toAddress(link, 'link');
    return this.#inTurn(options, () => {
      const entry = this.#entries.get(address);
      if (entry === undefined || entry.due === null) return false;
      this.#setDeadline(entry, null);
      return true;
    });
  }

  /** The milliseconds `link` has left; null when it has no deadline or is gone. */
  async timeToLive(link, options) {
    const address = toAddress(link, 'link');
    return this.#inTurn(options, () => {
      const due = this.#entries.get(address)?.due ?? null;
      return due === null ? null : due.at - this.#now;
    });
  }

  async count(restriction = [], options) {
    checkRestriction(restriction);
    return this.#inTurn(options, () => this.links.count(restriction, options));
  }

  /** A read of one link by its address, or by its source and target, uses what it finds; a scan does not. */
  async each(restriction = [], options) {
    const parts = checkRestriction(restriction);
    const namesOne =
      (parts.length > 0 && !isAny(parts[0])) ||
      (parts.length === 3 && !isAny(parts[1]) && !isAny(parts[2]));
    return this.#inTurn(options, async () => {
      const links = await this.links.each(restriction, options);
      if (namesOne) for (const { index } of links) this.#touch(index);
      return links;
    });
  }

  /** Frees a place first, and not by evicting what the new link refers to. */
  async create(source, target, options) {
    const kept = [toAddress(source, 'source'), toAddress(target, 'target')];
    return this.#inTurn(options, async () => {
      await this.#makeRoom(kept, options);
      const link = await this.#writing(() => this.links.create(source, target, options));
      // A store that keeps links unique can answer with one it has.
      if (this.#entries.has(link.index)) this.#touch(link.index);
      else this.#changed({ before: null, after: link });
      return link;
    });
  }

  async update(index, source, target, options) {
    return this.#inTurn(options, async () => {
      const changes = await this.#writing(() => this.links.update(index, source, target, options));
      for (const change of changes) this.#changed(change);
      return changes;
    });
  }

  async delete(index, options) {
    return this.#inTurn(options, () => this.#deleted(index, options));
  }

  async getName(index, options) {
    return this.#inTurn(options, () => this.links.getName(index, options));
  }

  async setName(index, name, options) {
    return this.#inTurn(options, () => this.links.setName(index, name, options));
  }

  async getByName(name, options) {
    return this.#inTurn(options, () => this.links.getByName(name, options));
  }

  async removeName(index, options) {
    return this.#inTurn(options, () => this.links.removeName(index, options));
  }

  async exportLinks(options) {
    return this.#inTurn(options, () => this.links.exportLinks(options));
  }

  async requestText() {
    throw refusedText('a raw request');
  }

  async request() {
    throw refusedText('a raw request');
  }

  async query() {
    throw refusedText('a query');
  }

  batch(operations, options) {
    return runBatch(this, operations, options);
  }
}

refuseTriplets(CacheLinks, 'a cache');
