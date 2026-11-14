// Pinned types (rust/src/pinned_types.rs): the type links `(a: 1 a)` at the
// fixed addresses 1, 2, 3, ... that the names sidecar and the Unicode string
// storage are built on. `PinnedTypesDecorator` only forwards to its
// `LinkStorage`, so the names layer reads its pinned types straight off the
// store through `pinnedTypes`.

const INITIAL_SOURCE = 1;

export class PinnedTypes {
  constructor(links) {
    this.links = links;
    this.current = 1;
  }

  /** The next pinned type address, created when missing. */
  nextType() {
    const address = this.current;
    const found = this.links.get(address);
    if (found !== null) {
      if (found.source !== INITIAL_SOURCE || found.target !== address) {
        throw new Error(
          `Unexpected link found at address ${address}. Expected: (${address}: ${INITIAL_SOURCE} ${address}), Found: (${found.index}: ${found.source} ${found.target}).`,
        );
      }
    } else {
      const created = this.links.getOrCreate(INITIAL_SOURCE, address);
      if (created !== address) {
        throw new Error(
          `Pinned type address ${address} could not be created deterministically; got ${created}.`,
        );
      }
    }
    this.current += 1;
    return address;
  }

  /** `take_types`: the next `count` pinned types. */
  takeTypes(count) {
    const types = [];
    for (let i = 0; i < count; i++) types.push(this.nextType());
    return types;
  }
}

/** `PinnedTypesAccess::pinned_types` of a `LinkStorage`: its first `count` types. */
export const pinnedTypes = (links, count) => new PinnedTypes(links).takeTypes(count);
