// The links map of a `LinkStorage` with its exact-pair, source and target
// indexes (rust/src/indexed_links.rs). Rust keeps the indexes optional and
// scans a plain map without them; the answers are the same either way, so
// this map always keeps them and `with_indexed_lookups` only decides the
// trace line. Every write goes through `insert` or `remove`, so the indexes
// never go stale -- including the writes of the uniqueness and usages
// resolution stack.

const ascending = (left, right) => left - right;

const pairKey = (source, target) => `${source} ${target}`;

function addAddress(index, key, address) {
  let addresses = index.get(key);
  if (addresses === undefined) {
    addresses = new Set();
    index.set(key, addresses);
  }
  addresses.add(address);
}

function removeAddress(index, key, address) {
  const addresses = index.get(key);
  addresses.delete(address);
  if (addresses.size === 0) index.delete(key);
}

export class IndexedLinks {
  constructor() {
    this.links = new Map();
    this.pairs = new Map();
    this.sources = new Map();
    this.targets = new Map();
    this.sorted = null;
  }

  get size() {
    return this.links.size;
  }

  has(id) {
    return this.links.has(id);
  }

  /** The link at `id`, or `undefined`. */
  get(id) {
    return this.links.get(id);
  }

  /** Stores `value` at its index, returning the link it replaced. */
  insert(value) {
    const previous = this.remove(value.index);
    addAddress(this.pairs, pairKey(value.source, value.target), value.index);
    addAddress(this.sources, value.source, value.index);
    addAddress(this.targets, value.target, value.index);
    this.links.set(value.index, value);
    this.sorted = null;
    return previous;
  }

  /** Removes the link at `id`, returning it, or `undefined`. */
  remove(id) {
    const value = this.links.get(id);
    if (value === undefined) return undefined;
    this.links.delete(id);
    this.sorted = null;
    removeAddress(this.pairs, pairKey(value.source, value.target), id);
    removeAddress(this.sources, value.source, id);
    removeAddress(this.targets, value.target, id);
    return value;
  }

  /** Every link by address. The array is shared: do not modify it. */
  values() {
    if (this.sorted === null) {
      this.sorted = [...this.links.values()].sort((left, right) => left.index - right.index);
    }
    return this.sorted;
  }

  /** The lowest address holding exactly `(source, target)`, or `null`. */
  search(source, target) {
    const addresses = this.pairs.get(pairKey(source, target));
    if (addresses === undefined) return null;
    let lowest = Infinity;
    for (const address of addresses) lowest = Math.min(lowest, address);
    return lowest;
  }

  /** The links matching a pattern, `null` matching any value, by address. */
  query(index = null, source = null, target = null) {
    if (index != null) {
      const value = this.links.get(index);
      return value !== undefined &&
        (source == null || value.source === source) &&
        (target == null || value.target === target)
        ? [value]
        : [];
    }
    let addresses;
    if (source != null && target != null) addresses = this.pairs.get(pairKey(source, target));
    else if (source != null) addresses = this.sources.get(source);
    else if (target != null) addresses = this.targets.get(target);
    else return [...this.values()];
    if (addresses === undefined) return [];
    return [...addresses].sort(ascending).map((address) => this.links.get(address));
  }

  /** An independent map holding the same links. */
  copy() {
    const copy = new IndexedLinks();
    for (const value of this.values()) copy.insert(value);
    return copy;
  }
}
