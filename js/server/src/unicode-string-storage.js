// The string and name layer of the names sidecar store
// (rust/src/unicode_string_storage.rs). Opening it pins the types 1..6
// (Type, UnicodeSymbol, UnicodeSequence, String, EmptyString, Name) and
// names them. A string is `(String sequence)`, a name `(Name string)`, and a
// named link `(link name)`; an external reference stands for a link of the
// data store, so the names of the data live on `(external(id) name)` pairs.
//
// The Rust conversion caches only skip `get_or_create` calls that find the
// links they would create, so they are left out. The accessors and type
// helpers no caller uses (`get_types`, `is_type`, `get_or_create_type`,
// `unicode_sequence_code_units`, `get_by_name`) are not ported.
import { externalReference, externalReferenceValue } from './hybrid-reference.js';
import { PinnedTypes } from './pinned-types.js';
import { stringToUnicodeSequence, unicodeSequenceToString } from './sequences.js';

const TYPE_NAMES = ['Type', 'UnicodeSymbol', 'UnicodeSequence', 'String', 'EmptyString', 'Name'];

export class UnicodeStringStorage {
  /** Opens the string layer over the `LinkStorage` `links`, creating its types. */
  constructor(links) {
    this.links = links;
    const pinned = new PinnedTypes(links);
    [
      this.typeType,
      this.unicodeSymbolType,
      this.unicodeSequenceType,
      this.stringType,
      this.emptyStringType,
      this.nameType,
    ] = pinned.takeTypes(TYPE_NAMES.length);
    TYPE_NAMES.forEach((name, i) => this.setName(i + 1, name));
  }

  /** The string link of `content`, created when missing. */
  createString(content) {
    const sequence =
      content === ''
        ? this.emptyStringType
        : stringToUnicodeSequence(
            this.links,
            content,
            this.unicodeSymbolType,
            this.unicodeSequenceType,
          );
    return this.links.getOrCreate(this.stringType, sequence);
  }

  /** The text of a string link, or of a link up to two targets above one. */
  getString(stringValue) {
    let current = stringValue;
    for (let step = 0; step < 3; step++) {
      const found = this.links.get(current);
      if (found === null) break;
      if (found.source === this.stringType) {
        return found.target === this.emptyStringType
          ? ''
          : unicodeSequenceToString(
              this.links,
              found.target,
              this.unicodeSymbolType,
              this.unicodeSequenceType,
            );
      }
      current = found.target;
    }
    throw new Error('The passed link does not contain a string.');
  }

  setNameForExternalReference(id, name) {
    return this.setName(externalReference(id), name);
  }

  getNameByExternalReference(id) {
    return this.getName(externalReference(id));
  }

  /**
   * The data link named `name`, or `null`. The pinned types hold names too,
   * so only the external holders count. Creates the string of `name`.
   */
  getExternalReferenceByName(name) {
    for (const holder of this.nameHolders(name)) {
      const value = externalReferenceValue(holder);
      if (value !== null) return value;
    }
    return null;
  }

  /** Every name of a data link, first pair by address winning, by link. */
  externalNames() {
    const names = new Map();
    for (const pair of this.links.allLinks()) {
      const index = externalReferenceValue(pair.source);
      if (index === null || names.has(index)) continue;
      const candidate = this.links.get(pair.target);
      if (candidate !== null && candidate.source === this.nameType) {
        names.set(index, this.getString(candidate.target));
      }
    }
    return new Map([...names].sort(([left], [right]) => left - right));
  }

  removeNameByExternalReference(id) {
    this.removeName(externalReference(id));
  }

  /** Names `id`: the pair `(id (Name string))`. */
  setName(id, name) {
    const nameLink = this.links.getOrCreate(this.nameType, this.createString(name));
    return this.links.getOrCreate(id, nameLink);
  }

  /** The first name of `id` by pair address, or `null`. */
  getName(id) {
    for (const pair of this.namePairsOf(id)) {
      const candidate = this.links.get(pair.target);
      if (candidate !== null && candidate.source === this.nameType) {
        return this.getString(candidate.target);
      }
    }
    return null;
  }

  // The holders of `name` by pair address.
  nameHolders(name) {
    const nameLink = this.links.search(this.nameType, this.createString(name));
    if (nameLink === null) return [];
    return this.links.query(null, null, nameLink).map((pair) => pair.source);
  }

  namePairsOf(id) {
    return this.links.query(null, id, null);
  }

  /** Removes every name pair of `id`, and each name no other link holds. */
  removeName(id) {
    for (const pair of this.namePairsOf(id)) {
      const candidate = this.links.get(pair.target);
      if (candidate === null || candidate.source !== this.nameType) continue;
      // Rust checks that the pair still exists: only a deletion of its name
      // could have taken it, and that skips the pair at the line above.
      this.links.delete(pair.index);
      const stillUsed = this.links
        .query(null, null, candidate.index)
        .some((usage) => usage.index !== pair.index);
      if (!stillUsed && this.links.exists(candidate.index)) this.links.delete(candidate.index);
    }
  }
}
