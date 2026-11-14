// `NamedTypeLinks`, the storage contract every layer of the CLI is written
// against, with the default methods of the Rust trait
// (rust/src/named_type_links.rs). A layer -- names, transactions, version
// control, permissions, a remote store -- extends this class (or implements
// the same methods) and can be dropped anywhere in the stack.
//
// Every method is synchronous, as in Rust: the CLI and the embedded API run a
// query to completion before they answer. Addresses are numbers in
// `0..=0xffffffff`; a link is a frozen `{ index, source, target }`.
//
// A change observer is `(before, after) => void`, where `before`/`after` are
// links or `null` for "no link": `(null, link)` is a creation and
// `(link, null)` a deletion.
import { writeFileSync } from 'node:fs';
import { formatStructure } from './structure-format.js';
import { InvalidFormatError } from './errors.js';
import { isU32Text, MAX_ADDRESS } from './link.js';
import { encodeReferenceLiteral, formatReference } from './notation.js';

const ignore = () => {};

function validateEnsureAddress(id) {
  if (id === 0 || id === MAX_ADDRESS) {
    throw new InvalidFormatError(`Cannot ensure unsupported link address ${id}`);
  }
}

/**
 * The text of a name used as a reference. The notation decides how to
 * delimit it, except for digits that would otherwise read as an address and
 * for mixed quotes, whose literal form the C# port has always written.
 */
export function escapeLinoReference(reference) {
  if (reference === '') return '';
  if (isU32Text(reference)) return `'${reference}'`;
  if (reference.includes("'") && reference.includes('"')) return encodeReferenceLiteral(reference);
  return formatReference(reference);
}

export class NamedTypeLinks {
  /** A checked transport may restrict numeric input and query expansion. */
  queryLimits() {
    return null;
  }

  /**
   * Reserve the actual allocation sequence without staging observable rows:
   * a `Set` of the addresses the query would allocate, or `null`.
   */
  reserveQueryAllocations(_processor, _restriction, _substitution) {
    return null;
  }

  create(_source, _target) {
    throw new Error(`${this.constructor.name} does not implement create`);
  }

  tryCreate(source, target) {
    return this.create(source, target);
  }

  ensureCreated(_id) {
    throw new Error(`${this.constructor.name} does not implement ensureCreated`);
  }

  tryEnsureCreated(id) {
    validateEnsureAddress(id);
    return this.ensureCreated(id);
  }

  /** Whether a reference of a query to `id` names a link that exists (issue #161). */
  referenceExists(id) {
    return this.exists(id);
  }

  /** Whether `id` is an address of this store; `null` for a plain store. */
  claims(_id) {
    return null;
  }

  /** The addresses the next `count` links created in the store get. */
  nextCreatedAddresses(count) {
    const addresses = [];
    for (let i = 0; i < count; i++) addresses.push(this.tryCreate(0, 0));
    for (const address of [...addresses].reverse()) this.delete(address);
    return addresses;
  }

  getLink(_id) {
    throw new Error(`${this.constructor.name} does not implement getLink`);
  }

  exists(_id) {
    throw new Error(`${this.constructor.name} does not implement exists`);
  }

  update(id, source, target) {
    return this.updateObserved(id, source, target, ignore);
  }

  updateObserved(_id, _source, _target, _observer) {
    throw new Error(`${this.constructor.name} does not implement updateObserved`);
  }

  delete(id) {
    return this.deleteObserved(id, ignore);
  }

  deleteObserved(_id, _observer) {
    throw new Error(`${this.constructor.name} does not implement deleteObserved`);
  }

  allLinks() {
    throw new Error(`${this.constructor.name} does not implement allLinks`);
  }

  /** The links matching a pattern, `null` matching any value, by address. */
  queryLinks(index = null, source = null, target = null) {
    return this.allLinks()
      .filter(
        (value) =>
          (index == null || value.index === index) &&
          (source == null || value.source === source) &&
          (target == null || value.target === target),
      )
      .sort((left, right) => left.index - right.index);
  }

  search(_source, _target) {
    throw new Error(`${this.constructor.name} does not implement search`);
  }

  getOrCreate(_source, _target) {
    throw new Error(`${this.constructor.name} does not implement getOrCreate`);
  }

  tryGetOrCreate(source, target) {
    const found = this.search(source, target);
    return found != null ? found : this.tryCreate(source, target);
  }

  getName(_id) {
    throw new Error(`${this.constructor.name} does not implement getName`);
  }

  setName(_id, _name) {
    throw new Error(`${this.constructor.name} does not implement setName`);
  }

  getByName(_name) {
    throw new Error(`${this.constructor.name} does not implement getByName`);
  }

  removeName(_id) {
    throw new Error(`${this.constructor.name} does not implement removeName`);
  }

  save() {}

  getOrCreateNamed(name) {
    const found = this.getByName(name);
    if (found != null) return found;
    const id = this.tryCreate(0, 0);
    this.setName(id, name);
    this.update(id, id, id);
    return id;
  }

  formatReference(id) {
    const name = this.getName(id);
    if (name == null) return String(id);
    // `name.parse::<T>().is_ok() && name.parse::<u32>().is_err()` never holds
    // for T = u32, so a name always goes through `escape_lino_reference`.
    return escapeLinoReference(name);
  }

  formatLino(value) {
    return `(${this.formatReference(value.index)}: ${this.formatReference(
      value.source,
    )} ${this.formatReference(value.target)})`;
  }

  linoLines() {
    return [...this.allLinks()]
      .sort((left, right) => left.index - right.index)
      .map((value) => this.formatLino(value));
  }

  writeLinoOutput(path) {
    const lines = this.linoLines();
    try {
      writeFileSync(path, lines.map((line) => `${line}\n`).join(''));
    } catch (error) {
      throw new Error(`Failed to create LiNo output: ${path}`, { cause: error });
    }
  }

  printAllLino(out = process.stdout) {
    for (const line of this.linoLines()) out.write(`${line}\n`);
  }

  formatChangeLino(before, after) {
    const beforeText = before == null ? '' : this.formatLino(before);
    const afterText = after == null ? '' : this.formatLino(after);
    return `(${beforeText}) (${afterText})`;
  }

  printChangeLino(before, after, out = process.stdout) {
    out.write(`${this.formatChangeLino(before, after)}\n`);
  }

  formatStructure(id) {
    return formatStructure(
      this,
      id,
      new Set(),
      (store, address) => store.getLink(address),
      (store, address) => store.exists(address),
      (store, address) => store.formatReference(address),
    );
  }
}
