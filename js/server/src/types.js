// Types over links (rust/src/types.rs): a type is a link `(type: parent
// type)` -- its source is its parent, its target itself -- and an instance
// `(instance: type instance)` is a value of its source. More parents are
// pairs `(type parent)` marked by `(Extends pair)`. The root of the types
// and the `Extends` marker are named points of the store, found or made by
// `TypeMarkers.ensure`.
import { NamedTypeLinks } from './named-type-links.js';

export const ROOT_TYPE_NAME = 'Type';

export const EXTENDS_NAME = 'Extends';

/** The addresses of the root type and of the `Extends` marker (`null` without one). */
export class TypeMarkers {
  constructor(root, extendsMarker = null) {
    this.root = root;
    this.extends = extendsMarker;
  }

  /** The named markers of `links`, created when missing. */
  static ensure(links) {
    return new TypeMarkers(ensurePoint(links, ROOT_TYPE_NAME), ensurePoint(links, EXTENDS_NAME));
  }
}

/** The point `(id: id id)` named `name`, created and named when missing. */
export function ensurePoint(links, name) {
  const known = links.getByName(name);
  if (known != null && known !== 0 && links.exists(known)) return known;
  const id = links.tryCreate(0, 0);
  links.update(id, id, id);
  links.setName(id, name);
  return id;
}

function stored(links, id) {
  return id === 0 ? null : links.getLink(id);
}

function ensureExists(links, id) {
  const found = stored(links, id);
  if (found == null) throw new Error(`Link ${id} does not exist.`);
  return found;
}

/**
 * A store with the `Types` operations over the markers it was given; every
 * `NamedTypeLinks` method goes to the store underneath.
 */
export class TypesDecorator extends NamedTypeLinks {
  constructor(links, root, extendsMarker = null) {
    super();
    this.links = links;
    this.typeMarkers = new TypeMarkers(root, extendsMarker);
  }

  static withMarkers(links, markers) {
    return new TypesDecorator(links, markers.root, markers.extends);
  }

  /** Over the named markers of `links`, created when missing. */
  static withNamedMarkers(links) {
    return TypesDecorator.withMarkers(links, TypeMarkers.ensure(links));
  }

  markers() {
    return this.typeMarkers;
  }

  root() {
    return this.typeMarkers.root;
  }

  extends() {
    return this.typeMarkers.extends;
  }

  /** The type of a stored link: its source. */
  typeOf(id) {
    const found = stored(this, id);
    return found == null ? null : found.source;
  }

  /** Whether `id` is stored as a type, `(id: parent id)`. */
  isType(id) {
    const found = stored(this, id);
    return found != null && found.target === id;
  }

  /** The direct parents of a type: its source, then the types it extends. */
  parentsOf(typeId) {
    const parents = [];
    const found = stored(this, typeId);
    if (found == null) return parents;
    if (found.source !== typeId && found.source !== 0) parents.push(found.source);
    this.addExtendedTypes(typeId, parents);
    return parents;
  }

  /** `typeId` and every type above it, nearest first. */
  ancestorsOf(typeId) {
    const ancestors = [typeId];
    const seen = new Set(ancestors);
    for (let i = 0; i < ancestors.length; i++) {
      for (const parent of this.parentsOf(ancestors[i])) {
        if (!seen.has(parent)) {
          seen.add(parent);
          ancestors.push(parent);
        }
      }
    }
    return ancestors;
  }

  /** Whether `id` is a pair marked by `Extends`. */
  isExtension(id) {
    const extendsMarker = this.extends();
    return extendsMarker != null && id !== 0 && this.search(extendsMarker, id) != null;
  }

  isOf(id, typeId) {
    const found = stored(this, id);
    return found != null && this.isOfLink(found, typeId);
  }

  /**
   * Whether `value` is of `typeId`: a link is of its source and of every
   * type it extends itself, and of their ancestors; an `Extends` pair is of
   * type `Extends` only.
   */
  isOfLink(value, typeId) {
    const starts = [];
    if (this.isExtension(value.index)) {
      starts.push(this.extends());
    } else {
      starts.push(value.source);
      if (value.index !== 0 && this.exists(value.index)) this.addExtendedTypes(value.index, starts);
    }
    const stack = starts.reverse();
    const seen = new Set();
    while (stack.length > 0) {
      const at = stack.pop();
      if (at === typeId) return true;
      if (at === 0 || seen.has(at)) continue;
      seen.add(at);
      stack.push(...this.parentsOf(at));
    }
    return false;
  }

  /** Whether a stored link is `typeId` itself or of it. */
  isA(id, typeId) {
    return stored(this, id) != null && (id === typeId || this.isOf(id, typeId));
  }

  isALink(value, typeId) {
    return value.index === typeId || this.isOfLink(value, typeId);
  }

  /** A new type `(id: parent id)`, extending each of `more`. */
  define(parent, more = []) {
    if (more.length > 0 && this.extends() == null) {
      throw new Error('Multiple parents need an Extends marker.');
    }
    ensureExists(this, parent);
    for (const other of more) ensureExists(this, other);
    const id = this.tryCreate(0, 0);
    this.update(id, parent, id);
    for (const other of more) this.extend(id, other);
    return id;
  }

  defineRoot() {
    return this.define(this.root(), []);
  }

  /** Marks `typeId` as extending `parent`: the pair `(typeId parent)` and its `Extends` mark. */
  extend(typeId, parent) {
    const extendsMarker = this.extends();
    if (extendsMarker == null) throw new Error('Multiple parents need an Extends marker.');
    ensureExists(this, typeId);
    const parentLink = ensureExists(this, parent);
    if (parentLink.source === typeId && parentLink.target === parent) {
      throw new Error(
        `Type ${parent} is a direct sub-type of ${typeId}: the pair (${typeId} ${parent}) would be its definition.`,
      );
    }
    const pair = this.tryGetOrCreate(typeId, parent);
    this.tryGetOrCreate(extendsMarker, pair);
    return pair;
  }

  /** A new value `(id: typeId id)`. */
  instance(typeId) {
    ensureExists(this, typeId);
    const id = this.tryCreate(0, 0);
    this.update(id, typeId, id);
    return id;
  }

  /** The value `(typeId target)`, found or created. */
  instanceWithTarget(typeId, target) {
    ensureExists(this, typeId);
    ensureExists(this, target);
    return this.tryGetOrCreate(typeId, target);
  }

  addExtendedTypes(typeId, parents) {
    if (this.extends() == null || typeId === 0) return;
    for (const pair of this.queryLinks(null, typeId, null)) {
      if (pair.index !== typeId && pair.target !== 0 && this.isExtension(pair.index)) {
        parents.push(pair.target);
      }
    }
  }

  create(source, target) {
    return this.links.create(source, target);
  }

  ensureCreated(id) {
    return this.links.ensureCreated(id);
  }

  tryEnsureCreated(id) {
    return this.links.tryEnsureCreated(id);
  }

  getLink(id) {
    return this.links.getLink(id);
  }

  exists(id) {
    return this.links.exists(id);
  }

  tryCreate(source, target) {
    return this.links.tryCreate(source, target);
  }

  tryGetOrCreate(source, target) {
    return this.links.tryGetOrCreate(source, target);
  }

  referenceExists(id) {
    return this.links.referenceExists(id);
  }

  nextCreatedAddresses(count) {
    return this.links.nextCreatedAddresses(count);
  }

  updateObserved(id, source, target, observer) {
    return this.links.updateObserved(id, source, target, observer);
  }

  deleteObserved(id, observer) {
    return this.links.deleteObserved(id, observer);
  }

  allLinks() {
    return this.links.allLinks();
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

  getName(id) {
    return this.links.getName(id);
  }

  setName(id, name) {
    return this.links.setName(id, name);
  }

  getByName(name) {
    return this.links.getByName(name);
  }

  removeName(id) {
    this.links.removeName(id);
  }

  save() {
    this.links.save();
  }

  getOrCreateNamed(name) {
    return this.links.getOrCreateNamed(name);
  }
}
