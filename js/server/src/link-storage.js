// `LinkStorage`, the text links database (rust/src/link_storage.rs): links and
// their names held in memory, loaded from and saved to one file of
// `(index source target "name")` rows. Writes go through the upstream
// uniqueness and usages resolution stack (link-storage-doublets.js), and the
// allocator hands out freed addresses again exactly as the C# store does,
// because the address a query reports for a created link depends on it.
//
// A save writes `<db>.saving` and renames it over the database, so a process
// stopped in the middle leaves the previous complete file (issue #108).
import {
  closeSync,
  existsSync,
  openSync,
  readFileSync,
  renameSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { ioErrorText } from './error-report.js';
import { LinkError, NotFoundError } from './errors.js';
import { IndexedLinks } from './indexed-links.js';
import { link, parseU32 } from './link.js';
import { resolvedDelete, resolvedUpdate } from './link-storage-doublets.js';
import { NamedTypeLinks } from './named-type-links.js';
import {
  NAME_HEADER,
  nameMetadata,
  needsNameMetadata,
  parseLinkLine,
  parseNameMetadata,
  splitWhitespace,
  trim,
} from './text-storage-format.js';

/** Prefix of the database line that records the freed addresses. */
const UNUSED_HEADER = '# unused:';

const utf8 = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });

/** The revision of a file a store has not seen yet: `StorageRevision::default()`. */
export const NO_REVISION = Object.freeze({ len: 0, modified: 0n });

/**
 * `StorageRevision::of`: the size and modification time of `path`, or
 * `NO_REVISION` when it does not exist.
 */
export function storageRevision(path) {
  let stats;
  try {
    stats = statSync(path, { bigint: true });
  } catch (error) {
    if (error.code === 'ENOENT') return NO_REVISION;
    throw new LinkError(`I/O error: ${ioErrorText(error)}`, { cause: error });
  }
  return Object.freeze({ len: Number(stats.size), modified: stats.mtimeNs });
}

export const sameRevision = (left, right) =>
  left.len === right.len && left.modified === right.modified;

/** The lines of `bytes` as `BufRead::lines` yields them, without decoding. */
function rawLines(bytes) {
  const lines = [];
  let start = 0;
  while (start < bytes.length) {
    let end = bytes.indexOf(0x0a, start);
    if (end === -1) end = bytes.length;
    // A carriage return before the line feed goes with the trim of the line.
    lines.push(bytes.subarray(start, end));
    start = end + 1;
  }
  return lines;
}

/** The addresses of an `UNUSED_HEADER` line, in the allocator's stack order. */
function parseUnusedHeader(addresses) {
  return splitWhitespace(addresses)
    .map(parseU32)
    .filter((address) => address !== null)
    .reverse();
}

export class LinkStorage extends NamedTypeLinks {
  /**
   * Opens the database at `dbPath`, loading it when the file exists.
   * `trace` writes the `[TRACE]` lines of the Rust store to `stderr`.
   */
  constructor(dbPath, { trace = false, stderr = process.stderr } = {}) {
    super();
    this.links = new IndexedLinks();
    this.names = new Map();
    this.nameToId = new Map();
    // The highest address handed out and not given back, and the freed
    // addresses below it, most recently freed last.
    this.allocated = 0;
    this.unused = [];
    this.dbPath = dbPath;
    this.revision = NO_REVISION;
    this.trace = trace;
    this.stderr = stderr;
    this.indexedLookups = false;
    // An empty path never exists, which is what makes the store in-memory.
    if (existsSync(dbPath)) this.load();
    this.revision = storageRevision(dbPath);
  }

  /** A store that never reads or writes a file; `save` keeps nothing. */
  static inMemory({ trace = false, stderr = process.stderr } = {}) {
    return new LinkStorage('', { trace, stderr });
  }

  traceLine(text) {
    if (this.trace) this.stderr.write(`[TRACE] ${text}\n`);
  }

  /** The exact recycling state, without file I/O, for a query preview. */
  queryCopy() {
    const copy = LinkStorage.inMemory();
    copy.links = this.links.copy();
    copy.names = new Map(this.names);
    copy.nameToId = new Map(this.nameToId);
    copy.allocated = this.allocated;
    copy.unused = [...this.unused];
    copy.indexedLookups = this.indexedLookups;
    return copy;
  }

  reserveQueryAllocations(processor, restriction, substitution) {
    const scratch = this.queryCopy();
    processor.preview(scratch, restriction, substitution);
    return new Set(scratch.allLinks().map((value) => value.index));
  }

  /**
   * `with_indexed_lookups`. The lookups are always indexed here; the call
   * keeps the trace line and survives a reload.
   */
  withIndexedLookups() {
    this.enableIndexedLookups();
    return this;
  }

  enableIndexedLookups() {
    this.indexedLookups = true;
    this.traceLine(`IndexedLinksDecorator enabled for ${this.links.size} links`);
  }

  isInMemory() {
    return this.dbPath === '';
  }

  /** The database file, `''` for an in-memory store. */
  databasePath() {
    return this.dbPath;
  }

  isTraceEnabled() {
    return this.trace;
  }

  /** The revision of the database file seen at the last load or save. */
  observedRevision() {
    return this.revision;
  }

  /** Marks the current file as seen by `hasExternalChanges`. */
  refreshObservedRevision() {
    if (!this.isInMemory()) this.revision = storageRevision(this.dbPath);
  }

  /** Whether another process replaced the database since it was seen. */
  hasExternalChanges() {
    return !this.isInMemory() && !sameRevision(storageRevision(this.dbPath), this.revision);
  }

  /** Replaces the state only after the whole database loads. */
  reloadFromDisk() {
    const loaded = new LinkStorage(this.dbPath, { trace: this.trace, stderr: this.stderr });
    if (this.indexedLookups) loaded.enableIndexedLookups();
    Object.assign(this, loaded);
  }

  load() {
    const path = this.dbPath;
    let fd;
    try {
      fd = openSync(path, 'r');
    } catch (error) {
      throw new Error(`Failed to open database: ${path}`, { cause: error });
    }
    let bytes;
    try {
      bytes = readFileSync(fd);
    } catch (error) {
      throw new Error(`Failed to read ${path} at line 1`, { cause: error });
    } finally {
      closeSync(fd);
    }
    let recordedUnused = null;
    rawLines(bytes).forEach((raw, number) => {
      let text;
      try {
        text = utf8.decode(raw);
      } catch {
        throw new Error(`Failed to read ${path} at line ${number + 1}`, {
          cause: new Error('stream did not contain valid UTF-8'),
        });
      }
      const line = trim(text);
      if (line.startsWith(UNUSED_HEADER)) {
        recordedUnused = parseUnusedHeader(line.slice(UNUSED_HEADER.length));
        return;
      }
      if (line.startsWith(NAME_HEADER)) {
        let metadata;
        try {
          metadata = parseNameMetadata(line.slice(NAME_HEADER.length));
        } catch (error) {
          throw new Error(`Invalid name metadata in ${path} at line ${number + 1}`, {
            cause: error,
          });
        }
        this.setName(metadata[0], metadata[1]);
        return;
      }
      if (line === '' || line.startsWith('#')) return;
      const row = parseLinkLine(line);
      if (row === null) return;
      this.links.insert(row.link);
      this.allocated = Math.max(this.allocated, row.link.index);
      if (row.name !== null) this.setName(row.link.index, row.name);
    });
    this.unused = this.restoreUnused(recordedUnused);
    this.traceLine(`Loaded ${this.links.size} links from ${path}`);
  }

  /**
   * The freed-address stack after a load: the recorded one without the
   * addresses in use or past the end, or the missing addresses below the
   * highest link, lowest reused first, for a file that records none.
   */
  restoreUnused(recorded) {
    if (recorded !== null) {
      const seen = new Set();
      return recorded.filter(
        (address) =>
          address > 0 &&
          address < this.allocated &&
          !this.links.has(address) &&
          !seen.has(address) &&
          seen.add(address),
      );
    }
    const unused = [];
    for (let address = this.allocated - 1; address >= 1; address--) {
      if (!this.links.has(address)) unused.push(address);
    }
    return unused;
  }

  /** The next address: a freed one first, as `AllocateLink` in C#. */
  allocate() {
    if (this.unused.length > 0) return this.unused.pop();
    this.allocated += 1;
    return this.allocated;
  }

  /**
   * Gives `address` back. Freeing the highest address shrinks the store and
   * takes every freed address that became the new end with it.
   */
  release(address) {
    if (address === 0 || address > this.allocated) return;
    if (address < this.allocated) {
      this.unused.push(address);
      return;
    }
    this.allocated = address - 1;
    for (;;) {
      const position = this.unused.indexOf(this.allocated);
      if (position === -1) return;
      this.unused.splice(position, 1);
      this.allocated -= 1;
    }
  }

  /** The text `save` writes. */
  databaseText() {
    let text = '';
    if (this.unused.length > 0) {
      text += `${UNUSED_HEADER} ${[...this.unused].reverse().join(' ')}\n`;
    }
    for (const value of this.links.values()) {
      const name = this.names.get(value.index);
      text +=
        name !== undefined && !needsNameMetadata(name)
          ? `(${value.index} ${value.source} ${value.target} "${name}")\n`
          : `(${value.index} ${value.source} ${value.target})\n`;
    }
    const metadata = [...this.names]
      .filter(([id, name]) => !this.links.has(id) || needsNameMetadata(name))
      .sort(([left], [right]) => left - right);
    for (const [id, name] of metadata) text += `${nameMetadata(id, name)}\n`;
    return text;
  }

  save() {
    if (this.isInMemory()) return;
    const saving = `${this.dbPath}.saving`;
    let fd;
    try {
      fd = openSync(saving, 'w');
    } catch (error) {
      throw new Error(`Failed to create database: ${saving}`, { cause: error });
    }
    try {
      writeFileSync(fd, this.databaseText());
    } finally {
      closeSync(fd);
    }
    try {
      renameSync(saving, this.dbPath);
    } catch (error) {
      throw new Error(`Failed to replace database: ${this.dbPath}`, { cause: error });
    }
    this.traceLine(`Saved ${this.links.size} links to ${this.dbPath}`);
  }

  /** Creates `(source target)` at the next address. */
  create(source, target) {
    const id = this.allocate();
    this.links.insert(link(id, source, target));
    this.traceLine(`Created link: (${id} ${source} ${target})`);
    return id;
  }

  /**
   * Creates `(id: 0 0)` by asking the allocator until it hands out `id`, and
   * frees the addresses passed on the way in the order they were created --
   * `ILinksExtensions.EnsureCreated` in C#.
   */
  ensureCreated(id) {
    if (id === 0 || this.links.has(id)) return id;
    const passedOver = [];
    for (;;) {
      const created = this.create(0, 0);
      if (created === id) break;
      passedOver.push(created);
    }
    for (const address of passedOver) this.deleteRaw(address);
    this.traceLine(`Ensured link: (${id} 0 0)`);
    return id;
  }

  /** The link at `id`, or `null`. */
  get(id) {
    return this.links.get(id) ?? null;
  }

  getLink(id) {
    return this.get(id);
  }

  exists(id) {
    return this.links.has(id);
  }

  /** Writes `(source target)` at `id` with no policy; the link before. */
  updateRaw(id, source, target) {
    const before = this.links.get(id);
    if (before === undefined) throw new NotFoundError(id);
    this.traceLine(
      `Updating link ${id} from (${before.source} ${before.target}) to (${source} ${target})`,
    );
    this.links.insert(link(id, source, target));
    return before;
  }

  /** Removes exactly `id` and its name, with no policy; the removed link. */
  deleteRaw(id) {
    const name = this.names.get(id);
    if (name !== undefined) {
      this.names.delete(id);
      this.nameToId.delete(name);
    }
    const removed = this.links.remove(id);
    if (removed === undefined) throw new NotFoundError(id);
    this.release(id);
    this.traceLine(`Deleted link: (${removed.index} ${removed.source} ${removed.target})`);
    return removed;
  }

  /**
   * An update through the uniqueness and usages resolution stack, reporting
   * every change. The duplicate a link merges into comes back from the stack
   * as an unchanged pair, which C# does not report, so neither does this.
   */
  updateObserved(id, source, target, observer) {
    const before = this.links.get(id);
    if (before === undefined) throw new NotFoundError(id);
    resolvedUpdate(this, id, source, target, (changedBefore, after) => {
      const mergedInto =
        changedBefore.index === after.index &&
        changedBefore.source === after.source &&
        changedBefore.target === after.target &&
        changedBefore.index !== id;
      if (!mergedInto) observer(changedBefore, after);
    });
    return before;
  }

  /** A deletion that cascades to every link still using `id`. */
  deleteObserved(id, observer) {
    const before = this.links.get(id);
    if (before === undefined) throw new NotFoundError(id);
    resolvedDelete(this, id, observer);
    return before;
  }

  /** Every link by address. */
  all() {
    return [...this.links.values()];
  }

  allLinks() {
    return this.all();
  }

  /** The links matching a pattern, `null` matching any value, by address. */
  query(index = null, source = null, target = null) {
    return this.links.query(index, source, target);
  }

  queryLinks(index = null, source = null, target = null) {
    return this.query(index, source, target);
  }

  /** The lowest address holding exactly `(source target)`, or `null`. */
  search(source, target) {
    return this.links.search(source, target);
  }

  getOrCreate(source, target) {
    return this.search(source, target) ?? this.create(source, target);
  }

  /** `(index source target)` with names in place of the addresses that have one. */
  format(value) {
    const reference = (id) => this.names.get(id) ?? String(id);
    return `(${reference(value.index)} ${reference(value.source)} ${reference(value.target)})`;
  }

  printAllLinks(out = process.stdout) {
    for (const value of this.links.values()) out.write(`${this.format(value)}\n`);
  }

  printChange(before, after, out = process.stdout) {
    const text = (value) => (value == null ? '' : this.format(value));
    out.write(`(${text(before)}) (${text(after)})\n`);
  }

  /** The link named `name`, created as a self-referencing point when missing. */
  getOrCreateNamed(name) {
    const found = this.nameToId.get(name);
    if (found !== undefined) return found;
    const id = this.create(0, 0);
    try {
      this.update(id, id, id);
    } catch {
      // Rust drops the error (`.ok()`), and keeps the name.
    }
    this.names.set(id, name);
    this.nameToId.set(name, id);
    this.traceLine(`Created named link: ${name} => ${id}`);
    return id;
  }

  /** Names `id`, taking the name away from any other link that had it. */
  setName(id, name) {
    const old = this.names.get(id);
    if (old !== undefined) {
      this.names.delete(id);
      this.nameToId.delete(old);
    }
    const previous = this.nameToId.get(name);
    if (previous !== undefined) this.names.delete(previous);
    this.nameToId.set(name, id);
    this.names.set(id, name);
    this.traceLine(`Set name: ${id} => ${name}`);
    return id;
  }

  getName(id) {
    return this.names.get(id) ?? null;
  }

  getByName(name) {
    return this.nameToId.get(name) ?? null;
  }

  removeName(id) {
    const name = this.names.get(id);
    if (name === undefined) return;
    this.names.delete(id);
    this.nameToId.delete(name);
    this.traceLine(`Removed name: ${id} => ${name}`);
  }
}
