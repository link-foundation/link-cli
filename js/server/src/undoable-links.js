// Writes kept to be undone (issue #149): `LinksUndoLog` and the
// `UndoableLinksDecorator` that fills it, so a request that fails can be
// undone whole. Port of rust/src/undoable_links.rs; the C# counterparts are
// `LinksUndoLog<TLinkAddress>` and `UndoableLinksDecorator<TLinkAddress>`.
//
// A change is `[before, after]` as a store reports it, `null` standing for no
// link: `[null, link]` is a creation, `[link, null]` a deletion.
import { NamedTypeLinks } from './named-type-links.js';
import { isNull, link, sameLink } from './link.js';

const present = (value) => (isNull(value) ? null : value);

/**
 * The writes made to a store, kept to undo them: every change a write
 * reports, and the names the links had before.
 *
 * Undoing goes back through the changes: it deletes what was created,
 * updates back what was updated and creates again, at the same address,
 * what was deleted. Names are not changes: a deleted link loses its name
 * before anyone above the names store hears of the deletion, so the names a
 * write may touch are kept before it, with `keepName` and
 * `keepNamesOfUsages`.
 */
export class LinksUndoLog {
  #changes = [];
  #names = [];
  #named = new Set();

  /** Whether nothing is kept. */
  isEmpty() {
    return this.#changes.length === 0 && this.#names.length === 0;
  }

  /** The changes kept, in the order they were made. */
  changes() {
    return this.#changes;
  }

  /** Keeps one change; a change that changes nothing is not kept. */
  record(before, after) {
    if (before != null && sameLink(before, after)) return;
    this.#changes.push([before, after]);
  }

  /** Keeps one change as a change observer reports it: the null link is no link. */
  recordObserved(before, after) {
    this.record(present(before), present(after));
  }

  /** Keeps the name `id` has now, unless one of it is kept already. */
  keepName(names, id) {
    if (this.#named.has(id)) return;
    this.#named.add(id);
    this.#names.push([id, names.getName(id) ?? null]);
  }

  /** Keeps the name of the link called `name` now, if any. */
  keepHolderOf(links, name) {
    const holder = links.getByName(name);
    if (holder != null && holder !== 0) this.keepName(links, holder);
  }

  /**
   * Keeps the names of `id` and of every link that uses it, directly or
   * not: the links deleting it deletes, or merging it repoints.
   */
  keepNamesOfUsages(links, id) {
    for (const usage of usagesOf(links, id)) this.keepName(links, usage);
  }

  /** Forgets everything kept: the writes stay. */
  clear() {
    this.#changes = [];
    this.#names = [];
    this.#named = new Set();
  }

  /**
   * Undoes every write kept, on `links` (the store the writes were made on,
   * not a view over it), and forgets them. Returns the addresses of the links
   * the writes created, now deleted again.
   */
  undo(links) {
    const created = [];
    for (const [before, after] of [...this.#changes].reverse()) {
      if (before == null && after != null) {
        // Whatever used it is undone already.
        if (links.exists(after.index)) links.delete(after.index);
        created.push(after.index);
      } else if (before != null && after == null) {
        if (!links.exists(before.index)) links.tryEnsureCreated(before.index);
        if (before.source !== 0 || before.target !== 0) {
          links.update(before.index, before.source, before.target);
        }
      } else if (before != null && links.exists(before.index)) {
        links.update(before.index, before.source, before.target);
      }
    }
    // A name is the link's again; first take every name a write gave.
    for (const [id, name] of this.#names) {
      if (name !== (links.getName(id) ?? null) && (name == null || !links.exists(id))) {
        links.removeName(id);
      }
    }
    for (const [id, name] of this.#names) {
      if (name != null && links.exists(id) && name !== links.getName(id)) {
        links.setName(id, name);
      }
    }
    this.clear();
    return created;
  }
}

/** `id` and every link that uses it, directly or not, `id` first. */
export function usagesOf(links, id) {
  const found = [id];
  const seen = new Set([id]);
  for (let i = 0; i < found.length; i++) {
    const at = found[i];
    const usages = [...links.queryLinks(null, at, null), ...links.queryLinks(null, null, at)];
    for (const usage of usages) {
      if (!seen.has(usage.index)) {
        seen.add(usage.index);
        found.push(usage.index);
      }
    }
  }
  return found;
}

/**
 * Keeps the names an update of `index` to `(source target)` may take: of the
 * link, and of its usages too when it merges into an equal link and so is
 * deleted.
 */
export function keepNamesBeforeUpdate(log, links, index, source, target) {
  const equal = links.search(source, target);
  if (equal != null && equal !== 0 && equal !== index) {
    log.keepNamesOfUsages(links, index);
  } else {
    log.keepName(links, index);
  }
}

/**
 * A store that keeps every write in a `LinksUndoLog` while `recording()`, so
 * a request that fails can be undone whole. The links server puts it under
 * every session with permissions.
 */
export class UndoableLinksDecorator extends NamedTypeLinks {
  #links;
  #log = new LinksUndoLog();
  #recording = false;

  constructor(links) {
    super();
    this.#links = links;
  }

  /** The writes kept since `begin`. */
  log() {
    return this.#log;
  }

  /** Whether writes are kept. */
  recording() {
    return this.#recording;
  }

  /** Starts keeping writes, forgetting the ones kept before. */
  begin() {
    this.#log.clear();
    this.#recording = true;
  }

  /** Stops keeping writes; they stay. */
  commit() {
    this.#recording = false;
    this.#log.clear();
  }

  /**
   * Undoes every write kept since `begin`. Returns the addresses of the links
   * the writes created, now deleted again.
   */
  rollback() {
    this.#recording = false;
    return this.#log.undo(this.#links);
  }

  inner() {
    return this.#links;
  }

  #created(id) {
    if (this.#recording) {
      this.#log.record(null, this.#links.getLink(id) ?? link(id, 0, 0));
    }
    return id;
  }

  queryLimits() {
    return this.#links.queryLimits();
  }

  reserveQueryAllocations(processor, restriction, substitution) {
    return this.#links.reserveQueryAllocations(processor, restriction, substitution);
  }

  create(source, target) {
    return this.#created(this.#links.create(source, target));
  }

  ensureCreated(id) {
    if (id === 0 || this.#links.exists(id)) return id;
    return this.#created(this.#links.ensureCreated(id));
  }

  tryEnsureCreated(id) {
    if (id !== 0 && this.#links.exists(id)) return id;
    return this.#created(this.#links.tryEnsureCreated(id));
  }

  getLink(id) {
    return this.#links.getLink(id);
  }

  exists(id) {
    return this.#links.exists(id);
  }

  tryCreate(source, target) {
    return this.#created(this.#links.tryCreate(source, target));
  }

  referenceExists(id) {
    return this.#links.referenceExists(id);
  }

  nextCreatedAddresses(count) {
    // Created and deleted again: nothing to undo.
    return this.#links.nextCreatedAddresses(count);
  }

  updateObserved(id, source, target, observer) {
    if (!this.#recording) return this.#links.updateObserved(id, source, target, observer);
    keepNamesBeforeUpdate(this.#log, this.#links, id, source, target);
    return this.#links.updateObserved(id, source, target, (before, after) => {
      this.#log.recordObserved(before, after);
      observer(before, after);
    });
  }

  deleteObserved(id, observer) {
    if (!this.#recording) return this.#links.deleteObserved(id, observer);
    this.#log.keepNamesOfUsages(this.#links, id);
    return this.#links.deleteObserved(id, (before, after) => {
      this.#log.recordObserved(before, after);
      observer(before, after);
    });
  }

  allLinks() {
    return this.#links.allLinks();
  }

  queryLinks(index = null, source = null, target = null) {
    return this.#links.queryLinks(index, source, target);
  }

  search(source, target) {
    return this.#links.search(source, target);
  }

  getOrCreate(source, target) {
    return this.#links.search(source, target) ?? this.create(source, target);
  }

  getName(id) {
    return this.#links.getName(id);
  }

  setName(id, name) {
    if (this.#recording) {
      this.#log.keepName(this.#links, id);
      this.#log.keepHolderOf(this.#links, name);
    }
    return this.#links.setName(id, name);
  }

  getByName(name) {
    return this.#links.getByName(name);
  }

  removeName(id) {
    if (this.#recording) this.#log.keepName(this.#links, id);
    return this.#links.removeName(id);
  }

  save() {
    return this.#links.save();
  }
}
