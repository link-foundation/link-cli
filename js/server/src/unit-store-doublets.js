// The `Doublets` trait of `doublets` 0.21 (src/data/traits.rs) with the
// decorator helpers of src/decorators/helpers.rs: everything a store layer
// gets from `count_links`, `each_links`, `create_links`, `update_links`,
// `delete_links` and `get_link`. Both the unit store (unit-store.js) and the
// resolution decorators (unit-store-resolution.js) extend `Doublets`, so a
// helper calls the methods of the layer it runs on, as the Rust default
// methods do.
//
// A query is an array of up to three parts, `[index, source, target]`, where
// `type.any` matches every value. A read handler gets a link and returns
// `false` to stop (`Flow::Break`); anything else continues. A write handler
// gets the link before and after a change and cannot stop it: every handler
// the Rust storage passes returns `Flow::Continue`, so a `Fuse` never trips.
//
// Errors are thrown as the `LinkError` the Rust `From<doublets::Error>`
// makes of them: `NotExists` as `NotFoundError`, every other error as
// `DoubletsError` ("Doublets store error: ...").
import { LinkError, NotFoundError } from './errors.js';

/** `LinkError::Doublets`: an error of the `doublets` store itself. */
export class DoubletsError extends LinkError {
  constructor(message, options) {
    super(`Doublets store error: ${message}`, options);
  }
}

/** `helpers::part`: the part at `position` of `query`, 0 when it is shorter. */
export const part = (query, position) => query[position] ?? 0;

/**
 * The provided methods of `Doublets<T>` over the required ones a subclass
 * implements: `type` (an `AddressType`), `countLinks(query)`,
 * `eachLinks(query, handler)`, `createLinks(query, handler)`,
 * `updateLinks(query, change, handler)`, `deleteLinks(query, handler)` and
 * `getLink(index)` (a link or `null`).
 */
export class Doublets {
  /** `constants().any`. */
  get any() {
    return this.type.any;
  }

  /** `count`: how many links exist. */
  count() {
    return this.countLinks([]);
  }

  /** `each_iter`: the links matching `query`, in visiting order. */
  eachIter(query) {
    const found = [];
    this.eachLinks(query, (value) => {
      found.push(value);
    });
    return found;
  }

  /** `create`: the address of a new empty link. */
  create() {
    let index = 0;
    this.createLinks([], (_before, after) => {
      index = after.index;
    });
    return index;
  }

  /** `create_link_with`: a new link `(source, target)`, every change reported. */
  createLinkWith(source, target, handler) {
    let created = 0;
    this.createLinks([], (before, after) => {
      created = after.index;
      handler(before, after);
    });
    this.updateLinks([created], [created, source, target], handler);
  }

  /** `create_link`: the link `(source, target)` the write ends with. */
  createLink(source, target) {
    let index = 0;
    this.createLinkWith(source, target, (_before, after) => {
      index = after.index;
    });
    return index;
  }

  /** `create_point`: a new link that is its own source and target. */
  createPoint() {
    const created = this.create();
    return this.update(created, created, created);
  }

  /** `update_with`. */
  updateWith(index, source, target, handler) {
    this.updateLinks([index], [index, source, target], handler);
  }

  /** `update`: the address of the link the update ends with. */
  update(index, source, target) {
    let result = 0;
    this.updateWith(index, source, target, (_before, after) => {
      result = after.index;
    });
    return result;
  }

  /** `delete_with`. */
  deleteWith(index, handler) {
    this.deleteLinks([index], handler);
  }

  /** `delete`: the address of the last link the deletion reported. */
  delete(index) {
    let result = 0;
    this.deleteWith(index, (before) => {
      result = before.index;
    });
    return result;
  }

  /** `try_get_link`: the link `index`, or `NotFoundError`. */
  tryGetLink(index) {
    const found = this.getLink(index);
    if (found === null) throw new NotFoundError(index);
    return found;
  }

  /** `find`: the first link matching `query`, or `null`. */
  find(query) {
    let found = null;
    this.eachLinks(query, (value) => {
      found = value;
      return false;
    });
    return found;
  }

  /** `search`: the address of the link `(source, target)`, or `null`. */
  search(source, target) {
    return this.find([this.any, source, target])?.index ?? null;
  }

  /** `get_or_create`. */
  getOrCreate(source, target) {
    return this.search(source, target) ?? this.createLink(source, target);
  }

  /**
   * `exist`: whether `link` is a live address. There is no external range,
   * so every address from 1 to the largest inner one is internal.
   */
  exist(link) {
    return link >= 1 && link <= this.type.maxInner && this.countLinks([link]) !== 0;
  }
}

/** `helpers::usages_of`: the other links that have `link` as source or target. */
export function usagesOf(links, link) {
  const { any } = links;
  const usages = links.eachIter([any, link, any]);
  for (const usage of links.eachIter([any, any, link])) {
    if (usage.source !== link) usages.push(usage);
  }
  return usages.filter((usage) => usage.index !== link);
}

/** `helpers::enforce_reset_values`: empties the link `index` unless it is `(0, 0)`. */
export function enforceResetValues(links, index, handler) {
  const current = links.tryGetLink(index);
  if (current.source === 0 && current.target === 0) return;
  links.updateLinks([index], [index, 0, 0], handler);
}

/** `helpers::merge_usages`: points every usage of `old` at `replacement` instead. */
export function mergeUsages(links, old, replacement, handler) {
  if (old === replacement) return;
  for (const usage of usagesOf(links, old)) {
    const source = usage.source === old ? replacement : usage.source;
    const target = usage.target === old ? replacement : usage.target;
    links.updateLinks([usage.index], [usage.index, source, target], handler);
  }
}
