// The rows of Deep over triplets. Port of rust/src/graphql/store.rs.
//
// A row of `links` is a triplet with the external id `deep:<id>`, since Deep's
// ids are not the addresses of the store. Deep's type `0` is a triplet of its
// own, `DeepNoType`, because a triplet always has a type. A row of `strings`,
// `numbers` or `objects` is a triplet `(row: DeepString owner 0)`, so deleting
// the owner takes it along, and its text is the name `<row>:<text>`. Addresses
// and values are also kept in memory, rebuilt from the links when opening.
//
// Deep ids are i64, held as BigInts; addresses of the store are numbers. A
// value kind is the name of its table. Ids grow as a release build of Rust
// does: past the largest i64 they wrap.

import { LinksExternalIds } from './external-ids.js';
import { Json, parseI64 } from './graphql-json.js';
import { Position } from './graphql-plan.js';
import { TripletsPermissionsDecorator } from './triplets-permissions.js';
import { cascadeOf, TripletQuery, TripletsOverDoublets } from './triplets.js';
import { UndoableLinksDecorator } from './undoable-links.js';

const MARKER_NAMES = { strings: 'DeepString', numbers: 'DeepNumber', objects: 'DeepObject' };

/** The tables of values; a kind is its table's name. */
export const ValueKind = Object.freeze({
  String: 'strings',
  Number: 'numbers',
  Object: 'objects',
  ALL: Object.freeze(['strings', 'numbers', 'objects']),
  /** The kind of `table`, or `null` when it is not a table of values. */
  ofTable: (table) => (ValueKind.ALL.includes(table) ? table : null),
});

const encode = (kind, value) =>
  kind === ValueKind.String && value.kind === 'string' ? value.value : value.write();

function decode(kind, text) {
  if (kind === ValueKind.String) return Json.str(text);
  try {
    return Json.parse(text);
  } catch {
    return Json.NULL;
  }
}

const wrap = (value) => BigInt.asIntN(64, value);
const max = (left, right) => (left > right ? left : right);
const ownerKey = (kind, owner) => `${kind}|${owner}`;

/** A row of `links`, as JSON. */
export function linkRowJson(row) {
  return Json.object([
    ['id', new Json('int', row.id)],
    ['type_id', new Json('int', row.typeId)],
    ['from_id', new Json('int', row.fromId)],
    ['to_id', new Json('int', row.toId)],
  ]);
}

function namedPoint(triplets, name) {
  const found = triplets.getByName(name);
  if (found != null) return found;
  const point = triplets.createTripletPoint();
  triplets.setName(point, name);
  return point;
}

export class DeepStore {
  constructor(triplets, policy, noType, markers) {
    this.triplets = triplets;
    this.policy = policy;
    /** The links each subject created through its view, by subject. */
    this.created = new Map();
    this.ids = new LinksExternalIds('deep');
    this.toTriplet = new Map();
    this.toDeep = new Map();
    this.valueRows = new Map();
    this.byOwner = new Map();
    this.markers = markers;
    this.noType = noType;
    this.nextId = 1n;
    /** `x-hasura-admin-secret`: the request is not judged by the policy. */
    this.admin = false;
    /** Bumped by every committed write, for what is derived from the rows. */
    this.generation = 0;
    /** Derived tables by name: `{ generation, rows }`. */
    this.derived = new Map();
    /** Inside a mutation what is derived is not kept: it may yet change. */
    this.inMutation = false;
  }

  /** The rows of Deep in `links`, a `NamedTypeLinks` store, seen under `policy` or `null`. */
  static open(links, policy) {
    const triplets = TripletsOverDoublets.withNamedMarkers(new UndoableLinksDecorator(links));
    const noType = namedPoint(triplets, 'DeepNoType');
    const markers = ValueKind.ALL.map((kind) => namedPoint(triplets, MARKER_NAMES[kind]));
    const store = new DeepStore(triplets, policy, noType, markers);
    store.reload();
    return store;
  }

  /** Rebuilds what is kept in memory from the links. */
  reload() {
    this.toTriplet.clear();
    this.toDeep.clear();
    this.valueRows.clear();
    this.byOwner.clear();
    this.nextId = 1n;
    for (const triplet of this.triplets.eachTriplet(TripletQuery.any())) {
      const text = this.ids.idOf(this.triplets, triplet.index);
      const id = text === null ? null : parseI64(text);
      if (id !== null) {
        this.toTriplet.set(id, triplet.index);
        this.toDeep.set(triplet.index, id);
        this.nextId = max(this.nextId, wrap(id + 1n));
      }
    }
    ValueKind.ALL.forEach((kind, slot) => {
      for (const row of this.triplets.eachTriplet(TripletQuery.byType(this.markers[slot]))) {
        const owner = this.toDeep.get(row.from);
        if (owner === undefined) continue;
        const name = this.triplets.getName(row.index) ?? '';
        const colon = name.indexOf(':');
        const text = colon < 0 ? '' : name.slice(colon + 1);
        this.byOwner.set(ownerKey(kind, owner), row.index);
        this.valueRows.set(row.index, { kind, owner, value: decode(kind, text) });
      }
    });
  }

  /** Runs `read` over the triplets as `subject` sees them. */
  view(subject, read) {
    if (this.policy === null || this.admin) return read(this.triplets);
    const created = this.created.get(subject) ?? new Set();
    this.created.delete(subject);
    const view = new TripletsPermissionsDecorator(this.triplets, this.policy, subject).withCreated(
      created,
    );
    try {
      return read(view);
    } finally {
      this.created.set(subject, view.created());
    }
  }

  hasPolicy() {
    return this.policy !== null;
  }

  /** The address of the row `id` of `links`, or `null`. */
  tripletOf(id) {
    return this.toTriplet.get(id) ?? null;
  }

  /** The address a Deep id stands for in `position` of a query. */
  address(position, id) {
    if (id === 0n && position === Position.Type) return this.noType;
    return this.tripletOf(id);
  }

  deepOf(triplet) {
    return this.toDeep.get(triplet) ?? null;
  }

  /** The row `{ id, typeId, fromId, toId }` of a triplet, if it is a row of `links`. */
  rowOf(triplet) {
    const id = this.deepOf(triplet.index);
    if (id === null) return null;
    const part = (address) => (address === 0 ? 0n : this.deepOf(address));
    const typeId = triplet.typeId === this.noType ? 0n : this.deepOf(triplet.typeId);
    const fromId = part(triplet.from);
    const toId = part(triplet.to);
    if (typeId === null || fromId === null || toId === null) return null;
    return { id, typeId, fromId, toId };
  }

  /** The rows of values `[row, { kind, owner, value }]`, by row. */
  values() {
    return [...this.valueRows.entries()].sort(([left], [right]) => left - right);
  }

  /** `[row, value]` of the value of `kind` that `owner` has, or `null`. */
  valueOfOwner(kind, owner) {
    const row = this.byOwner.get(ownerKey(kind, owner));
    return row === undefined ? null : [row, this.valueRows.get(row)];
  }

  value(row) {
    return this.valueRows.get(row) ?? null;
  }

  /** `count` fresh ids. */
  reserve(count) {
    const first = this.nextId;
    this.nextId = wrap(this.nextId + max(count, 0n));
    const ids = [];
    for (let id = first; id < this.nextId; id += 1n) ids.push(id);
    return ids;
  }

  begin() {
    this.inMutation = true;
    this.triplets.linksMut().begin();
  }

  commit() {
    this.inMutation = false;
    this.triplets.linksMut().commit();
    this.generation += 1;
    this.triplets.save();
  }

  rollback() {
    this.inMutation = false;
    this.triplets.linksMut().rollback();
    this.reload();
  }

  typeAddress(typeId) {
    const address = this.address(Position.Type, typeId);
    if (address === null) throw new Error(`Foreign key violation: no link ${typeId} for type_id.`);
    return address;
  }

  partAddress(id, column) {
    if (id === 0n) return 0;
    const address = this.tripletOf(id);
    if (address === null) throw new Error(`Foreign key violation: no link ${id} for ${column}.`);
    return address;
  }

  /** Inserts a row of `links`, `id` `null` for the next one; a part may name the id being inserted. */
  createLink(subject, given, typeId, fromId, toId) {
    const id = given ?? this.nextId;
    if (this.toTriplet.has(id)) {
      throw new Error(
        `Uniqueness violation: duplicate key value violates unique constraint "links_pkey": ${id}.`,
      );
    }
    if (typeId === id || fromId === id || toId === id) {
      const point = this.view(subject, (view) => view.createTripletPoint());
      this.install(id, point);
      this.updateLink(subject, id, typeId, fromId, toId);
    } else {
      const type = this.typeAddress(typeId);
      const from = this.partAddress(fromId, 'from_id');
      const to = this.partAddress(toId, 'to_id');
      const triplet = this.view(subject, (view) => view.createTriplet(type, from, to));
      this.install(id, triplet);
    }
    return id;
  }

  install(id, triplet) {
    this.ids.bind(this.triplets, id.toString(), triplet);
    this.toTriplet.set(id, triplet);
    this.toDeep.set(triplet, id);
    this.nextId = max(this.nextId, wrap(id + 1n));
  }

  updateLink(subject, id, typeId, fromId, toId) {
    const index = this.partAddress(id, 'id');
    const type = this.typeAddress(typeId);
    const from = this.partAddress(fromId, 'from_id');
    const to = this.partAddress(toId, 'to_id');
    this.view(subject, (view) => view.updateTriplet(index, type, from, to));
  }

  /** Deletes a row of `links` and all that uses it; returns the ids deleted. */
  deleteLink(subject, id) {
    const index = this.partAddress(id, 'id');
    const first = this.triplets.getTriplet(index);
    if (first === null) throw new Error(`Link ${id} is not a triplet.`);
    const cascade = cascadeOf(this.triplets, first);
    const deleted = [];
    // The external ids and value names point at the rows: they go first.
    for (const triplet of cascade) {
      const deep = this.deepOf(triplet.index);
      if (deep !== null) {
        this.ids.unbind(this.triplets, deep.toString());
        deleted.push(deep);
      }
      if (this.valueRows.has(triplet.index)) this.triplets.removeName(triplet.index);
    }
    this.view(subject, (view) => view.deleteTriplet(index));
    for (const triplet of cascade) {
      const deep = this.deepOf(triplet.index);
      if (deep !== null) {
        this.toDeep.delete(triplet.index);
        this.toTriplet.delete(deep);
      }
      const row = this.value(triplet.index);
      if (row !== null) {
        this.valueRows.delete(triplet.index);
        this.byOwner.delete(ownerKey(row.kind, row.owner));
      }
    }
    return deleted;
  }

  /** Whether `subject` may change what hangs on the link `owner`; its address. */
  requireOwnerWritable(subject, owner) {
    const address = this.partAddress(owner, 'link_id');
    if (this.policy !== null && !this.admin) {
      const outer = this.triplets.getLink(address);
      const allowed = this.view(subject, (view) => outer != null && view.canWriteTriplet(outer));
      if (!allowed) throw new Error(`Forbidden: ${subject} may not write the value of ${owner}.`);
    }
    return address;
  }

  /** Adds the value row of `kind` that `owner` has. */
  createValue(subject, kind, owner, value) {
    const address = this.requireOwnerWritable(subject, owner);
    if (this.byOwner.has(ownerKey(kind, owner))) {
      throw new Error(`Uniqueness violation: link ${owner} already has a row in ${kind}.`);
    }
    return this.addValue(kind, owner, address, value);
  }

  addValue(kind, owner, address, value) {
    const marker = this.markers[ValueKind.ALL.indexOf(kind)];
    const row = this.triplets.createTriplet(marker, address, 0);
    this.triplets.setName(row, `${row}:${encode(kind, value)}`);
    this.byOwner.set(ownerKey(kind, owner), row);
    this.valueRows.set(row, { kind, owner, value });
    return row;
  }

  updateValue(subject, row, value) {
    const found = this.value(row);
    if (found === null) throw new Error(`No value row ${row}.`);
    this.requireOwnerWritable(subject, found.owner);
    this.triplets.setName(row, `${row}:${encode(found.kind, value)}`);
    this.valueRows.set(row, { ...found, value });
  }

  deleteValue(subject, row) {
    const found = this.value(row);
    if (found === null) throw new Error(`No value row ${row}.`);
    this.requireOwnerWritable(subject, found.owner);
    this.triplets.removeName(row);
    this.triplets.deleteTriplet(row);
    this.valueRows.delete(row);
    this.byOwner.delete(ownerKey(found.kind, found.owner));
  }

  /**
   * Loads rows `{id, type_id, from_id, to_id, string?, number?, object?}`,
   * such as a dump of `@deep-foundation/core`; rows already there are left
   * alone, so loading twice changes nothing. Ids may refer forward, so all
   * the points are made first. Returns how many rows were new.
   */
  loadPackage(rows) {
    const int = (row, key) => row.get(key)?.asI64() ?? 0n;
    const fresh = rows.asList().filter((row) => this.tripletOf(int(row, 'id')) === null);
    for (const row of fresh) this.install(int(row, 'id'), this.triplets.createTripletPoint());
    for (const row of fresh) {
      const index = this.partAddress(int(row, 'id'), 'id');
      const type = this.typeAddress(int(row, 'type_id'));
      const from = this.partAddress(int(row, 'from_id'), 'from_id');
      const to = this.partAddress(int(row, 'to_id'), 'to_id');
      this.triplets.updateTriplet(index, type, from, to);
    }
    for (const row of fresh) {
      for (const [key, kind] of [
        ['string', ValueKind.String],
        ['number', ValueKind.Number],
        ['object', ValueKind.Object],
      ]) {
        const value = row.get(key);
        if (value !== null) {
          const owner = int(row, 'id');
          this.addValue(kind, owner, this.partAddress(owner, 'id'), value);
        }
      }
    }
    this.triplets.save();
    this.generation += 1;
    return fresh.length;
  }

  /**
   * The grants of the policy as `[write, subject, object]`, in Deep ids
   * (`null` where the rule says `Anything`).
   */
  grants() {
    if (this.policy === null) return [];
    const markers = this.policy.markers();
    const side = (address) => (address === markers.anything ? null : this.toDeep.get(address));
    const grants = [];
    for (const [write, marker] of [
      [false, markers.allowRead],
      [true, markers.allowWrite],
    ]) {
      for (const [subject, object] of this.policy.granted(this.triplets, marker)) {
        const [left, right] = [side(subject), side(object)];
        if (left !== undefined && right !== undefined) grants.push([write, left, right]);
      }
    }
    return grants;
  }
}
