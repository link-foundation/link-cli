// Many links networks seen as one (issue #149): a client's links joined with a
// server's, a cache with its source, the networks of several services. The
// C# twin is `UnionLinksDecorator<TLinkAddress>`, the Rust one `UnionLinks`.
//
// The members are in order, and the link at an address is the one of the
// first member that has it: a member shadows the ones after it, as the local
// overlay of minimongo or minilinks shadows the documents of the server.
// Reads see the links of every member that are not shadowed. Members whose
// addresses do not overlap (a `RemapLinks` with an `OffsetAddressMap` moves
// one out of the way of another) shadow nothing.
//
// A new link is created in the first member, the primary one. An update, a
// delete and naming go to the member that has the link, so a link stays where
// it lives and that member's store resolves the write (merges and cascades
// included, which keep to that member). A link may refer to links of other
// members: a link of the client typed by a type of the server holds the
// server type's address. Names are found in the members in order.
//
// The union depends on no other layer and is itself a links client: wrap it,
// or put it in another union. It translates the calls with addresses; raw
// LiNo text, queries and subscriptions belong to one store, so they go to a
// member (`union.members[i]`), never to the wrong one.

import {
  byIndex,
  COMPOSED_OPERATIONS,
  refuseTriplets,
  requireGuarantees,
  runBatch,
  untranslatable,
} from './composed.js';
import { AddressRangeError, NotFoundError, UnsupportedError } from './errors.js';
import { toAddress } from './operations.js';

/** Networks seen as one; see the top of this module. */
export class UnionLinks {
  constructor(members) {
    if (!Array.isArray(members) || members.length === 0)
      throw new TypeError('a union joins at least one links network');
    for (const member of members) {
      if (!member || typeof member.each !== 'function')
        throw new TypeError('a member of a union is a links client');
    }
    this.members = Object.freeze([...members]);
  }

  /** The member new links are created in. */
  get primary() {
    return this.members[0];
  }

  get capabilities() {
    const all = this.members.map((member) => member.capabilities);
    const every = (guarantee) => all.every((capabilities) => capabilities[guarantee] === true);
    const maxAddress = all
      .map((capabilities) => capabilities.maxAddress)
      .reduce((max, address) => (address > max ? address : max));
    return Object.freeze({
      backend: `union(${all.map((capabilities) => capabilities.backend).join(', ')})`,
      transport: 'composed',
      protocol: all[0].protocol,
      operations: COMPOSED_OPERATIONS.filter((operation) =>
        all.every((capabilities) => capabilities.operations.includes(operation)),
      ),
      addressBits: Math.max(...all.map((capabilities) => capabilities.addressBits)),
      maxAddress,
      // A write goes to one member; nothing spans members.
      atomicTransactions: false,
      rollback: false,
      crashConsistency: every('crashConsistency'),
      durableFlush: every('durableFlush'),
      pagination: false,
      persistent: every('persistent'),
      rawTcp: every('rawTcp'),
      // The one guarantee a union adds: stores merged behind one contract.
      sharding: true,
      subscriptions: false,
      triplets: false,
      duplicateLinks: all[0].duplicateLinks,
      bulk: 'sequential-non-atomic',
    });
  }

  require(...guarantees) {
    requireGuarantees(this.capabilities, guarantees);
    return this;
  }

  /** Whether `member` has a link at `index`; an address past its range it has not. */
  async #has(member, index, options) {
    try {
      return (await member.count([index], options)) > 0n;
    } catch (error) {
      if (error instanceof AddressRangeError) return false;
      throw error;
    }
  }

  /** The position of the member that has the link at `index`, null when none has it. */
  async ownerOf(index, options) {
    const address = toAddress(index, 'index');
    if (address === 0n) return null;
    for (const [position, member] of this.members.entries()) {
      if (await this.#has(member, address, options)) return position;
    }
    return null;
  }

  /** Whether a member before the one at `position` has a link at `index`. */
  async #shadowed(position, index, options) {
    for (const member of this.members.slice(0, position)) {
      if (await this.#has(member, index, options)) return true;
    }
    return false;
  }

  async #owner(index, options) {
    const owner = await this.ownerOf(index, options);
    if (owner === null) throw new NotFoundError(toAddress(index, 'index'));
    return this.members[owner];
  }

  async #ownerOrPrimary(index, options) {
    return this.members[(await this.ownerOf(index, options)) ?? 0];
  }

  /** What `read` finds in every member, the shadowed links left out. */
  async #unshadowed(read, options) {
    const found = [];
    for (const [position, member] of this.members.entries()) {
      let links;
      try {
        links = await read(member);
      } catch (error) {
        // A restriction past a member's range matches nothing of it.
        if (error instanceof AddressRangeError) continue;
        throw error;
      }
      for (const link of links) {
        if (!(await this.#shadowed(position, link.index, options))) found.push(link);
      }
    }
    return byIndex(found);
  }

  async count(restriction = [], options) {
    return BigInt((await this.each(restriction, options)).length);
  }

  each(restriction = [], options) {
    return this.#unshadowed((member) => member.each(restriction, options), options);
  }

  create(source, target, options) {
    return this.primary.create(source, target, options);
  }

  async update(index, source, target, options) {
    return (await this.#owner(index, options)).update(index, source, target, options);
  }

  async delete(index, options) {
    return (await this.#owner(index, options)).delete(index, options);
  }

  /** The name of a link in the member that has it, else in the first member that names it. */
  async getName(index, options) {
    const owner = await this.ownerOf(index, options);
    if (owner !== null) return this.members[owner].getName(index, options);
    for (const member of this.members) {
      try {
        const name = await member.getName(index, options);
        if (name !== null) return name;
      } catch (error) {
        if (!(error instanceof AddressRangeError)) throw error;
      }
    }
    return null;
  }

  /** Names a link in the member that has it, the primary one when none does. */
  async setName(index, name, options) {
    return (await this.#ownerOrPrimary(index, options)).setName(index, name, options);
  }

  /** The link of `name` in the first member that has one it does not shadow. */
  async getByName(name, options) {
    for (const [position, member] of this.members.entries()) {
      const found = await member.getByName(name, options);
      if (found !== null && found !== 0n && !(await this.#shadowed(position, found, options)))
        return found;
    }
    return null;
  }

  /** Unnames a link in the member that has it, the primary one when none does. */
  async removeName(index, options) {
    await (await this.#ownerOrPrimary(index, options)).removeName(index, options);
  }

  async requestText() {
    throw untranslatable('a union', 'a raw request');
  }

  async request() {
    throw untranslatable('a union', 'a raw request');
  }

  async query() {
    throw untranslatable('a union', 'a query');
  }

  async exportLinks() {
    throw untranslatable('a union', 'an export');
  }

  batch(operations, options) {
    return runBatch(this, operations, options);
  }

  async subscribe() {
    throw unsubscribable();
  }

  async unsubscribe() {
    throw unsubscribable();
  }

  async receiveNotification() {
    throw unsubscribable();
  }

  onNotification() {
    throw unsubscribable();
  }

  get pendingNotifications() {
    return 0;
  }

  /** Closes every member, even when one fails to close. */
  async close() {
    const results = await Promise.allSettled(this.members.map((member) => member.close()));
    const failed = results.find((result) => result.status === 'rejected');
    if (failed) throw failed.reason;
  }

  async [Symbol.asyncDispose]() {
    await this.close();
  }
}

refuseTriplets(UnionLinks, 'a union');

function unsubscribable() {
  return new UnsupportedError(
    'a union does not subscribe: subscribe to the member whose changes you want (union.members[i])',
  );
}
