// The links interface as LiNo documents: ports
// rust/src/protocol/links_operations.rs (`LinksOperation`, `matches`, the
// reply parsers and builders). Addresses are u32 numbers, as the `u32` facade
// of the Rust module; a link is `{ index, source, target }` (link.js) and a
// change `[before, after]`, a missing side being the null link.
//
// | Request                          | Reply                                  |
// |----------------------------------|----------------------------------------|
// | `(count: (index source target))` | `(count: N)`                           |
// | `(each: (index source target))`  | one `(index: source target)` per match |
// | `(create: (source target))`      | `() ((index: source target))`          |
// | `(update: (index source target))`| `((index: s t)) ((index: source target))` per changed link |
// | `(delete: index)`                | `((index: source target)) ()` per removed link |
// | `(get-name: link)`               | `(name: 'text')`, or nothing           |
// | `(set-name: (link 'text'))`      | `(link: N)`                            |
// | `(get-by-name: 'text')`          | `(link: N)`, or nothing                |
// | `(remove-name: link)`            | nothing                                |
// | `(subscribe: (index source target))` | `(subscription: N)`; later pushes of `(notify: N)` |
// | `(unsubscribe: N)`               | nothing                                |
import { simplifyChanges } from './changes-simplifier.js';
import { LinkError } from './errors.js';
import { isNull, link, NULL_LINK, parseU32 } from './link.js';
import { ProtocolError } from './protocol-errors.js';
import { byteLength, formatDocument, formatLink, isRef } from './protocol-lino.js';

/** The wire spelling of "any value" in a restriction. */
export const ANY = '*';

/** `named(id, values)`: `(id: values...)`. */
export const named = (id, values) => ({ id, values });

/** `group(values)`: `(values...)`. */
export const group = (values) => ({ id: null, values });

/** `number(value)`: the reference of a number. */
export const number = (value) => String(value);

/** `malformed_argument`: `expected {expected}, found {argument}`. */
export const malformedArgument = (expected, argument) =>
  ProtocolError.malformed(`expected ${expected}, found ${formatLink(argument)}`);

/** `malformed_reply`: `expected a {expected} reply, found {document}`. */
export const malformedReply = (expected, document) =>
  ProtocolError.malformed(`expected a ${expected} reply, found ${formatDocument(document)}`);

/** `parts`: the values of an unnamed group; a lone reference is a group of one. */
export function parts(argument) {
  return !isRef(argument) && argument.id === null ? argument.values : [argument];
}

/** `parse_text_number`: Rust's `str::parse::<u32>()`. */
export function parseTextNumber(text) {
  const value = parseU32(text);
  if (value === null) throw ProtocolError.malformed(`expected a number, found '${text}'`);
  return value;
}

/** `parse_number`: a reference holding a u32. */
export function parseNumber(value) {
  if (!isRef(value)) {
    throw ProtocolError.malformed(`expected a number, found ${formatLink(value)}`);
  }
  return parseTextNumber(value);
}

/** `parse_numbers::<N>`: exactly `count` numbers. */
export function parseNumbers(argument, count) {
  const numbers = parts(argument).map(parseNumber);
  if (numbers.length !== count) throw malformedArgument(`${count} numbers`, argument);
  return numbers;
}

/** `parse_restriction_of`: up to `longest` parts, `*` (null) matching anything. */
export function parseRestrictionOf(argument, longest) {
  const values = parts(argument);
  if (values.length > longest) throw malformedArgument('restriction', argument);
  return values.map((part) => (part === ANY ? null : parseNumber(part)));
}

/** `restriction_lino`: `(parts...)`, `*` for any. */
export const restrictionLino = (restriction) =>
  group(restriction.map((part) => (part === null ? ANY : number(part))));

/** `numbers_lino`: `(numbers...)`. */
export const numbersLino = (numbers) => group(numbers.map(number));

/** `link_lino`: `(index: source target)` with plain numbers. */
export const linkLino = (value) =>
  named(number(value.index), [number(value.source), number(value.target)]);

const linkReply = (value) => (value === null ? [] : [named('link', [number(value)])]);

const changeSide = (value) => group(isNull(value) ? [] : [linkLino(value)]);

/** `changes_document`: one `(before) (after)` line per change. */
export const changesDocument = (changes) =>
  changes.map(([before, after]) => group([changeSide(before), changeSide(after)]));

/** One call of the links interface: `kind` is the wire name of the operation. */
export class LinksOperation {
  constructor(kind, fields = {}) {
    this.kind = kind;
    Object.assign(this, fields);
    Object.freeze(this);
  }

  static count(restriction) {
    return new LinksOperation('count', { restriction });
  }

  static each(restriction) {
    return new LinksOperation('each', { restriction });
  }

  static create(source, target) {
    return new LinksOperation('create', { source, target });
  }

  static update(index, source, target) {
    return new LinksOperation('update', { index, source, target });
  }

  static delete(index) {
    return new LinksOperation('delete', { index });
  }

  static getName(index) {
    return new LinksOperation('get-name', { index });
  }

  static setName(index, name) {
    return new LinksOperation('set-name', { index, name });
  }

  static getByName(name) {
    return new LinksOperation('get-by-name', { name });
  }

  static removeName(index) {
    return new LinksOperation('remove-name', { index });
  }

  static subscribe(restriction) {
    return new LinksOperation('subscribe', { restriction });
  }

  static unsubscribe(subscription) {
    return new LinksOperation('unsubscribe', { index: subscription });
  }

  /** The request document for this operation. */
  toDocument() {
    let argument;
    switch (this.kind) {
      case 'count':
      case 'each':
      case 'subscribe':
        argument = restrictionLino(this.restriction);
        break;
      case 'create':
        argument = numbersLino([this.source, this.target]);
        break;
      case 'update':
        argument = numbersLino([this.index, this.source, this.target]);
        break;
      case 'set-name':
        argument = group([number(this.index), this.name]);
        break;
      case 'get-by-name':
        argument = this.name;
        break;
      default:
        argument = number(this.index);
    }
    return [named(this.kind, [argument])];
  }

  /** Recognises an operation request; null for any other document. */
  static fromDocument(document) {
    if (document.length !== 1) return null;
    const [request] = document;
    if (isRef(request) || request.id === null || request.values.length !== 1) return null;
    const [argument] = request.values;
    switch (request.id) {
      case 'count':
      case 'each':
      case 'subscribe':
        return new LinksOperation(request.id, { restriction: parseRestrictionOf(argument, 3) });
      case 'create': {
        const [source, target] = parseNumbers(argument, 2);
        return LinksOperation.create(source, target);
      }
      case 'update': {
        const [index, source, target] = parseNumbers(argument, 3);
        return LinksOperation.update(index, source, target);
      }
      case 'delete':
      case 'get-name':
      case 'remove-name':
      case 'unsubscribe':
        return new LinksOperation(request.id, { index: parseNumber(argument) });
      case 'set-name': {
        const values = parts(argument);
        if (values.length !== 2 || !isRef(values[1])) {
          throw malformedArgument('set-name', argument);
        }
        return LinksOperation.setName(parseNumber(values[0]), values[1]);
      }
      case 'get-by-name':
        if (!isRef(argument)) throw malformedArgument('get-by-name', argument);
        return LinksOperation.getByName(argument);
      default:
        return null;
    }
  }

  /** The addresses the operation names, checked against `QueryLimits.maxAddress`. */
  addresses() {
    switch (this.kind) {
      case 'count':
      case 'each':
      case 'subscribe':
        return this.restriction.filter((part) => part !== null);
      case 'create':
        return [this.source, this.target];
      case 'update':
        return [this.index, this.source, this.target];
      case 'get-by-name':
        return [];
      default:
        return [this.index];
    }
  }

  /** Runs the operation against `storage` (a `NamedTypeLinks`) and returns the reply. */
  execute(storage) {
    const limits = storage.queryLimits();
    if (limits !== null) {
      if (this.restriction !== undefined && this.restriction.length > 3) {
        throw new LinkError('expected at most three restriction parts');
      }
      if (this.name !== undefined && byteLength(this.name) > limits.maxBytes) {
        throw new LinkError('protocol byte limit exceeded');
      }
      if (!this.addresses().every((value) => BigInt(value) <= limits.maxAddress)) {
        throw new LinkError('protocol address out of range');
      }
    }
    switch (this.kind) {
      case 'count':
        return [named('count', [number(matching(storage, this.restriction).length)])];
      case 'each': {
        const links = matching(storage, this.restriction);
        if (limits !== null && links.length > limits.maxResults) {
          throw new LinkError('protocol result limit exceeded');
        }
        return links.map(linkLino);
      }
      case 'create': {
        const created = link(storage.tryCreate(this.source, this.target), this.source, this.target);
        storage.save();
        return changesDocument([[NULL_LINK, created]]);
      }
      case 'update':
      case 'delete': {
        const changes = [];
        const observer = (before, after) => changes.push([before, after]);
        if (this.kind === 'update') {
          storage.updateObserved(this.index, this.source, this.target, observer);
        } else {
          storage.deleteObserved(this.index, observer);
        }
        storage.save();
        return changesDocument(simplifyChanges(changes));
      }
      case 'get-name': {
        const name = storage.getName(this.index);
        return name == null ? [] : [named('name', [name])];
      }
      case 'set-name': {
        const value = storage.setName(this.index, this.name);
        storage.save();
        return linkReply(value);
      }
      case 'get-by-name':
        return linkReply(storage.getByName(this.name) ?? null);
      case 'remove-name':
        storage.removeName(this.index);
        storage.save();
        return [];
      default:
        throw new LinkError('subscriptions belong to a connection to a links server');
    }
  }
}

/** Whether `value` matches `restriction`, with the shapes of the raw links interface. */
export function matches(value, restriction) {
  const is = (part, address) => part === null || part === address;
  switch (restriction.length) {
    case 0:
      return true;
    case 1:
      return is(restriction[0], value.index);
    case 2:
      return (
        is(restriction[0], value.index) &&
        (is(restriction[1], value.source) || is(restriction[1], value.target))
      );
    case 3:
      return (
        is(restriction[0], value.index) &&
        is(restriction[1], value.source) &&
        is(restriction[2], value.target)
      );
    default:
      return false;
  }
}

function matching(storage, restriction) {
  let links;
  if (restriction.length > 0 && restriction[0] !== null) {
    const found = storage.getLink(restriction[0]);
    links = found == null ? [] : [found];
  } else {
    links = storage.allLinks();
  }
  return links.filter((value) => matches(value, restriction)).sort((a, b) => a.index - b.index);
}

function parseNamedNumber(document, expected) {
  const [reply] = document;
  if (document.length !== 1 || isRef(reply) || reply.id !== expected) {
    throw malformedReply(expected, document);
  }
  if (reply.values.length !== 1) throw malformedReply(expected, document);
  return parseNumber(reply.values[0]);
}

/** The count of a `(count: N)` reply. */
export const parseCount = (document) => parseNamedNumber(document, 'count');

/** The number of a `(subscription: N)` reply. */
export const parseSubscription = (document) => parseNamedNumber(document, 'subscription');

/** The reply to a subscribe: `(subscription: N)`. */
export const subscriptionDocument = (subscription) => [
  named('subscription', [number(subscription)]),
];

/** `notification_number`: N when the first link is `(notify: N)`, else null. */
export function notificationNumber(document) {
  if (document.length === 0) return null;
  const [first] = document;
  if (isRef(first) || first.id !== 'notify' || first.values.length !== 1) return null;
  const [text] = first.values;
  if (!isRef(text) || !/^[0-9]*$/.test(text)) return null;
  return parseU32(text);
}

/** Whether `document` is a push rather than a reply: it starts with `(notify: N)`. */
export const isNotification = (document) => notificationNumber(document) !== null;

/** A push: `(notify: N)` followed by one `(before) (after)` line per change. */
export const notificationDocument = (subscription, changes) => [
  named('notify', [number(subscription)]),
  ...changesDocument(changes),
];

/** The subscription and the changes of a notification document. */
export function parseNotification(document) {
  const subscription = notificationNumber(document);
  if (subscription === null) throw malformedReply('notify', document);
  return { subscription, changes: parseChanges(document.slice(1)) };
}

function parseLink(value) {
  if (isRef(value) || value.id === null || value.values.length !== 2) {
    throw ProtocolError.malformed(`expected (index: source target), found ${formatLink(value)}`);
  }
  return link(
    parseTextNumber(value.id),
    parseNumber(value.values[0]),
    parseNumber(value.values[1]),
  );
}

function parseChangeSide(side) {
  if (!isRef(side)) {
    if (side.id === null && side.values.length === 0) return NULL_LINK;
    if (side.id === null && side.values.length === 1) return parseLink(side.values[0]);
    // `((index: source target))` loses its wrapper when the side is the
    // single named link itself.
    if (side.id !== null) return parseLink(side);
  }
  throw ProtocolError.malformed(`expected a change side, found ${formatLink(side)}`);
}

/** The links of an `each` reply. */
export const parseLinks = (document) => document.map(parseLink);

/** The `[before, after]` pairs of a `create`, `update` or `delete` reply. */
export function parseChanges(document) {
  return document.map((change) => {
    const sides = parts(change);
    if (sides.length !== 2) throw malformedReply('change', document);
    return [parseChangeSide(sides[0]), parseChangeSide(sides[1])];
  });
}

/** The name of a `get-name` reply, or null. */
export function parseName(document) {
  if (document.length === 0) return null;
  const [reply] = document;
  if (document.length !== 1 || isRef(reply) || reply.id !== 'name') {
    throw malformedReply('name', document);
  }
  if (reply.values.length !== 1 || !isRef(reply.values[0])) throw malformedReply('name', document);
  return reply.values[0];
}

/** The link of a `set-name` or `get-by-name` reply, or null. */
export function parseLinkReply(document) {
  if (document.length === 0) return null;
  return parseNamedNumber(document, 'link');
}
