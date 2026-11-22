// The memory a doublets unit store lives in: the `PersistentFileMapped`
// mapping of rust/src/storage/file_mem.rs (over `platform-mem` 0.4
// `FileMapped`) and a heap counterpart, both seen as an array of
// `LinkPart<T>` items (`doublets` 0.21 src/mem/unit/raw_link.rs).
//
// An item is eight little-endian `T` fields, `repr(C)`: `source`, `target`,
// `left_as_source`, `right_as_source`, `size_as_source`, `left_as_target`,
// `right_as_target`, `size_as_target`; item 0 is the `LinksHeader`. A `u64`
// item takes 64 bytes and a `u32` item 32, so a file is byte for byte the one
// a Rust `DoubletsStorage<usize | u32, FileMappedUnitStore<_>>` writes.
//
// There is no `mmap` in Node.js, Bun or Deno, so the file is read and written
// with positioned `readSync` / `writeSync` through a page cache instead. The
// store calls `commit` at the end of every public operation, which writes
// every changed byte range through to the file; `sync` is the `fsync` of a
// flush. The file grows with `ftruncate` to exactly the sizes the mapping
// does and is never renamed or shrunk, so another process holding it open
// (a Rust mapping included) keeps a valid view.
//
// A JavaScript number holds an integer exactly only up to 2^53 - 1, so a
// `u64` field is limited to `Number.MAX_SAFE_INTEGER`: a larger value in the
// file is reported as `AddressOverflowError` when it is read rather than
// silently rounded.
import {
  closeSync,
  constants as fsConstants,
  fstatSync,
  fsyncSync,
  ftruncateSync,
  readSync,
  writeSync,
} from 'node:fs';

import { AddressOverflowError } from './errors.js';
import { openFileSync } from './open-file.js';

/** The `LinkReference` a store addresses links by. */
export const AddressType = Object.freeze({
  /**
   * `usize` on a 64-bit target. `any` is `u64::MAX - 3`, which a double
   * cannot hold exactly: it rounds to 2^64, above every storable value, so
   * it still never collides with an address.
   */
  U64: Object.freeze({
    name: 'u64',
    width: 8,
    max: Number.MAX_SAFE_INTEGER,
    any: 2 ** 64 - 4,
    maxInner: 2 ** 64 - 7,
    maxInnerText: '18446744073709551609',
  }),
  /** `u32`: every value is exact. */
  U32: Object.freeze({
    name: 'u32',
    width: 4,
    max: 0xffffffff,
    any: 0xfffffffc,
    maxInner: 0xfffffff9,
    maxInnerText: '4294967289',
  }),
});

/** The fields of a `LinkPart`, by position. */
export const Field = Object.freeze({
  Source: 0,
  Target: 1,
  LeftAsSource: 2,
  RightAsSource: 3,
  SizeAsSource: 4,
  LeftAsTarget: 5,
  RightAsTarget: 6,
  SizeAsTarget: 7,
});

/** The fields of the `LinksHeader` in item 0, by position. */
export const Header = Object.freeze({
  Allocated: 0,
  Reserved: 1,
  Free: 2,
  FirstFree: 3,
  RootAsSource: 4,
  RootAsTarget: 5,
  LastFree: 6,
});

/** `FileMapped::new` extends a shorter file to one page before mapping it. */
export const MIN_FILE_SIZE = 4096;

/** The size of a cached page; a multiple of both item sizes. */
export const PAGE_SIZE = 65536;

/** Pages kept once an operation is committed; more are dropped. */
const CACHED_PAGES = 256;

const WORD = 2 ** 32;
/** The high word of the first `u64` above `Number.MAX_SAFE_INTEGER`. */
const UNSAFE_HIGH = 2 ** 21;

/** Whether `value` is an address of `type`. */
export const fitsAddress = (type, value) =>
  Number.isSafeInteger(value) && value >= 0 && value <= type.max;

/** `value` itself, or `AddressOverflowError` when it is not an address of `type`. */
export function checkAddress(type, value) {
  if (!fitsAddress(type, value)) throw new AddressOverflowError(value);
  return value;
}

/** The `File::options().read(true).write(true).create(true)` flags. */
const READ_WRITE_CREATE = fsConstants.O_RDWR | fsConstants.O_CREAT;

/** Writes all of `buffer[from, to)` at `position`, as `write_all_at`. */
function writeAll(fd, buffer, from, to, position) {
  let offset = from;
  while (offset < to) {
    offset += writeSync(fd, buffer, offset, to - offset, position + offset - from);
  }
}

/** Fills `buffer` from `position`; whatever lies past the end of the file stays zero. */
function readAll(fd, buffer, position) {
  let offset = 0;
  while (offset < buffer.length) {
    const read = readSync(fd, buffer, offset, buffer.length - offset, position + offset);
    if (read === 0) return;
    offset += read;
  }
}

/**
 * A database file seen as `LinkPart` items. `capacity` is the number of
 * items the mapping covers (`allocated().len()`), zero until the first
 * `grow`, as for an upstream `FileMapped`.
 */
export class FileUnitMemory {
  constructor(fd, type, initializedBytes) {
    this.fd = fd;
    this.type = type;
    this.width = type.width;
    this.itemSize = type.width * 8;
    this.capacity = 0;
    // `PersistentFileMapped::initialized_bytes`: the length of the file
    // before its padding, then of every completed growth.
    this.initializedBytes = initializedBytes;
    this.pages = new Map();
    this.dirty = new Map();
  }

  /** `PersistentFileMapped::from_path`: opens or creates `path` without truncating it. */
  static open(path, type) {
    const fd = openFileSync(path, READ_WRITE_CREATE, 0o666);
    try {
      const { size } = fstatSync(fd);
      if (size < MIN_FILE_SIZE) ftruncateSync(fd, MIN_FILE_SIZE);
      return new FileUnitMemory(fd, type, size);
    } catch (error) {
      closeSync(fd);
      throw error;
    }
  }

  /** The cached page `number`, read from the file on first use. */
  page(number) {
    let page = this.pages.get(number);
    if (page === undefined) {
      page = Buffer.alloc(PAGE_SIZE);
      readAll(this.fd, page, number * PAGE_SIZE);
      this.pages.set(number, page);
    }
    return page;
  }

  /** Marks the bytes `[from, to)` of page `number` as changed. */
  touch(number, from, to) {
    const range = this.dirty.get(number);
    if (range === undefined) this.dirty.set(number, [from, to]);
    else {
      range[0] = Math.min(range[0], from);
      range[1] = Math.max(range[1], to);
    }
  }

  /** Field `field` of item `item`. */
  get(item, field) {
    const offset = item * this.itemSize + field * this.width;
    const page = this.page(Math.floor(offset / PAGE_SIZE));
    const at = offset % PAGE_SIZE;
    const low = page.readUInt32LE(at);
    if (this.width === 4) return low;
    const high = page.readUInt32LE(at + 4);
    if (high >= UNSAFE_HIGH) {
      throw new AddressOverflowError(((BigInt(high) << 32n) | BigInt(low)).toString());
    }
    return high * WORD + low;
  }

  /** Sets field `field` of item `item` to `value`, an address of the type. */
  set(item, field, value) {
    const offset = item * this.itemSize + field * this.width;
    const number = Math.floor(offset / PAGE_SIZE);
    const page = this.page(number);
    const at = offset % PAGE_SIZE;
    page.writeUInt32LE(value % WORD, at);
    if (this.width === 8) page.writeUInt32LE(Math.floor(value / WORD), at + 4);
    this.touch(number, at, at + this.width);
  }

  /** Zeroes the file bytes `[from, to)`. */
  zero(from, to) {
    for (let offset = from; offset < to;) {
      const number = Math.floor(offset / PAGE_SIZE);
      const at = offset % PAGE_SIZE;
      const end = Math.min(PAGE_SIZE, at + to - offset);
      this.page(number).fill(0, at, end);
      this.touch(number, at, end);
      offset += end - at;
    }
  }

  /**
   * `resize_mem` through `PersistentFileMapped::grow_filled`: covers
   * `capacity` items, extending the file to exactly `capacity * itemSize`
   * bytes when it is shorter. Only the items the file did not hold before
   * are filled with zeros; past the original length the file is zero
   * already, so only a trailing partial item of it is written.
   */
  grow(capacity) {
    if (capacity <= this.capacity) return;
    const size = capacity * this.itemSize;
    if (fstatSync(this.fd).size < size) ftruncateSync(this.fd, size);
    const kept = Math.max(this.capacity, Math.floor(this.initializedBytes / this.itemSize));
    this.zero(kept * this.itemSize, Math.min(size, this.initializedBytes));
    this.initializedBytes = Math.max(this.initializedBytes, size);
    this.capacity = capacity;
  }

  /** Writes every changed byte through to the file. */
  commit() {
    for (const [number, [from, to]] of this.dirty) {
      writeAll(this.fd, this.pages.get(number), from, to, number * PAGE_SIZE + from);
    }
    this.dirty.clear();
    if (this.pages.size > CACHED_PAGES) this.pages.clear();
  }

  /** `File::sync_all` of the database file. */
  sync() {
    this.commit();
    fsyncSync(this.fd);
  }

  /** Forgets the cached pages so the next reads see what other processes wrote. */
  reload() {
    this.commit();
    this.pages.clear();
  }

  /** Writes everything through and closes the file. */
  close() {
    if (this.fd === null) return;
    this.commit();
    closeSync(this.fd);
    this.fd = null;
  }
}

/**
 * `LinkPart` items on the heap, the `Global` memory of an in-memory store.
 * Items are kept sparsely, so `capacity` costs nothing until written.
 */
export class HeapUnitMemory {
  constructor(type) {
    this.type = type;
    this.capacity = 0;
    this.items = new Map();
  }

  get(item, field) {
    return this.items.get(item)?.[field] ?? 0;
  }

  set(item, field, value) {
    let fields = this.items.get(item);
    if (fields === undefined) {
      fields = new Array(8).fill(0);
      this.items.set(item, fields);
    }
    fields[field] = value;
  }

  grow(capacity) {
    this.capacity = Math.max(this.capacity, capacity);
  }

  commit() {}

  sync() {}

  reload() {}

  close() {}
}
