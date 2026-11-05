// The ids other systems give our links, stored as links (issue #149): a UUID
// of a service, an ObjectId of a document database, a URL, bound to a link of
// ours and back. The layout is the one of the C# and Rust `LinksExternalIds`,
// so a store written by one reads the same in the others:
//
// The point named `ExternalId` is `E`, a scheme (such as `uuid`) is the link
// `(S: E S)` named after it, and a link `l` bound to the id `x` has a pair
// `(p: S l)` named `scheme:x`. A link has one id of a scheme and an id one
// link. The table lives in the data or in a store of its own; every method
// takes the store of the table.
//
// `ExternalIdMap` makes such a table the map of a `RemapLinks`, so the links
// read under the other system's ids. Keep that table in a store apart from
// the network it remaps: in the same store, every read would give the
// table's own links ids too.

import { LinksError } from './errors.js';
import { ANY, toAddress } from './operations.js';

/** The name of the point every scheme starts from. */
export const EXTERNAL_ID_NAME = 'ExternalId';

async function linkAt(store, index, options) {
  const [link] = await store.each([index], options);
  return link ?? null;
}

async function search(store, source, target, options) {
  const [link] = await store.each([ANY, source, target], options);
  return link ? link.index : null;
}

/** A new link `(l: source target)`; `source`/`target` null for the link itself. */
async function newLink(store, source, target, options) {
  const created = await store.create(0n, 0n, options);
  if (created === null) throw new LinksError('the store merged a new link into an existing one; it cannot hold a new point', { outcome: 'failed' });
  const index = created.index;
  await store.update(index, source ?? index, target ?? index, options);
  return index;
}

/** The ids of one scheme; see the top of this module. */
export class LinksExternalIds {
  constructor(scheme) {
    if (typeof scheme !== 'string' || scheme === '' || scheme.includes(':')) {
      throw new TypeError(`a scheme is a non-empty string without a colon, got ${JSON.stringify(scheme)}`);
    }
    this.scheme = scheme;
  }

  /** The name of the pair of `id`. */
  nameOf(id) {
    if (typeof id !== 'string' || id === '') throw new TypeError('an id is a non-empty string');
    return `${this.scheme}:${id}`;
  }

  /** The link `id` is bound to, null when it is bound to none. */
  async find(store, id, options) {
    const pair = await this.#pairOf(store, id, options);
    if (pair === null) return null;
    return (await linkAt(store, pair, options))?.target ?? null;
  }

  /** The id `link` is bound to, null when it has none. */
  async idOf(store, link, options) {
    const address = toAddress(link, 'link');
    const point = await this.#schemeLink(store, options);
    if (point === null || address === 0n) return null;
    const pair = await search(store, point, address, options);
    if (pair === null) return null;
    const name = await store.getName(pair, options);
    const prefix = `${this.scheme}:`;
    return name?.startsWith(prefix) ? name.slice(prefix.length) : null;
  }

  /** Binds `id` to `link`, instead of any link it had and of any id the link had; resolves to the pair. */
  async bind(store, id, link, options) {
    const name = this.nameOf(id);
    const address = toAddress(link, 'link');
    if (address === 0n) throw new TypeError('the null link has no id');
    await this.unbind(store, id, options);
    let point = await this.#schemeLink(store, options);
    if (point === null) {
      let root = await store.getByName(EXTERNAL_ID_NAME, options);
      if (root === null || root === 0n || (await linkAt(store, root, options)) === null) {
        root = await newLink(store, null, null, options);
        await store.setName(root, EXTERNAL_ID_NAME, options);
      }
      point = await newLink(store, root, null, options);
      await store.setName(point, this.scheme, options);
    }
    let pair = await search(store, point, address, options);
    if (pair === null) {
      const created = await store.create(point, address, options);
      pair = created?.index ?? (await search(store, point, address, options));
    }
    await store.setName(pair, name, options);
    return pair;
  }

  /** The link `id` is bound to, a new point `(l: l l)` of `data` (or of the table's store) bound to it when it has none. */
  async ensure(store, data, id, options) {
    const found = await this.find(store, id, options);
    if (found !== null) return found;
    const link = await newLink(data ?? store, null, null, options);
    await this.bind(store, id, link, options);
    return link;
  }

  /** Unbinds `id`; resolves to whether it was bound. */
  async unbind(store, id, options) {
    const pair = await this.#pairOf(store, id, options);
    if (pair === null) return false;
    await store.removeName(pair, options);
    await store.delete(pair, options);
    return true;
  }

  async #pairOf(store, id, options) {
    const name = this.nameOf(id);
    const point = await this.#schemeLink(store, options);
    if (point === null) return null;
    const pair = await store.getByName(name, options);
    if (pair === null || pair === 0n) return null;
    return (await linkAt(store, pair, options))?.source === point ? pair : null;
  }

  /** The link `(S: E S)` of the scheme, null when the table has none yet. */
  async #schemeLink(store, options) {
    const root = await store.getByName(EXTERNAL_ID_NAME, options);
    if (root === null || root === 0n) return null;
    const point = await store.getByName(this.scheme, options);
    if (point === null || point === 0n) return null;
    const link = await linkAt(store, point, options);
    return link !== null && link.source === root && link.target === point ? point : null;
  }
}

/**
 * The ids of a scheme kept in links as the map of a `RemapLinks`: the links
 * of the network below read under those ids, and a link that has none gets
 * `newId()` (a random UUID by default). `table` is the store of the table,
 * apart from the network remapped.
 */
export class ExternalIdMap {
  constructor(table, scheme, { newId } = {}) {
    if (!table || typeof table.each !== 'function') throw new TypeError('the table of an ExternalIdMap is a links client');
    this.table = table;
    this.ids = scheme instanceof LinksExternalIds ? scheme : new LinksExternalIds(scheme);
    this.newId = newId ?? (() => globalThis.crypto.randomUUID());
    if (typeof this.newId !== 'function') throw new TypeError('newId is a function returning a new id');
  }

  toInner(outer) {
    return typeof outer === 'string' && outer !== '' ? this.ids.find(this.table, outer) : null;
  }

  async toOuter(inner) {
    const known = await this.ids.idOf(this.table, inner);
    if (known !== null) return known;
    const id = this.newId();
    await this.ids.bind(this.table, id, inner);
    return id;
  }
}
