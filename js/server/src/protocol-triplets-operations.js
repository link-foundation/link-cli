// The triplets of a server in triplet mode as LiNo documents: ports
// rust/src/protocol/triplets_operations.rs (`TripletsOperation`, the triplet
// replies, notifications and parsers, `pack_triplets` and
// `canonical_triplets`). A triplet is `{ index, typeId, from, to }`
// (triplets.js) and a change `{ before, after }`, a missing side being null.
//
// | Request                          | Reply                                   |
// |----------------------------------|-----------------------------------------|
// | `(count: (index type from to))`  | `(count: N)`                            |
// | `(each: (index type from to))`   | one `(index: type from to)` per match   |
// | `(create: (type from to))`       | `() ((index: type from to))`            |
// | `(create: (0 0 0))`              | `() ((index: index 0 0))`, a point      |
// | `(update: (index type from to))` | `((index: t f t)) ((index: type from to))` |
// | `(delete: index)`                | `((index: type from to)) ()` per removed triplet |
// | `(subscribe: (index type from to))` | `(subscription: N)`; later pushes of `(notify: N)` |
import { LinkError } from './errors.js';
import { ProtocolError, alternateText } from './protocol-errors.js';
import {
  LinksOperation,
  group,
  malformedArgument,
  malformedReply,
  named,
  notificationNumber,
  number,
  numbersLino,
  parseNumber,
  parseNumbers,
  parseRestrictionOf,
  parseTextNumber,
  parts,
  restrictionLino,
} from './protocol-links-operations.js';
import { errorDocument, formatDocument, formatLink, isRef } from './protocol-lino.js';
import { referenceName } from './protocol-server-query.js';
import { TripletQuery, processTripletsQuery, triplet, tripletChange } from './triplets.js';

/** Whether two triplets (or nulls) are equal, Rust's `Option<Triplet>` `==`. */
export function sameTriplet(a, b) {
  if (a === null || b === null) return a === b;
  return a.index === b.index && a.typeId === b.typeId && a.from === b.from && a.to === b.to;
}

const changed = (change) => !sameTriplet(change.before, change.after);

/** One call of the `Triplets` interface: `kind` is the wire name of the operation. */
export class TripletsOperation {
  constructor(kind, fields = {}) {
    this.kind = kind;
    Object.assign(this, fields);
    Object.freeze(this);
  }

  static count(restriction) {
    return new TripletsOperation('count', { restriction });
  }

  static each(restriction) {
    return new TripletsOperation('each', { restriction });
  }

  static create(typeId, from, to) {
    return new TripletsOperation('create', { typeId, from, to });
  }

  static update(index, typeId, from, to) {
    return new TripletsOperation('update', { index, typeId, from, to });
  }

  static delete(index) {
    return new TripletsOperation('delete', { index });
  }

  static subscribe(restriction) {
    return new TripletsOperation('subscribe', { restriction });
  }

  /** The request document for this operation. */
  toDocument() {
    let argument;
    switch (this.kind) {
      case 'create':
        argument = numbersLino([this.typeId, this.from, this.to]);
        break;
      case 'update':
        argument = numbersLino([this.index, this.typeId, this.from, this.to]);
        break;
      case 'delete':
        argument = number(this.index);
        break;
      default:
        argument = restrictionLino(this.restriction);
    }
    return [named(this.kind, [argument])];
  }

  /** Recognises a triplet operation; null for any other document. */
  static fromDocument(document) {
    if (document.length !== 1) return null;
    const [request] = document;
    if (isRef(request) || request.id === null || request.values.length !== 1) return null;
    const [argument] = request.values;
    switch (request.id) {
      case 'count':
      case 'each':
      case 'subscribe':
        return new TripletsOperation(request.id, { restriction: parseRestrictionOf(argument, 4) });
      case 'create':
        return TripletsOperation.create(...parseNumbers(argument, 3));
      case 'update':
        return TripletsOperation.update(...parseNumbers(argument, 4));
      case 'delete':
        return TripletsOperation.delete(parseNumber(argument));
      default:
        return null;
    }
  }

  /** Runs the operation against `triplets` and returns the reply document. */
  execute(triplets) {
    switch (this.kind) {
      case 'count':
        return [named('count', [number(matching(triplets, this.restriction).length)])];
      case 'each':
        return matching(triplets, this.restriction).map(tripletLino);
      case 'create': {
        let created;
        if (this.typeId === 0 && this.from === 0 && this.to === 0) {
          const index = triplets.createTripletPoint();
          created = triplet(index, index, 0, 0);
        } else {
          const index = triplets.createTriplet(this.typeId, this.from, this.to);
          created = triplet(index, this.typeId, this.from, this.to);
        }
        triplets.save();
        return tripletChangesDocument([tripletChange(null, created)]);
      }
      case 'update': {
        const before = triplets.updateTriplet(this.index, this.typeId, this.from, this.to);
        triplets.save();
        const after = triplet(this.index, this.typeId, this.from, this.to);
        return tripletChangesDocument([tripletChange(before, after)]);
      }
      case 'delete': {
        const deleted = triplets.deleteTriplet(this.index);
        triplets.save();
        return tripletChangesDocument(deleted.map((value) => tripletChange(value, null)));
      }
      default:
        throw new LinkError('subscriptions belong to a connection to a links server');
    }
  }
}

/** `triplet_query`: the parts a restriction leaves out match anything. */
export function tripletQuery(restriction) {
  const part = (at) => restriction[at] ?? null;
  return new TripletQuery(part(0), part(1), part(2), part(3));
}

/** `restriction_of_query`: the restriction of `query`, in the order of the wire. */
export const restrictionOfQuery = (query) => [query.index, query.typeId, query.from, query.to];

/** `matches_triplet`: whether `value` matches `restriction`, part by part. */
export const matchesTriplet = (value, restriction) =>
  restriction.length <= 4 && tripletQuery(restriction).matches(value);

const matching = (triplets, restriction) => triplets.eachTriplet(tripletQuery(restriction));

/** `execute_triplets_request`: one request of a server in triplet mode. */
export function executeTripletsRequest(triplets, document) {
  return executeObservedTripletsRequest(triplets, document, []);
}

/** Like `executeTripletsRequest`, adding every triplet the request changed to `changes`. */
export function executeObservedTripletsRequest(triplets, document, changes) {
  try {
    return tryExecuteTripletsRequest(triplets, document, changes);
  } catch (error) {
    return errorDocument(alternateText(error));
  }
}

function tryExecuteTripletsRequest(triplets, document, changes) {
  const operation = TripletsOperation.fromDocument(document);
  if (operation !== null) {
    const reply = operation.execute(triplets);
    if (['create', 'update', 'delete'].includes(operation.kind)) {
      changes.push(...parseTripletChanges(reply).filter(changed));
    }
    return reply;
  }
  const links = LinksOperation.fromDocument(document);
  if (links !== null) return links.execute(triplets);
  if (document.length === 0) {
    return triplets
      .eachTriplet(TripletQuery.any())
      .map((value) => namedTripletLino(triplets, value));
  }
  const made = processTripletsQuery(triplets, formatDocument(document));
  const writes = made.filter(changed);
  changes.push(...writes);
  if (writes.length > 0) triplets.save();
  return made.map((change) =>
    group([namedSide(triplets, change.before), namedSide(triplets, change.after)]),
  );
}

/** `(index: type from to)`, naming every reference that has a name. */
function namedTripletLino(triplets, value) {
  const reference = (id) => referenceName(triplets, id);
  const index = reference(value.index);
  return named(index, [reference(value.typeId), reference(value.from), reference(value.to)]);
}

const namedSide = (triplets, value) =>
  group(value === null ? [] : [namedTripletLino(triplets, value)]);

/** `triplet_lino`: `(index: type from to)` with plain numbers. */
export const tripletLino = (value) =>
  named(number(value.index), [number(value.typeId), number(value.from), number(value.to)]);

const sideLino = (value) => group(value === null ? [] : [tripletLino(value)]);

/** A reply listing `changes`, one `(before) (after)` line each. */
export const tripletChangesDocument = (changes) =>
  changes.map((change) => group([sideLino(change.before), sideLino(change.after)]));

/** A push: `(notify: N)` followed by one `(before) (after)` line per change. */
export const tripletNotificationDocument = (subscription, changes) => [
  named('notify', [number(subscription)]),
  ...tripletChangesDocument(changes),
];

/** The subscription and the changes of a triplet notification document. */
export function parseTripletNotification(document) {
  const subscription = notificationNumber(document);
  if (subscription === null) throw malformedReply('notify', document);
  return { subscription, changes: parseTripletChanges(document.slice(1)) };
}

/** The triplets of an `each` reply. */
export const parseTriplets = (document) => document.map(parseTriplet);

/** The `{ before, after }` changes of a `create`, `update` or `delete` reply. */
export function parseTripletChanges(document) {
  return document.map((change) => {
    const sides = parts(change);
    if (sides.length !== 2) throw malformedReply('change', document);
    return tripletChange(parseSide(sides[0]), parseSide(sides[1]));
  });
}

function parseSide(side) {
  if (!isRef(side)) {
    if (side.id === null && side.values.length === 0) return null;
    if (side.id === null && side.values.length === 1) return parseTriplet(side.values[0]);
    // `((index: type from to))` loses its wrapper when the side is the
    // single named link itself.
    if (side.id !== null) return parseTriplet(side);
  }
  throw malformedArgument('a change side', side);
}

/** A triplet, as `(index: type from to)` or as `packTriplets` writes it. */
export function parseTriplet(value) {
  const malformed = () =>
    ProtocolError.malformed(`expected (index: type from to), found ${formatLink(value)}`);
  if (isRef(value) || value.id === null) throw malformed();
  let values = value.values;
  if (values.length === 1 && !isRef(values[0]) && values[0].id === null) {
    values = values[0].values;
  }
  if (values.length !== 3) throw malformed();
  return triplet(
    parseTextNumber(value.id),
    parseNumber(values[0]),
    parseNumber(values[1]),
    parseNumber(values[2]),
  );
}

/**
 * `pack_triplets`: in a binary format whose arity holds three references but
 * not five, `(index: type from to)` becomes `(index: (type from to))`, whose
 * second part travels as one link of three references.
 */
export function packTriplets(document, format) {
  const arity = format.options?.arity;
  if (arity && arity.contains(3) && !arity.contains(5)) return document.map(packLink);
  return document;
}

function packLink(value) {
  if (isRef(value)) return value;
  if (value.id === null) return group(value.values.map(packLink));
  if (value.values.length === 3) return named(value.id, [group(value.values)]);
  return value;
}

/** `canonical_triplets`: the inverse of `packTriplets`. */
export const canonicalTriplets = (document) => document.map(unpackLink);

function unpackLink(value) {
  if (isRef(value)) return value;
  if (value.id === null) return group(value.values.map(unpackLink));
  const [only] = value.values;
  if (value.values.length === 1 && !isRef(only) && only.id === null && only.values.length === 3) {
    return named(value.id, only.values);
  }
  return value;
}
