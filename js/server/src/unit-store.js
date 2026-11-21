// The doublets unit store: `doublets::unit::Store` of `doublets` 0.21
// (src/mem/unit/store.rs) with its `CheckedMutation` writes
// (src/decorators/mutation.rs) and `Release`, over a database file
// (`FileMappedUnitStore` of rust/src/storage/doublets_storage.rs) or the heap.
//
// Item 0 of the memory is the `LinksHeader`; item `n` is the link `n`. A
// live link is a node of the sources tree when its source is not null and of
// the targets tree when its target is not null; a freed address below the
// highest one is an element of the free list, and freeing the highest one
// gives back every freed address under it as well. The memory grows by
// `reserve_step` items whenever only one reserved item is left.
//
// Every public operation writes its changes through to the file before it
// returns (`commit`), so the file is always what the Rust store would have in
// its mapping after the same operations.
//
// A tree that misses a link it holds (two links with the same pair, or a
// file written wrong) makes the Rust store panic on an overflow in a debug
// build. Here the write throws a `DoubletsError` instead and the store
// refuses every later operation, since the trees may now hold a cycle that
// would never end a walk.
import { link, NULL_LINK } from './link.js';
import { Doublets, DoubletsError, part } from './unit-store-doublets.js';
import {
  AddressType,
  checkAddress,
  Field,
  FileUnitMemory,
  fitsAddress,
  Header,
  HeapUnitMemory,
} from './unit-store-memory.js';
import { LinksTree, UnusedLinks } from './unit-store-trees.js';
import { AddressOverflowError } from './errors.js';

/** `Store::SIZE_STEP`: the number of items the memory grows by. */
export const RESERVE_STEP = 2 ** 20;

/** `DEFAULT_PAGE_SIZE`: the items the memory first covers to read the header. */
export const DEFAULT_PAGE_SIZE = 8 * 1024;

/** Every part of a query is an address of `type` or `type.any`. */
function checkQuery(type, query) {
  if (query.length > 3) throw new RangeError(`a query has at most 3 parts, not ${query.length}`);
  for (const value of query) {
    if (value !== type.any && !fitsAddress(type, value)) throw new AddressOverflowError(value);
  }
}

/** A `doublets::unit::Store` over a `FileUnitMemory` or a `HeapUnitMemory`. */
export class UnitStore extends Doublets {
  constructor(memory) {
    super();
    this.memory = memory;
    this.type = memory.type;
    this.reserveStep = Math.min(RESERVE_STEP, this.type.max);
    const linkAt = (index) => this.linkAt(index);
    this.sources = LinksTree.sources(memory, linkAt);
    this.targets = LinksTree.targets(memory, linkAt);
    this.unused = new UnusedLinks(memory);
    this.corruption = null;
    this.write(() => this.init());
  }

  /**
   * The store in the database file `path`, created when missing:
   * `FileMappedUnitStore::new(PersistentFileMapped::from_path(path))`.
   */
  static open(path, { addressType = AddressType.U64 } = {}) {
    const memory = FileUnitMemory.open(path, addressType);
    try {
      return new UnitStore(memory);
    } catch (error) {
      memory.close();
      throw error;
    }
  }

  /** An empty store on the heap, as over `Global` memory. */
  static inMemory({ addressType = AddressType.U64 } = {}) {
    return new UnitStore(new HeapUnitMemory(addressType));
  }

  /** `Store::init`: covers the header, then every allocated link and a step. */
  init() {
    this.memory.grow(Math.min(DEFAULT_PAGE_SIZE, this.reserveStep));
    this.memory.grow(Math.max(this.reserveStep, this.header(Header.Allocated)));
    this.setHeader(Header.Reserved, this.memory.capacity - 1);
  }

  /** Runs `action`, then writes its changes through, whether it fails or not. */
  write(action) {
    this.ensureSound();
    let result;
    try {
      result = action();
    } catch (error) {
      if (error.corrupt === true) this.corruption = error;
      throw error;
    } finally {
      this.memory.commit();
    }
    return result;
  }

  /** Throws once a write found the trees corrupt. */
  ensureSound() {
    if (this.corruption === null) return;
    const detail = this.corruption.message.replace(/^Doublets store error: /, '');
    throw new DoubletsError(`the store is corrupt: ${detail}`, { cause: this.corruption });
  }

  header(field) {
    return this.memory.get(0, field);
  }

  setHeader(field, value) {
    this.memory.set(0, field, value);
  }

  /** The link stored at `index`, live or not. */
  linkAt(index) {
    return link(index, this.memory.get(index, Field.Source), this.memory.get(index, Field.Target));
  }

  /** `get_total`: the allocated addresses that are not free. */
  total() {
    return this.header(Header.Allocated) - this.header(Header.Free);
  }

  /** `is_unused`: a freed item keeps its free-list pointers in a sourceless node. */
  isUnused(index) {
    if (index <= this.header(Header.Allocated) && this.header(Header.FirstFree) !== index) {
      return (
        this.memory.get(index, Field.SizeAsSource) === 0 &&
        this.memory.get(index, Field.Source) !== 0
      );
    }
    return true;
  }

  /** `exists`: whether `index` is a live link. */
  exists(index) {
    return index >= 1 && index <= this.header(Header.Allocated) && !this.isUnused(index);
  }

  /** `get_link`: the live link `index`, or `null`. */
  getLink(index) {
    this.ensureSound();
    checkQuery(this.type, [index]);
    return this.exists(index) ? this.linkAt(index) : null;
  }

  /** `count_links`. */
  countLinks(query) {
    this.ensureSound();
    checkQuery(this.type, query);
    if (query.length === 0) return this.total();
    const { any } = this;
    const index = query[0];
    if (query.length === 1) {
      if (index === any) return this.total();
      return this.exists(index) ? 1 : 0;
    }
    if (query.length === 2) {
      const value = query[1];
      if (index === any) {
        if (value === any) return this.total();
        return this.targets.countUsages(value) + this.sources.countUsages(value);
      }
      if (!this.exists(index)) return 0;
      if (value === any) return 1;
      const stored = this.linkAt(index);
      return stored.source === value || stored.target === value ? 1 : 0;
    }
    const [, source, target] = query;
    if (index === any) {
      if (source === any && target === any) return this.total();
      if (source === any) return this.targets.countUsages(target);
      if (target === any) return this.sources.countUsages(source);
      return this.sources.search(source, target) === 0 ? 0 : 1;
    }
    if (!this.exists(index)) return 0;
    const stored = this.linkAt(index);
    const sourceMatches = source === any || stored.source === source;
    const targetMatches = target === any || stored.target === target;
    return sourceMatches && targetMatches ? 1 : 0;
  }

  /** `each_links`: `false` when `handler` stopped the walk. */
  eachLinks(query, handler) {
    this.ensureSound();
    checkQuery(this.type, query);
    return this.eachCore(query, handler);
  }

  /** `each_core`. */
  eachCore(query, handler) {
    if (query.length === 0) return this.eachAll(handler);
    const { any } = this;
    const index = query[0];
    if (query.length === 1) {
      if (index === any) return this.eachAll(handler);
      return this.visit(this.getLink(index), handler);
    }
    if (query.length === 2) {
      const value = query[1];
      if (index === any) {
        if (value === any) return this.eachAll(handler);
        return (
          this.eachCore([any, value, any], handler) && this.eachCore([any, any, value], handler)
        );
      }
      const found = this.getLink(index);
      const matches =
        found !== null && (value === any || found.source === value || found.target === value);
      return matches ? handler(found) !== false : true;
    }
    const [, source, target] = query;
    if (index === any) {
      if (source === any && target === any) return this.eachAll(handler);
      if (source === any) return this.targets.eachUsages(target, handler);
      if (target === any) return this.sources.eachUsages(source, handler);
      return this.visit(this.getLink(this.sources.search(source, target)), handler);
    }
    const found = this.getLink(index);
    if (found === null) return true;
    const sourceMatches = source === any || found.source === source;
    const targetMatches = target === any || found.target === target;
    return sourceMatches && targetMatches ? handler(found) !== false : true;
  }

  /** Every live link in address order. */
  eachAll(handler) {
    const allocated = this.header(Header.Allocated);
    for (let index = 1; index <= allocated; index++) {
      if (this.exists(index) && handler(this.linkAt(index)) === false) return false;
    }
    return true;
  }

  /** `handler(found)` unless `found` is `null`. */
  visit(found, handler) {
    return found === null || handler(found) !== false;
  }

  /** `create_links`: reuses the first free address, or allocates the next one. */
  createLinks(_query, handler) {
    const index = this.write(() => {
      const free = this.header(Header.FirstFree);
      if (free !== 0) {
        this.unused.detach(free);
        return free;
      }
      const allocated = this.header(Header.Allocated);
      if (allocated >= this.type.maxInner) {
        throw new DoubletsError(
          `limit for the number of links in the storage has been reached: ${this.type.maxInnerText}`,
        );
      }
      if (allocated >= this.type.max) throw new AddressOverflowError(allocated + 1);
      if (allocated >= this.header(Header.Reserved) - 1) {
        this.memory.grow(this.memory.capacity + this.reserveStep);
        this.setHeader(Header.Reserved, this.memory.capacity);
      }
      this.setHeader(Header.Allocated, allocated + 1);
      return allocated + 1;
    });
    handler(NULL_LINK, link(index, 0, 0));
  }

  /** `CheckedMutation::update_links`: sets the source and target of a live link. */
  updateLinks(query, change, handler) {
    const index = part(query, 0);
    const source = checkAddress(this.type, part(change, 1));
    const target = checkAddress(this.type, part(change, 2));
    this.tryGetLink(index);
    this.updateUnchecked(index, source, target, handler);
  }

  /** `CheckedMutation::delete_links`: empties a live link, then frees it. */
  deleteLinks(query, handler) {
    const index = part(query, 0);
    const original = this.tryGetLink(index);
    if (original.source !== 0 || original.target !== 0) {
      this.updateUnchecked(index, 0, 0, () => {});
    }
    this.releaseUnchecked(index, (_before, after) => handler(original, after));
  }

  /** `update_unchecked`: moves `index` to its new places in both trees. */
  updateUnchecked(index, source, target, handler) {
    const old = this.write(() => {
      const before = this.linkAt(index);
      if (before.source !== 0) this.sources.detach(index);
      if (before.target !== 0) this.targets.detach(index);
      this.memory.set(index, Field.Source, source);
      this.memory.set(index, Field.Target, target);
      if (source !== 0) this.sources.attach(index);
      if (target !== 0) this.targets.attach(index);
      return before;
    });
    handler(old, link(index, source, target));
  }

  /** `release_unchecked`: frees `index`, which is live and `(0, 0)`. */
  releaseUnchecked(index, handler) {
    this.write(() => {
      const allocated = this.header(Header.Allocated);
      if (index < allocated) {
        this.unused.attachAsFirst(index);
        return;
      }
      this.setHeader(Header.Allocated, allocated - 1);
      for (;;) {
        const last = this.header(Header.Allocated);
        if (!(last > 0 && this.isUnused(last))) break;
        this.unused.detach(last);
        this.setHeader(Header.Allocated, last - 1);
      }
    });
    handler(link(index, 0, 0), NULL_LINK);
  }

  /** Writes everything through and `fsync`s the file. */
  sync() {
    this.memory.sync();
  }

  /** Forgets cached pages, so the next reads see the file as it is now. */
  reload() {
    this.memory.reload();
  }

  /** Writes everything through and closes the file. */
  close() {
    this.memory.close();
  }
}
