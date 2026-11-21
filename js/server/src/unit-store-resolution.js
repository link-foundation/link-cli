// The resolution decorators `DoubletsStorage::with_automatic_uniqueness_and_usages_resolution`
// stacks over a unit store (rust/src/storage/doublets_storage.rs), from
// `doublets` 0.21: `CascadeUniquenessAndUsagesResolver`
// (src/decorators/uniqueness.rs), `NonNullContentsLinkDeletionResolver`
// (src/decorators/deletion.rs) and `CascadeUsagesResolver`
// (src/decorators/usages.rs).
//
// A decorator forwards every operation to the layer it wraps (`links`) but
// the one it resolves. The helpers it calls on itself run through its own
// methods, as the Rust default methods do on `self`, so a merge recurses
// through the whole stack while the helpers called on `links` stay below it.
import {
  Doublets,
  enforceResetValues,
  mergeUsages,
  part,
  usagesOf,
} from './unit-store-doublets.js';

/** `decorator_struct!` and `forward!`: a layer that hands everything to `links`. */
export class Decorator extends Doublets {
  constructor(links) {
    super();
    this.links = links;
    this.type = links.type;
  }

  countLinks(query) {
    return this.links.countLinks(query);
  }

  eachLinks(query, handler) {
    return this.links.eachLinks(query, handler);
  }

  createLinks(query, handler) {
    this.links.createLinks(query, handler);
  }

  updateLinks(query, change, handler) {
    this.links.updateLinks(query, change, handler);
  }

  deleteLinks(query, handler) {
    this.links.deleteLinks(query, handler);
  }

  getLink(index) {
    return this.links.getLink(index);
  }

  sync() {
    this.links.sync();
  }

  reload() {
    this.links.reload();
  }

  close() {
    this.links.close();
  }
}

/**
 * `CascadeUsagesResolver`: deleting a link first empties and deletes every
 * link that uses it, depth first, each address once.
 */
export class CascadeUsagesResolver extends Decorator {
  deleteLinks(query, handler) {
    this.cascade(part(query, 0), [], handler);
  }

  cascade(index, visited, handler) {
    if (visited.includes(index)) return;
    visited.push(index);
    for (const usage of usagesOf(this, index)) {
      if (!this.links.exist(usage.index)) continue;
      enforceResetValues(this.links, usage.index, handler);
      this.cascade(usage.index, visited, handler);
    }
    this.links.deleteLinks([index], handler);
  }
}

/** `NonNullContentsLinkDeletionResolver`: empties a link before deleting it. */
export class NonNullContentsLinkDeletionResolver extends Decorator {
  deleteLinks(query, handler) {
    enforceResetValues(this.links, part(query, 0), handler);
    this.links.deleteLinks(query, handler);
  }
}

/**
 * `CascadeUniquenessAndUsagesResolver`: updating a link into a duplicate of
 * another one moves every usage of it to the other link and deletes it.
 */
export class CascadeUniquenessAndUsagesResolver extends Decorator {
  updateLinks(query, change, handler) {
    const old = part(query, 0);
    const replacement = this.links.search(part(change, 1), part(change, 2));
    if (replacement === null) {
      this.links.updateLinks(query, change, handler);
      return;
    }
    mergeUsages(this, old, replacement, handler);
    if (old !== replacement && this.exist(old)) this.deleteLinks([old], handler);
    const survivor = this.tryGetLink(replacement);
    handler(survivor, survivor);
  }
}

/** `with_automatic_uniqueness_and_usages_resolution` of a store. */
export const withAutomaticUniquenessAndUsagesResolution = (store) =>
  new CascadeUniquenessAndUsagesResolver(
    new NonNullContentsLinkDeletionResolver(new CascadeUsagesResolver(store)),
  );
