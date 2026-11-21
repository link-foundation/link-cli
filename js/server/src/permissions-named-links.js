// The store as one subject sees it: `PermissionsDecorator`, with every read,
// write and name operation checked against the decorator's policy, and the
// `Forbidden` error it refuses writes with. Port of the decorator in
// rust/src/permissions.rs and of its `NamedTypeLinks` implementation in
// rust/src/permissions/named_links.rs.
//
// Reads skip the links the subject may not read, so a hidden link is as
// missing as a deleted one: to `exists`, to `getLink`, and to every write,
// which answers `Link not found`. A write the subject may not make fails with
// `Forbidden` and changes nothing. An update needs the right to write the
// link before and after it.
//
// A link the subject created through this view is its own while it has no
// type (its source is 0), so it can give it one. `forget` drops that claim
// when the link is gone.
//
// The store below may change more than the link asked for: deleting a link
// deletes the links that use it, and an update that makes a link equal to
// another merges them, repointing the usages. Every link a write changes must
// be the subject's to change; when one is not, the write is undone and
// refused as a whole.
//
// A reference needs `canReference`: without the right an unreadable address
// answers as a missing one, to `referenceExists`, to a create and to an
// update. The write check runs first, so a type the subject may not write
// stays `Forbidden` for hidden and missing alike. A reference a link already
// holds may stay; once replaced, it cannot be put back without the right.
// Choosing the address of a new link (`tryEnsureCreated`) is a reference too,
// so a limited subject cannot choose addresses.
//
// Names follow references: `getName` and `getByName` answer only for a link
// the subject may refer to, so any other name is an unknown name. Naming or
// unnaming a link needs the right to write it, and taking a name needs the
// right to write the link that holds it.
import { errorText } from './error-report.js';
import { NotFoundError } from './errors.js';
import { link, NULL_LINK, sameLink } from './link.js';
import { NamedTypeLinks } from './named-type-links.js';
import { keepNamesBeforeUpdate, LinksUndoLog } from './undoable-links.js';

/** A write a subject may not make; it reads `Forbidden: what`. */
export class Forbidden extends Error {
  constructor(what) {
    super(`Forbidden: ${what}`);
    this.name = 'Forbidden';
    /** What was refused, such as `update 7`. */
    this.what = what;
  }
}

/** The store as one subject sees it; see the module comment. */
export class PermissionsDecorator extends NamedTypeLinks {
  #links;
  #policy;
  #subject;
  #created = new Set();

  /**
   * `links` as `subject` (the null link is a guest) sees it under `policy`,
   * an `AccessPolicy`.
   */
  constructor(links, policy, subject) {
    super();
    this.#links = links;
    this.#policy = policy;
    this.#subject = subject;
  }

  /** The same view with the links `created` before taken as the subject's own. */
  withCreated(created) {
    this.#created = created;
    return this;
  }

  /** The subject the store is seen as. */
  subject() {
    return this.#subject;
  }

  setSubject(subject) {
    this.#subject = subject;
  }

  /** The links the subject created through this view and still has. */
  created() {
    return this.#created;
  }

  /** Swaps the links taken as the subject's own, returning the old ones. */
  replaceCreated(created) {
    const old = this.#created;
    this.#created = created;
    return old;
  }

  policy() {
    return this.#policy;
  }

  inner() {
    return this.#links;
  }

  /** Whether `value` is the subject's own: created through this view and still without a type. */
  owns(value) {
    return value.source === 0 && this.#created.has(value.index);
  }

  /** Whether the subject may read `value`. */
  canRead(value) {
    return this.owns(value) || this.#policy.canRead(this.#links, this.#subject, value);
  }

  /** Whether the subject may write `value` as it is. */
  canWrite(value) {
    return this.owns(value) || this.#policy.canWrite(this.#links, this.#subject, value);
  }

  /**
   * Whether the subject may refer to the address `id`, which may hold no
   * link: a missing address is judged as an untyped link, so only a rule for
   * `Anything` covers it.
   */
  canReference(id) {
    const value = this.#links.getLink(id) ?? link(id, 0, 0);
    return (
      this.ownsTriplet(value) ||
      this.canRead(value) ||
      this.#policy.canReference(this.#links, this.#subject, value)
    );
  }

  /** The references `before` does not hold yet need the right: without it the address answers as a missing one. */
  #checkReferences(before, source, target) {
    for (const part of [source, target]) {
      const held =
        before != null &&
        (part === before.index || part === before.source || part === before.target);
      if (part !== 0 && !held && !this.canReference(part)) throw new NotFoundError(part);
    }
  }

  /** `owns` when the store is read as triplets, where a point that is only its own type has no type yet either. */
  ownsTriplet(value) {
    const point = value.source === value.index && value.target === value.index;
    return this.owns(value) || (point && this.#created.has(value.index));
  }

  /** `canRead` when the store is read as triplets. */
  canReadTriplet(value) {
    return this.ownsTriplet(value) || this.#policy.canRead(this.#links, this.#subject, value);
  }

  /** `canWrite` when the store is read as triplets. */
  canWriteTriplet(value) {
    return this.ownsTriplet(value) || this.#policy.canWrite(this.#links, this.#subject, value);
  }

  /** Whether the policy lets the subject write `value`, ownership aside. */
  policyCanWrite(value) {
    return this.#policy.canWrite(this.#links, this.#subject, value);
  }

  policyCanCreate() {
    return this.#policy.canCreate(this.#links, this.#subject);
  }

  /** The link at `index` is one the subject made on the store below. */
  remember(index) {
    this.#created.add(index);
  }

  /** The store below and the policy, to judge a request on a layer over the same store. */
  partsMut() {
    return [this.#links, this.#policy];
  }

  /** The link at `index` is gone: a new one there is not the subject's own. */
  forget(index) {
    this.#created.delete(index);
  }

  /** The stored link at `index`, `null` when there is none; a hidden one is not found. */
  #visible(index) {
    if (index === 0) return null;
    const value = this.#links.getLink(index);
    if (value == null) return null;
    if (!this.canRead(value)) throw new NotFoundError(index);
    return value;
  }

  #requireWritable(index, what) {
    const value = this.#visible(index);
    if (value == null) throw new NotFoundError(index);
    if (!this.canWrite(value)) throw new Forbidden(`${what} ${index}`);
  }

  #requireCreate(source, target) {
    if (
      !this.policyCanCreate() ||
      (source !== 0 && !this.policyCanWrite(link(0, source, target)))
    ) {
      throw new Forbidden('create');
    }
  }

  /** Whether the subject may change a link from `before` to `after`. */
  #allowed(before, after) {
    if (before != null && after != null && sameLink(before, after)) return true;
    if (before == null) {
      return this.policyCanCreate() && (after.source === 0 || this.policyCanWrite(after));
    }
    if (after == null) return this.canWrite(before);
    return (
      (this.owns(before) && after.source === 0) ||
      (this.canWrite(before) && this.policyCanWrite(after))
    );
  }

  /**
   * Makes a write on the store below, then checks every link it changed: the
   * link asked for is checked already, any other must be the subject's to
   * change, or the write is undone and refused.
   */
  #checked(what, index, log, write, observer) {
    let result;
    try {
      result = write(this.#links, (before, after) => log.recordObserved(before, after));
    } catch (error) {
      const undo = this.#tryUndo(log);
      // anyhow's `error.context(..)`: the write's error is the cause.
      if (undo != null)
        throw new Error(`and undoing it failed: ${errorText(undo)}`, { cause: error });
      throw error;
    }
    const net = new Map();
    for (const [before, after] of log.changes()) {
      const changed = before ?? after;
      if (changed == null) continue;
      const known = net.get(changed.index);
      if (known != null) known[1] = after;
      else net.set(changed.index, [before, after]);
    }
    for (const [at, [first, last]] of net) {
      if (at === index || this.#allowed(first, last)) continue;
      this.#undo(log);
      throw new Forbidden(what);
    }
    for (const [at, [, last]] of net) {
      if (last == null) this.#created.delete(at);
    }
    for (const [before, after] of log.changes()) observer(before ?? NULL_LINK, after ?? NULL_LINK);
    return result;
  }

  #undo(log) {
    for (const gone of log.undo(this.#links)) this.#created.delete(gone);
  }

  /** Undoes `log`; returns how undoing failed, or `null`. */
  #tryUndo(log) {
    try {
      this.#undo(log);
      return null;
    } catch (undo) {
      return undo;
    }
  }

  queryLimits() {
    return this.#links.queryLimits();
  }

  reserveQueryAllocations(processor, restriction, substitution) {
    const reserved = this.#links.reserveQueryAllocations(processor, restriction, substitution);
    if (reserved == null) return null;
    // A preview lists every row of its scratch store. Only addresses that do
    // not exist yet are allocations; counting an existing row would let a
    // hidden address pass as one and answer differently from a missing one.
    return new Set([...reserved].filter((id) => !this.#links.referenceExists(id)));
  }

  /** Throws when the subject may not create, as `tryCreate`. */
  create(source, target) {
    return this.tryCreate(source, target);
  }

  /** Throws when the subject may not create, as `tryEnsureCreated`. */
  ensureCreated(id) {
    return this.tryEnsureCreated(id);
  }

  tryEnsureCreated(id) {
    if (id !== 0 && this.exists(id)) return id;
    if (!this.policyCanCreate()) throw new Forbidden('create');
    if (id !== 0 && !this.canReference(id)) throw new NotFoundError(id);
    if (id !== 0 && this.#links.exists(id)) {
      // The subject may refer to this hidden link: the point must not land on it.
      throw new Error(`Link ${id} exists`);
    }
    const created = this.#links.tryEnsureCreated(id);
    if (id !== 0) this.#created.add(created);
    return created;
  }

  getLink(id) {
    const value = this.#links.getLink(id);
    if (value == null) return null;
    return this.canRead(value) ? value : null;
  }

  exists(id) {
    return this.getLink(id) != null;
  }

  tryCreate(source, target) {
    this.#requireCreate(source, target);
    this.#checkReferences(null, source, target);
    const id = this.#links.tryCreate(source, target);
    this.#created.add(id);
    return id;
  }

  /** A link the subject may not refer to is as missing as a deleted one. */
  referenceExists(id) {
    return this.#links.referenceExists(id) && this.canReference(id);
  }

  claims(id) {
    return this.#links.claims(id);
  }

  nextCreatedAddresses(count) {
    return this.#links.nextCreatedAddresses(count);
  }

  updateObserved(id, source, target, observer) {
    const before = this.#visible(id);
    if (before == null) return this.#links.updateObserved(id, source, target, observer);
    const after = link(id, source, target);
    if (
      !(this.owns(before) && source === 0) &&
      (!this.canWrite(before) || !this.policyCanWrite(after))
    ) {
      throw new Forbidden(`update ${id}`);
    }
    this.#checkReferences(before, source, target);
    const log = new LinksUndoLog();
    keepNamesBeforeUpdate(log, this.#links, id, source, target);
    return this.#checked(
      `update ${id}`,
      id,
      log,
      (links, recorder) => links.updateObserved(id, source, target, recorder),
      observer,
    );
  }

  deleteObserved(id, observer) {
    const before = this.#visible(id);
    if (before == null) return this.#links.deleteObserved(id, observer);
    if (!this.canWrite(before)) throw new Forbidden(`delete ${id}`);
    const log = new LinksUndoLog();
    log.keepNamesOfUsages(this.#links, id);
    return this.#checked(
      `delete ${id}`,
      id,
      log,
      (links, recorder) => links.deleteObserved(id, recorder),
      observer,
    );
  }

  allLinks() {
    return this.#links.allLinks().filter((value) => this.canRead(value));
  }

  queryLinks(index = null, source = null, target = null) {
    return this.#links.queryLinks(index, source, target).filter((value) => this.canRead(value));
  }

  search(source, target) {
    const found = this.#links.search(source, target);
    if (found == null) return null;
    return this.exists(found) ? found : null;
  }

  /** Throws when the subject may not create, as `tryGetOrCreate`. */
  getOrCreate(source, target) {
    return this.tryGetOrCreate(source, target);
  }

  /** The name of a link the subject may refer to; any other has none. */
  getName(id) {
    if (!this.canReference(id)) return null;
    return this.#links.getName(id);
  }

  setName(id, name) {
    this.#requireWritable(id, 'set-name');
    const holder = this.#links.getByName(name);
    if (holder != null && holder !== 0 && holder !== id) {
      const value = this.#links.getLink(holder);
      if (value != null && !this.canWrite(value)) {
        // Taking the name takes it from its link.
        throw new Forbidden(`set-name ${id}`);
      }
    }
    return this.#links.setName(id, name);
  }

  /** The link of a name, when the subject may refer to it; any other name is an unknown name. */
  getByName(name) {
    const found = this.#links.getByName(name);
    if (found == null) return null;
    return found === 0 || this.canReference(found) ? found : null;
  }

  removeName(id) {
    const value = this.#links.getLink(id);
    if (value != null) {
      // Nothing the subject sees has a name to remove.
      if (!this.canRead(value)) return;
      this.#requireWritable(id, 'remove-name');
    }
    this.#links.removeName(id);
  }

  save() {
    this.#links.save();
  }
}
