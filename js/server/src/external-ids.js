// The ids other systems give our links, stored as links (issue #149): a UUID
// of a service, an ObjectId of a document database, a URL, bound to a link of
// ours and back, so the code that talks to the other system never keeps a
// table of its own. Port of rust/src/external_ids.rs.
//
// The ids of one kind share a scheme, such as `uuid`. The point named
// `ExternalId` is `E`, the scheme is the link `(S: E S)` named after it, and
// a link `l` bound to the id `x` has a pair `(p: S l)` named `scheme:x`, so a
// link has one id of a scheme and an id one link. No link of the table but a
// pair starts at `S`, so a pair is never a link of the table itself, whatever
// address `l` has. Every method takes the store of the table: the data or a
// store of its own.

import { debugStr } from './graphql-json.js';
import { ensurePoint } from './types.js';

/** The name of the point every scheme starts from. */
export const EXTERNAL_ID_NAME = 'ExternalId';

function newPoint(links) {
  const link = links.tryCreate(0, 0);
  links.update(link, link, link);
  return link;
}

/** A name that is held, by a link other than the null one. */
const held = (store, name) => {
  const found = store.getByName(name);
  return found === null || found === 0 ? null : found;
};

export class LinksExternalIds {
  /** The ids of `scheme`, such as `uuid`; a scheme has no colon. */
  constructor(scheme) {
    if (scheme === '' || scheme.includes(':')) {
      throw new Error(`A scheme is not empty and has no colon: ${debugStr(scheme)}`);
    }
    this.schemeName = scheme;
  }

  /** The kind of the ids. */
  scheme() {
    return this.schemeName;
  }

  /** The name of the pair of `id`. */
  nameOf(id) {
    if (id === '') throw new Error('An id is not empty.');
    return `${this.schemeName}:${id}`;
  }

  /** The link `id` is bound to, `null` when it is bound to none. */
  find(store, id) {
    const pair = this.pairOf(store, id);
    // `pairOf` found the pair among the links.
    return pair === null ? null : store.getLink(pair).target;
  }

  /** The id `link` is bound to, `null` when it has none. */
  idOf(store, link) {
    const point = this.schemeLink(store);
    if (point === null || link === 0) return null;
    const pair = store.search(point, link);
    if (pair === null) return null;
    const name = store.getName(pair);
    const prefix = `${this.schemeName}:`;
    return name !== null && name.startsWith(prefix) ? name.slice(prefix.length) : null;
  }

  /**
   * Binds `id` to `link`, instead of any link it had and of any id the link
   * had; returns the pair that binds them.
   */
  bind(store, id, link) {
    const name = this.nameOf(id);
    if (link === 0) throw new Error('The null link has no id.');
    this.unbind(store, id);
    let point = this.schemeLink(store);
    if (point === null) {
      const root = ensurePoint(store, EXTERNAL_ID_NAME);
      point = store.tryCreate(0, 0);
      store.update(point, root, point);
      store.setName(point, this.schemeName);
    }
    const pair = store.search(point, link) ?? store.tryCreate(point, link);
    store.setName(pair, name);
    return pair;
  }

  /**
   * The link `id` is bound to, a new point `(l: l l)` of `data` bound to it
   * when it has none. `data` is `null` when the table is in the data.
   */
  ensure(store, data, id) {
    const found = this.find(store, id);
    if (found !== null) return found;
    const link = newPoint(data ?? store);
    this.bind(store, id, link);
    return link;
  }

  /** Unbinds `id`; whether it was bound. */
  unbind(store, id) {
    const pair = this.pairOf(store, id);
    if (pair === null) return false;
    store.removeName(pair);
    store.delete(pair);
    return true;
  }

  pairOf(store, id) {
    const name = this.nameOf(id);
    const point = this.schemeLink(store);
    if (point === null) return null;
    const pair = held(store, name);
    if (pair === null) return null;
    return store.getLink(pair)?.source === point ? pair : null;
  }

  /** The link `(S: E S)` of the scheme, `null` when the table has none yet. */
  schemeLink(store) {
    const root = held(store, EXTERNAL_ID_NAME);
    if (root === null) return null;
    const point = held(store, this.schemeName);
    if (point === null) return null;
    const found = store.getLink(point);
    return found?.source === root && found.target === point ? point : null;
  }
}
