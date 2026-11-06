// A links network under other addresses (issue #149): every address that goes
// down is mapped to the network's own and every address that comes up is
// mapped back, so the code above never thinks about remapping. The C# twin is
// `RemapLinksDecorator<TLinkAddress>`, the Rust one `RemapLinks`.
//
// The map is any object with `toInner(outer)` and `toOuter(inner)`, either of
// which may return a promise:
//
// - `OffsetAddressMap` moves a network out of the way of another, so a
//   `UnionLinks` of them shadows nothing;
// - `AddressTable` gives the addresses of another system addresses of our own
//   range, or, with `newId`, gives our links ids of another kind (UUIDs);
// - `ExternalIdMap` (external-ids.js) keeps the ids of another system in
//   links, so the table survives the process.
//
// Above a remap whose map gives ids that are not addresses (UUID strings),
// the indexes, sources and targets of links are those ids: the network reads
// as the other system names it. The null link is `0n` on both sides.
//
// A read that names an address that is not of the network below finds
// nothing; a write that names one fails with an AddressRangeError (a
// reference) or a NotFoundError (the link written), before anything is sent.

import { LinksDecorator } from './client.js';
import {
  byIndex,
  checkRestriction,
  COMPOSED_OPERATIONS,
  isAny,
  isNullAddress,
  refuseTriplets,
  requireGuarantees,
  runBatch,
  untranslatable,
} from './composed.js';
import { AddressRangeError, NotFoundError } from './errors.js';
import { toAddress } from './operations.js';

/**
 * Every address moved by the same offset: the network below reads at
 * `offset + 1` and up, so networks moved by different offsets share no
 * address and a union of them shadows nothing.
 */
export class OffsetAddressMap {
  constructor(offset) {
    this.offset = toAddress(offset, 'offset');
  }

  toInner(outer) {
    const address = toAddress(outer);
    return address > this.offset ? address - this.offset : null;
  }

  toOuter(inner) {
    return toAddress(inner) + this.offset;
  }

  /** The largest address above, given the largest one below. */
  maxAddress(innerMaxAddress) {
    return typeof innerMaxAddress === 'bigint' ? innerMaxAddress + this.offset : innerMaxAddress;
  }
}

/**
 * A table of the addresses above and below. A link below that has no address
 * above yet gets one: the next free address from `first` on, or `newId()`
 * (for example `() => crypto.randomUUID()`) when the ids above are of
 * another kind. `pairs()` and `add()` keep the table across runs, in links
 * or anywhere else.
 */
export class AddressTable {
  #inner = new Map();
  #outer = new Map();
  #next;
  #newId;

  constructor({ first, newId } = {}) {
    if ((first === undefined) === (newId === undefined)) {
      throw new TypeError(
        'an address table gives addresses from `first` on or ids from `newId()`, one of the two',
      );
    }
    if (newId !== undefined) {
      if (typeof newId !== 'function')
        throw new TypeError('newId is a function returning a new id');
      this.#newId = newId;
    } else {
      this.#next = toAddress(first, 'first');
      if (this.#next === 0n) throw new AddressRangeError('the null link is no address to give');
    }
  }

  /** Whether the addresses above are BigInt addresses (a table from `first` on). */
  get numeric() {
    return this.#newId === undefined;
  }

  #key(outer) {
    return this.numeric ? toAddress(outer) : outer;
  }

  toInner(outer) {
    return this.#inner.get(this.#key(outer)) ?? null;
  }

  toOuter(inner) {
    const address = toAddress(inner);
    const known = this.#outer.get(address);
    if (known !== undefined) return known;
    let outer;
    if (this.numeric) {
      while (this.#inner.has(this.#next)) this.#next += 1n;
      outer = this.#next;
      this.#next += 1n;
    } else {
      outer = this.#newId();
    }
    this.add(outer, address);
    return outer;
  }

  /** Maps `outer` to `inner`; neither may be mapped to another one already. */
  add(outer, inner) {
    const key = this.#key(outer);
    const address = toAddress(inner);
    if (isNullAddress(key) || address === 0n)
      throw new AddressRangeError('the null link is mapped to itself');
    const knownInner = this.#inner.get(key);
    if (knownInner !== undefined && knownInner !== address)
      throw new AddressRangeError(`${key} is mapped to ${knownInner} already`);
    const knownOuter = this.#outer.get(address);
    if (knownOuter !== undefined && knownOuter !== key)
      throw new AddressRangeError(`${address} is mapped from ${knownOuter} already`);
    this.#inner.set(key, address);
    this.#outer.set(address, key);
  }

  /** Every `[outer, inner]` pair, by the address above. */
  pairs() {
    const pairs = [...this.#inner];
    return pairs.sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  }
}

const REMAPPED_OPERATIONS = [
  ...COMPOSED_OPERATIONS,
  'subscribe',
  'unsubscribe',
  'receiveNotification',
  'onNotification',
];

/** `links` under the addresses of `map`; see the top of this module. */
export class RemapLinks extends LinksDecorator {
  #delivered = Promise.resolve();

  constructor(links, map) {
    super(links);
    if (!map || typeof map.toInner !== 'function' || typeof map.toOuter !== 'function') {
      throw new TypeError('an address map has toInner(outer) and toOuter(inner)');
    }
    this.map = map;
  }

  get capabilities() {
    const inner = this.links.capabilities;
    return Object.freeze({
      ...inner,
      backend: `remap(${inner.backend})`,
      operations: inner.operations.filter((operation) => REMAPPED_OPERATIONS.includes(operation)),
      maxAddress:
        typeof this.map.maxAddress === 'function'
          ? this.map.maxAddress(inner.maxAddress)
          : inner.maxAddress,
      bulk: 'sequential-non-atomic',
      triplets: false,
    });
  }

  require(...guarantees) {
    requireGuarantees(this.capabilities, guarantees);
    return this;
  }

  /** The address above of `inner`; the null link reads the same. */
  async toOuter(inner) {
    const address = toAddress(inner);
    if (address === 0n) return 0n;
    return this.map.toOuter(address);
  }

  /** The address below of `outer`, null when it is no address of the network below. */
  async toInner(outer) {
    if (isNullAddress(outer)) return 0n;
    const inner = await this.map.toInner(outer);
    return inner === null || inner === undefined ? null : toAddress(inner);
  }

  async #found(outer) {
    const inner = await this.toInner(outer);
    if (inner === null || inner === 0n) throw new NotFoundError(outer);
    return inner;
  }

  async #referred(outer) {
    const inner = await this.toInner(outer);
    if (inner === null)
      throw new AddressRangeError(
        `a link of this network cannot refer to ${outer}: the address is not of this network`,
      );
    return inner;
  }

  async #outerLink(link) {
    if (link === null) return null;
    return {
      index: await this.toOuter(link.index),
      source: await this.toOuter(link.source),
      target: await this.toOuter(link.target),
    };
  }

  async #outerChanges(changes) {
    const outer = [];
    for (const { before, after } of changes)
      outer.push({ before: await this.#outerLink(before), after: await this.#outerLink(after) });
    return outer;
  }

  /** The restriction below, null when a part is no address of the network below. */
  async #innerRestriction(restriction) {
    const inner = [];
    for (const part of checkRestriction(restriction)) {
      if (isAny(part)) {
        inner.push(part);
        continue;
      }
      const address = await this.toInner(part);
      if (address === null) return null;
      inner.push(address);
    }
    return inner;
  }

  async count(restriction = [], options) {
    const inner = await this.#innerRestriction(restriction);
    return inner === null ? 0n : this.links.count(inner, options);
  }

  async each(restriction = [], options) {
    const inner = await this.#innerRestriction(restriction);
    if (inner === null) return [];
    const links = [];
    for (const link of await this.links.each(inner, options))
      links.push(await this.#outerLink(link));
    return byIndex(links);
  }

  async create(source, target, options) {
    const [innerSource, innerTarget] = [await this.#referred(source), await this.#referred(target)];
    return this.#outerLink(await this.links.create(innerSource, innerTarget, options));
  }

  async update(index, source, target, options) {
    const inner = await this.#found(index);
    const [innerSource, innerTarget] = [await this.#referred(source), await this.#referred(target)];
    return this.#outerChanges(await this.links.update(inner, innerSource, innerTarget, options));
  }

  async delete(index, options) {
    return this.#outerChanges(await this.links.delete(await this.#found(index), options));
  }

  async getName(index, options) {
    const inner = await this.toInner(index);
    return inner === null || inner === 0n ? null : this.links.getName(inner, options);
  }

  /** Names a link; resolves to its address above. */
  async setName(index, name, options) {
    const inner = await this.#found(index);
    await this.links.setName(inner, name, options);
    return this.toOuter(inner);
  }

  async getByName(name, options) {
    const inner = await this.links.getByName(name, options);
    return inner === null ? null : this.toOuter(inner);
  }

  async removeName(index, options) {
    const inner = await this.toInner(index);
    if (inner !== null && inner !== 0n) await this.links.removeName(inner, options);
  }

  async requestText() {
    throw untranslatable('a remap', 'a raw request');
  }

  async request() {
    throw untranslatable('a remap', 'a raw request');
  }

  async query() {
    throw untranslatable('a remap', 'a query');
  }

  async exportLinks() {
    throw untranslatable('a remap', 'an export');
  }

  batch(operations, options) {
    return runBatch(this, operations, options);
  }

  /** Subscribes below; the changes pushed read under the addresses above. */
  async subscribe(restriction = [], options) {
    const inner = await this.#innerRestriction(restriction);
    if (inner === null)
      throw new AddressRangeError('a subscription names an address that is not of this network');
    return this.links.subscribe(inner, options);
  }

  async receiveNotification(options) {
    const notification = await this.links.receiveNotification(options);
    return notification === null
      ? null
      : {
          subscription: notification.subscription,
          changes: await this.#outerChanges(notification.changes),
        };
  }

  onNotification(listener) {
    if (typeof listener !== 'function')
      throw new TypeError('a notification listener is a function');
    return this.links.onNotification((notification) => {
      // Mapping may wait (a map that reads links); the order is kept.
      this.#delivered = this.#delivered
        .then(() => this.#outerChanges(notification.changes))
        .then((changes) => listener({ subscription: notification.subscription, changes }))
        .catch((error) =>
          queueMicrotask(() => {
            throw error;
          }),
        );
    });
  }
}

refuseTriplets(RemapLinks, 'a remap');
