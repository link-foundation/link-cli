// Permissions on triplets (issues #149 and #152): the same rules as on
// doublets, judged on the triplet as a whole. Port of
// rust/src/triplets/permissions.rs.
//
// The doublets of a triplet are not the subject's business. A rule on the
// type `Note` says nothing about the wrappers and pairs that happen to store
// the parts of a note, so a `PermissionsDecorator` under
// `TripletsOverDoublets` would hide them and refuse to write them. It goes
// above instead, over the `Triplets` store, and asks the policy only about
// the outer doublet `(index: type …)`:
//
// - a triplet the subject may not read does not exist for the subject, and
//   neither does anything that is not a triplet;
// - creating needs the right to create and to write the type;
// - every part needs the right to refer to it (issue #161): without it an
//   unreadable part answers as a missing one, after the write check, and a
//   part the triplet already holds may stay;
// - choosing the address of a new point is a reference too;
// - updating needs the right to write the triplet as it is and as it will be;
// - deleting needs the right to write every triplet the delete takes along,
//   or nothing is deleted;
// - a point the subject created, still only its own type, is the subject's:
//   that is what a substitution query reserves before it sets the type.
//
// Rust implements `Triplets` for `PermissionsDecorator<L: Triplets, _>`; here
// that is `TripletsPermissionsDecorator`, the decorator with the triplet
// methods, whose inner store is a `Triplets` store.
import { NotFoundError } from './errors.js';
import { link } from './link.js';
import { Forbidden, PermissionsDecorator } from './permissions-named-links.js';
import { cascadeOf, Triplets } from './triplets.js';

/** Whether `part` is `value` or one of its parts. */
const holds = (value, part) =>
  part === value.index || part === value.typeId || part === value.from || part === value.to;

/** A `PermissionsDecorator` over a `Triplets` store that is a `Triplets` store itself; see the module comment. */
export class TripletsPermissionsDecorator extends Triplets(PermissionsDecorator) {
  /** The outer doublet of a triplet the store below holds. */
  #outerOf(index) {
    return this.inner().getLink(index);
  }

  #tripletReadable(value) {
    const outer = this.#outerOf(value.index);
    return outer != null && this.canReadTriplet(outer);
  }

  #tripletWritable(value) {
    const outer = this.#outerOf(value.index);
    return outer != null && this.canWriteTriplet(outer);
  }

  /**
   * `from` or `to` must be `0`, the triplet itself, a part `before` holds or
   * a triplet the subject may refer to; a hidden one answers as a missing one.
   */
  #checkPart(part, before, name) {
    if (part === 0 || (before != null && holds(before, part))) return;
    if (this.referenceIsTriplet(part)) return;
    throw new Error(`The ${name} part ${part} is not a triplet.`);
  }

  tripletMarkers() {
    return this.inner().tripletMarkers();
  }

  roleOf(id) {
    const outer = this.#outerOf(id);
    if (outer == null || !this.canReadTriplet(outer)) return null;
    return this.inner().roleOf(id);
  }

  referenceIsTriplet(index) {
    return this.isTriplet(index) || (this.canReference(index) && this.inner().isTriplet(index));
  }

  createTriplet(typeId, from, to) {
    if (!this.policyCanCreate() || (typeId !== 0 && !this.policyCanWrite(link(0, typeId, 0)))) {
      throw new Forbidden('create');
    }
    // A subject that may write a type need not read it: the rule that says
    // so names it. That it exists is left to the layer below.
    if (typeId !== 0 && !this.canReference(typeId)) throw new NotFoundError(typeId);
    this.#checkPart(from, null, 'from');
    this.#checkPart(to, null, 'to');
    const index = this.inner().createTriplet(typeId, from, to);
    this.remember(index);
    return index;
  }

  createTripletPoint() {
    if (!this.policyCanCreate()) throw new Forbidden('create');
    const index = this.inner().createTripletPoint();
    this.remember(index);
    return index;
  }

  createTripletPointAt(address) {
    if (!this.policyCanCreate()) throw new Forbidden('create');
    // Choosing an address is a reference: without the right a hidden link
    // and a free address answer alike.
    if (!this.canReference(address)) throw new NotFoundError(address);
    // The subject may refer to this hidden link: the point must not land on it.
    if (this.inner().exists(address)) throw new Error(`Link ${address} exists`);
    this.inner().createTripletPointAt(address);
    this.remember(address);
  }

  updateTriplet(index, typeId, from, to) {
    const before = this.getTriplet(index);
    if (before == null) throw new Error(`Link ${index} is not a triplet.`);
    // Only an empty triplet has itself as its target.
    const target = from === 0 && to === 0 ? index : 0;
    if (!this.#tripletWritable(before) || !this.canWriteTriplet(link(index, typeId, target))) {
      throw new Forbidden(`update ${index}`);
    }
    if (typeId !== 0 && !holds(before, typeId) && !this.canReference(typeId)) {
      throw new NotFoundError(typeId);
    }
    this.#checkPart(from, before, 'from');
    this.#checkPart(to, before, 'to');
    return this.inner().updateTriplet(index, typeId, from, to);
  }

  deleteTriplet(index) {
    const first = this.getTriplet(index);
    if (first == null) throw new Error(`Link ${index} is not a triplet.`);
    for (const value of cascadeOf(this.inner(), first)) {
      if (!this.#tripletWritable(value)) throw new Forbidden(`delete ${index}`);
    }
    const deleted = this.inner().deleteTriplet(index);
    for (const value of deleted) this.forget(value.index);
    return deleted;
  }

  eachTriplet(query) {
    return this.inner()
      .eachTriplet(query)
      .filter((value) => this.#tripletReadable(value));
  }
}
