// The upstream `doublets` 0.21 decorator stack a `LinkStorage` writes through
// (rust/src/link_storage_doublets.rs and `with_automatic_uniqueness_and_usages_resolution`):
// `CascadeUniquenessAndUsagesResolver` over `NonNullContentsLinkDeletionResolver`
// over `CascadeUsagesResolver` over the store. An update that would duplicate
// a doublet merges into the existing one, and a deletion resets the link and
// deletes everything that still uses it first.
//
// The store plays the `UnitedMemoryLinks` role through four primitives: its
// `links` map, `updateRaw`, `deleteRaw` and the `(before, after)` handler,
// called with the null link for "nothing" as `Link::nothing()` is.
import { NotFoundError } from './errors.js';
import { link, NULL_LINK } from './link.js';

// `LinksConstants::<u32>::external()`: the internal addresses end below the
// service constants, and the upper half of the range is external references.
export const DOUBLETS_ANY = 0x7ffffffc;
const INTERNAL_END = 0x7ffffff9;
const EXTERNAL_START = 0x7fffffff;

/** `Doublets::exist`: an external reference always exists. */
export function exist(storage, address) {
  return (
    address >= EXTERNAL_START ||
    (address >= 1 && address <= INTERNAL_END && storage.links.has(address))
  );
}

/**
 * The `(any, source, target)` lookup with the index semantics of the unit
 * store: a null part is never attached to a tree, so it matches nothing, and
 * an exact pair yields at most the lowest address.
 */
export function byPattern(storage, source, target) {
  const { links } = storage;
  if (source === DOUBLETS_ANY && target === DOUBLETS_ANY) return [...links.values()];
  if (source === DOUBLETS_ANY) return target === 0 ? [] : links.query(null, null, target);
  if (target === DOUBLETS_ANY) return source === 0 ? [] : links.query(null, source, null);
  if (source === 0 || target === 0) return [];
  const found = links.search(source, target);
  return found === null ? [] : [links.get(found)];
}

/** `Doublets::search`: the first link of `(any, source, target)`, or `null`. */
export function doubletsSearch(storage, source, target) {
  const [found] = byPattern(storage, source, target);
  return found === undefined ? null : found.index;
}

/** `helpers::usages_of`: the links using `address`, the link itself excluded. */
export function usagesOf(storage, address) {
  const usages = [
    ...byPattern(storage, address, DOUBLETS_ANY),
    ...byPattern(storage, DOUBLETS_ANY, address).filter((usage) => usage.source !== address),
  ];
  return usages.filter((usage) => usage.index !== address);
}

function getExisting(storage, index) {
  const found = storage.links.get(index);
  if (found === undefined) throw new NotFoundError(index);
  return found;
}

function storeUpdate(storage, index, source, target, handler) {
  const before = storage.updateRaw(index, source, target);
  handler(before, link(index, source, target));
}

function storeDelete(storage, index, handler) {
  handler(storage.deleteRaw(index), NULL_LINK);
}

// `helpers::enforce_reset_values`, on the store itself.
function enforceResetValues(storage, index, handler) {
  const current = getExisting(storage, index);
  if (current.source === 0 && current.target === 0) return;
  storeUpdate(storage, index, 0, 0, handler);
}

// `CascadeUsagesResolver::cascade`. `usages_of` already leaves the link out,
// so the upstream `usage.index == index` guard never holds. Its visited set
// is left out too: a link is reset to `(0 0)` before its usages are visited
// and deleted after them, so no usage can lead back to a link on the way.
function cascade(storage, index, handler) {
  for (const usage of usagesOf(storage, index)) {
    if (!exist(storage, usage.index)) continue;
    enforceResetValues(storage, usage.index, handler);
    cascade(storage, usage.index, handler);
  }
  storeDelete(storage, index, handler);
}

/** A deletion through the whole stack. */
export function resolvedDelete(storage, index, handler) {
  enforceResetValues(storage, index, handler);
  cascade(storage, index, handler);
}

/** An update through the whole stack. */
export function resolvedUpdate(storage, index, source, target, handler) {
  const found = doubletsSearch(storage, source, target);
  if (found === null) {
    storeUpdate(storage, index, source, target, handler);
    return;
  }
  // `helpers::merge_usages`, re-entering the top of the stack per usage.
  if (index !== found) {
    for (const usage of usagesOf(storage, index)) {
      resolvedUpdate(
        storage,
        usage.index,
        usage.source === index ? found : usage.source,
        usage.target === index ? found : usage.target,
        handler,
      );
    }
    if (exist(storage, index)) resolvedDelete(storage, index, handler);
  }
  const survivor = getExisting(storage, found);
  handler(survivor, survivor);
}
