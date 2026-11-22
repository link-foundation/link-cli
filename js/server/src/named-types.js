// `NamedTypesDecorator` (rust/src/named_types.rs): the data `LinkStorage`
// with its names kept in a sidecar `LinkStorage`, `<db stem>.names.links`
// next to the database, as Unicode strings named onto external references
// (rust/src/named_links.rs, rust/src/unicode_string_storage.rs). Deleting a
// link removes its names.
//
// Rust wraps the data store in a `PinnedTypesDecorator` and the names store
// in a `NamedLinks` per name operation; both only forward, so this layer
// holds the data store itself and opens a `UnicodeStringStorage` over the
// names store per operation, as the Rust layer does.
import { NamedTypeLinks } from './named-type-links.js';
import { isNull } from './link.js';
import { LinkStorage } from './link-storage.js';
import { pinnedTypes } from './pinned-types.js';
import { UnicodeStringStorage } from './unicode-string-storage.js';

// The trailing separators and `.` components Rust `Path` ignores.
function trimTrailing(path) {
  let trimmed = path;
  for (;;) {
    if (trimmed.length > 1 && trimmed.endsWith('/')) trimmed = trimmed.slice(0, -1);
    else if (trimmed.endsWith('/.')) trimmed = trimmed.slice(0, -2);
    else return trimmed;
  }
}

// Rust `Path::file_stem` of a final component.
function fileStem(name) {
  if (name === '..') return '';
  const dot = name.lastIndexOf('.');
  return dot <= 0 ? name : name.slice(0, dot);
}

/**
 * `make_names_database_filename`: `<parent>/<stem>.names.links`, or
 * `<stem>.names.links` for a database path without a parent.
 */
export function makeNamesDatabaseFilename(databasePath) {
  const trimmed = trimTrailing(databasePath);
  const slash = trimmed.lastIndexOf('/');
  const name = trimmed.slice(slash + 1);
  if (name === '' || name === '.') return '.names.links';
  const namesFilename = `${fileStem(name)}.names.links`;
  const parent = trimTrailing(trimmed.slice(0, slash + 1));
  if (parent === '') return namesFilename;
  return parent.endsWith('/') ? `${parent}${namesFilename}` : `${parent}/${namesFilename}`;
}

export class NamedTypesDecorator extends NamedTypeLinks {
  /**
   * The layer over a data store and its names store. `trace` writes the
   * `[TRACE] NamedTypesDecorator` lines to `stderr`.
   */
  constructor(links, namesLinks, { trace = false, stderr = process.stderr } = {}) {
    super();
    this.links = links;
    this.namesLinks = namesLinks;
    this.trace = trace;
    this.stderr = stderr;
  }

  /**
   * `NamedTypesDecorator::new` and `with_names_database_path`: opens the
   * database and its names store, `namesDatabasePath` defaulting to the
   * sidecar next to it.
   */
  static open(databasePath, { trace = false, stderr = process.stderr, namesDatabasePath } = {}) {
    const namesPath = namesDatabasePath ?? makeNamesDatabaseFilename(databasePath);
    const links = new LinkStorage(databasePath, { trace, stderr });
    const namesLinks = new LinkStorage(namesPath, { trace, stderr });
    return new NamedTypesDecorator(links, namesLinks, { trace, stderr });
  }

  traceLine(text) {
    if (this.trace) this.stderr.write(`[TRACE] NamedTypesDecorator ${text}\n`);
  }

  withIndexedLookups() {
    this.links.enableIndexedLookups();
    this.namesLinks.enableIndexedLookups();
    return this;
  }

  /** Both stores without file I/O, untraced, for a query preview. */
  queryCopy() {
    return new NamedTypesDecorator(this.links.queryCopy(), this.namesLinks.queryCopy());
  }

  reserveQueryAllocations(processor, restriction, substitution) {
    const scratch = this.queryCopy();
    processor.preview(scratch, restriction, substitution);
    return new Set(scratch.allLinks().map((value) => value.index));
  }

  save() {
    this.links.save();
    this.namesLinks.save();
  }

  /** Whether another process replaced either database since it was seen. */
  hasExternalChanges() {
    return this.links.hasExternalChanges() || this.namesLinks.hasExternalChanges();
  }

  reloadFromDisk() {
    this.links.reloadFromDisk();
    this.namesLinks.reloadFromDisk();
  }

  create(source, target) {
    return this.links.create(source, target);
  }

  ensureCreated(id) {
    return this.links.ensureCreated(id);
  }

  /** The link at `id`, or `null`. */
  get(id) {
    return this.links.get(id);
  }

  getLink(id) {
    return this.links.get(id);
  }

  exists(id) {
    return this.links.exists(id);
  }

  updateObserved(id, source, target, observer) {
    const deleted = [];
    const updated = this.links.updateObserved(id, source, target, (before, after) => {
      if (isNull(after) && !isNull(before)) deleted.push(before.index);
      observer(before, after);
    });
    this.removeNames(deleted);
    return updated;
  }

  // Rust also adds `id` when the stack did not report it deleted, but the
  // stack always deletes `id` last and reports it, so that never happens.
  deleteObserved(id, observer) {
    const deleted = [];
    const removed = this.links.deleteObserved(id, (before, after) => {
      if (isNull(after) && !isNull(before)) deleted.push(before.index);
      observer(before, after);
    });
    this.removeNames(deleted);
    return removed;
  }

  removeNames(indexes) {
    for (const index of indexes) this.removeName(index);
  }

  all() {
    return this.links.all();
  }

  allLinks() {
    return this.links.allLinks();
  }

  query(index = null, source = null, target = null) {
    return this.links.query(index, source, target);
  }

  queryLinks(index = null, source = null, target = null) {
    return this.links.queryLinks(index, source, target);
  }

  search(source, target) {
    return this.links.search(source, target);
  }

  getOrCreate(source, target) {
    return this.links.getOrCreate(source, target);
  }

  /** `PinnedTypesAccess::pinned_types` over the data store. */
  pinnedTypes(count) {
    return pinnedTypes(this.links, count);
  }

  /** Every name of a data link, by link (`logical_names`). */
  logicalNames() {
    return this.withNamedLinks((named) => named.externalNames());
  }

  withNamedLinks(action) {
    return action(new UnicodeStringStorage(this.namesLinks));
  }

  getName(id) {
    this.traceLine(`get_name for link ${id}`);
    return this.withNamedLinks((named) => named.getNameByExternalReference(id));
  }

  setName(id, name) {
    this.traceLine(`set_name for link ${id}: ${name}`);
    const existing = this.getByName(name);
    if (existing !== null && existing !== id) this.removeName(existing);
    this.removeName(id);
    return this.withNamedLinks((named) => named.setNameForExternalReference(id, name));
  }

  getByName(name) {
    this.traceLine(`get_by_name for name ${name}`);
    return this.withNamedLinks((named) => named.getExternalReferenceByName(name));
  }

  removeName(id) {
    this.traceLine(`remove_name for link ${id}`);
    this.withNamedLinks((named) => named.removeNameByExternalReference(id));
  }
}
