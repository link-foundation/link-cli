// The tokens a links server knows, stored as links (issue #149): the
// authenticator of `(auth: token)` that `clink --tokens` uses. Port of
// rust/src/permissions/tokens.rs.
//
// A subject `s` of the data has a pair `(p: Token s)`, where `Token` is the
// point named `Token`, and each of its tokens is a link `(t: p t)`, named
// `sha256:<hex>` after the SHA-256 of the token, so the store never holds a
// token itself. The table lives in a store of its own or in the data; any
// other authenticator is a callback of the links sessions.
import { createHash } from 'node:crypto';
import { ensurePoint } from './types.js';

export class LinksTokenTable {
  /** The name of the point every token link starts from. */
  static TOKEN_NAME = 'Token';

  #store;

  constructor(store) {
    this.#store = store;
  }

  store() {
    return this.#store;
  }

  /** The name of the link of `token`. */
  static nameOf(token) {
    return `sha256:${createHash('sha256').update(token, 'utf8').digest('hex')}`;
  }

  /** The link of `token` and its subject, or `null` when the table does not know it. */
  #find(token) {
    const store = this.#store;
    const point = store.getByName(LinksTokenTable.TOKEN_NAME) ?? 0;
    const id = store.getByName(LinksTokenTable.nameOf(token)) ?? 0;
    if (point === 0 || id === 0) return null;
    const stored = store.getLink(id);
    if (stored == null || stored.target !== id) return null;
    const pair = stored.source;
    if (pair === 0 || pair === id) return null;
    const pairLink = store.getLink(pair);
    if (pairLink == null || pairLink.source !== point || pairLink.target === 0) return null;
    return { link: id, subject: pairLink.target };
  }

  /** The subject of `token`, or `null` when the table does not know it. */
  authenticate(token) {
    const found = this.#find(token);
    return found == null ? null : found.subject;
  }

  /**
   * Lets `token` authenticate as `subject`, instead of any subject it had.
   * Returns the link of the token.
   */
  grant(token, subject) {
    if (subject === 0) throw new Error('A token needs a subject');
    const store = this.#store;
    const point = ensurePoint(store, LinksTokenTable.TOKEN_NAME);
    const pair = store.tryGetOrCreate(point, subject);
    const name = LinksTokenTable.nameOf(token);
    const known = store.getByName(name);
    const id = known != null && known !== 0 && store.exists(known) ? known : store.tryCreate(0, 0);
    store.update(id, pair, id);
    store.setName(id, name);
    return id;
  }

  /** Forgets `token`; whether the table knew it. */
  revoke(token) {
    const found = this.#find(token);
    if (found == null) return false;
    this.#store.removeName(found.link);
    this.#store.delete(found.link);
    return true;
  }
}
