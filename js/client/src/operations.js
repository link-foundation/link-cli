// The LinksOperation documents of the links protocol, as the Rust
// `protocol::links_operations` and the C# `LinksOperation` write them, with
// every address a BigInt so that no width is lost on the way.
//
//   (count: (i s t))      -> (count: N)
//   (each: (i s t))       -> (i: s t) per link, by address
//   (create: (s t))       -> () ((i: s t))
//   (update: (i s t))     -> ((i: s t)) ((i: s2 t2)) per net change
//   (delete: i)           -> ((i: s t)) () per deleted link, usages included
//   (get-name: i)         -> (name: 'text') or nothing
//   (set-name: (i 'text'))-> (link: N)
//   (get-by-name: 'text') -> (link: N) or nothing
//   (remove-name: i)      -> nothing
//   (subscribe: (i s t))  -> (subscription: N), then (notify: N) pushes
//   (unsubscribe: N)      -> nothing
//
// A server pushes the changes of other connections that fit a subscription
// as `(notify: N)` followed by `(before) (after)` lines; a push is told from
// a reply by its first link, `(notify: N)` with one number.
//
// A restriction has up to three parts, `*` (or null) matching anything:
// `[]` every link, `[i]`, `[i, v]` where `v` is the source or the target, and
// `[i, s, t]`.
//
// A server in triplet mode (`clink --serve --triplets`) reads the same names
// with triplets instead, `T` being the type:
//
//   (count: (i T f t))     -> (count: N)
//   (each: (i T f t))      -> (i: T f t) per triplet, by address
//   (create: (T f t))      -> () ((i: T f t))
//   (update: (i T f t))    -> ((i: T f t)) ((i: T2 f2 t2)) per net change
//   (delete: i)            -> ((i: T f t)) () per deleted triplet, usages included
//   (subscribe: (i T f t)) -> (subscription: N), then (notify: N) pushes
//
// A triplet restriction has up to four parts, matched part by part, those left
// out matching anything. A reply may also carry a triplet as `(i: (T f t))`,
// the form a binary reply under `--arity 2..3` has; both read the same.

import { AddressRangeError, MalformedMessageError } from './errors.js';
import { formatLink, group, named, parseDocument } from './lino.js';

export const ANY = '*';
const DIGITS = /^[0-9]+$/;

/**
 * Converts an address to a BigInt. Numbers must be safe integers: a number
 * above 2^53 has already lost its value, so it is refused instead of rounded.
 */
export function toAddress(value, what = 'address') {
  if (typeof value === 'bigint') {
    if (value < 0n) throw new AddressRangeError(`${what} must not be negative, got ${value}`);
    return value;
  }
  if (typeof value === 'number') {
    if (!Number.isSafeInteger(value) || value < 0) {
      throw new AddressRangeError(`${what} must be a non-negative safe integer or a BigInt, got ${value}`);
    }
    return BigInt(value);
  }
  if (typeof value === 'string' && DIGITS.test(value)) return BigInt(value);
  throw new AddressRangeError(`${what} must be a BigInt, a safe integer or a decimal string, got ${String(value)}`);
}

/** Checks that `address` fits the backend; `maxAddress` null means unbounded. */
export function checkAddress(value, maxAddress, what = 'address') {
  const address = toAddress(value, what);
  if (maxAddress !== null && maxAddress !== undefined && address > maxAddress) {
    throw new AddressRangeError(`${what} ${address} exceeds the backend's largest address ${maxAddress}`);
  }
  return address;
}

function number(address) {
  return address.toString();
}

function restrictionLino(restriction, maxAddress, maxParts = 'three') {
  const limit = maxParts === 'four' ? 4 : 3;
  if (!Array.isArray(restriction)) throw new TypeError(`a restriction is an array of up to ${maxParts} parts`);
  if (restriction.length > limit) throw new TypeError(`a restriction has at most ${maxParts} parts, got ${restriction.length}`);
  return group(
    restriction.map((part, position) =>
      part === null || part === undefined || part === ANY ? ANY : number(checkAddress(part, maxAddress, `restriction part ${position}`)),
    ),
  );
}

function operation(name, argument) {
  return [named(name, [argument])];
}

/** Request documents, validated against `maxAddress` before anything is sent. */
export const requests = {
  count: (restriction = [], maxAddress) => operation('count', restrictionLino(restriction, maxAddress)),
  each: (restriction = [], maxAddress) => operation('each', restrictionLino(restriction, maxAddress)),
  create: (source, target, maxAddress) =>
    operation('create', group([number(checkAddress(source, maxAddress, 'source')), number(checkAddress(target, maxAddress, 'target'))])),
  update: (index, source, target, maxAddress) =>
    operation(
      'update',
      group([
        number(checkAddress(index, maxAddress, 'index')),
        number(checkAddress(source, maxAddress, 'source')),
        number(checkAddress(target, maxAddress, 'target')),
      ]),
    ),
  delete: (index, maxAddress) => operation('delete', number(checkAddress(index, maxAddress, 'index'))),
  getName: (index, maxAddress) => operation('get-name', number(checkAddress(index, maxAddress, 'index'))),
  setName: (index, name, maxAddress) => {
    if (typeof name !== 'string') throw new TypeError('a name is a string');
    return operation('set-name', group([number(checkAddress(index, maxAddress, 'index')), name]));
  },
  getByName: (name) => {
    if (typeof name !== 'string') throw new TypeError('a name is a string');
    return operation('get-by-name', name);
  },
  removeName: (index, maxAddress) => operation('remove-name', number(checkAddress(index, maxAddress, 'index'))),
  subscribe: (restriction = [], maxAddress) => operation('subscribe', restrictionLino(restriction, maxAddress)),
  unsubscribe: (subscription) => operation('unsubscribe', number(toSubscription(subscription))),
  countTriplets: (restriction = [], maxAddress) => operation('count', restrictionLino(restriction, maxAddress, 'four')),
  eachTriplets: (restriction = [], maxAddress) => operation('each', restrictionLino(restriction, maxAddress, 'four')),
  createTriplet: (type, from, to, maxAddress) =>
    operation(
      'create',
      group([
        number(checkAddress(type, maxAddress, 'type')),
        number(checkAddress(from, maxAddress, 'from')),
        number(checkAddress(to, maxAddress, 'to')),
      ]),
    ),
  updateTriplet: (index, type, from, to, maxAddress) =>
    operation(
      'update',
      group([
        number(checkAddress(index, maxAddress, 'index')),
        number(checkAddress(type, maxAddress, 'type')),
        number(checkAddress(from, maxAddress, 'from')),
        number(checkAddress(to, maxAddress, 'to')),
      ]),
    ),
  subscribeTriplets: (restriction = [], maxAddress) => operation('subscribe', restrictionLino(restriction, maxAddress, 'four')),
};

/** The largest subscription number: the servers count them in a `u32`. */
const MAX_SUBSCRIPTION = 2n ** 32n - 1n;

function toSubscription(value) {
  const subscription = toAddress(value, 'subscription');
  if (subscription > MAX_SUBSCRIPTION) throw new AddressRangeError(`subscription ${subscription} exceeds ${MAX_SUBSCRIPTION}`);
  return subscription;
}

function malformed(what, document) {
  return new MalformedMessageError(`expected ${what}, found ${document.map(formatLink).join(' ') || 'an empty reply'}`);
}

function parseNumber(value, what) {
  if (typeof value === 'string' && DIGITS.test(value)) return BigInt(value);
  throw new MalformedMessageError(`expected a number for ${what}, found ${typeof value === 'string' ? value : formatLink(value)}`);
}

/** The message of an `(error: 'message')` reply, or null. */
export function errorMessage(document) {
  if (document.length !== 1) return null;
  const [link] = document;
  if (typeof link === 'string' || link.id !== 'error' || link.values.length !== 1) return null;
  return typeof link.values[0] === 'string' ? link.values[0] : null;
}

/** `(index: source target)` as `{ index, source, target }` BigInts. */
export function parseLink(link) {
  if (typeof link === 'string' || link.id === null || link.values.length !== 2) {
    throw new MalformedMessageError(`expected (index: source target), found ${formatLink(link)}`);
  }
  return {
    index: parseNumber(link.id, 'an index'),
    source: parseNumber(link.values[0], 'a source'),
    target: parseNumber(link.values[1], 'a target'),
  };
}

/** `(index: source target)` with every reference kept as the text the backend wrote (a number or a name). */
export function parseNamedLink(link) {
  if (typeof link === 'string' || link.id === null || link.values.length !== 2 || link.values.some((value) => typeof value !== 'string')) {
    throw new MalformedMessageError(`expected (index: source target), found ${formatLink(link)}`);
  }
  return { index: link.id, source: link.values[0], target: link.values[1] };
}

/** The three parts of `(index: type from to)` or `(index: (type from to))`, null when it is neither. */
function tripletParts(link) {
  if (typeof link === 'string' || link.id === null) return null;
  const [first] = link.values;
  const parts = link.values.length === 1 && typeof first !== 'string' && first.id === null ? first.values : link.values;
  return parts.length === 3 ? parts : null;
}

/** `(index: type from to)` or `(index: (type from to))` as `{ index, type, from, to }` BigInts. */
export function parseTriplet(link) {
  const parts = tripletParts(link);
  if (parts === null) throw new MalformedMessageError(`expected (index: type from to), found ${formatLink(link)}`);
  return {
    index: parseNumber(link.id, 'an index'),
    type: parseNumber(parts[0], 'a type'),
    from: parseNumber(parts[1], 'a from'),
    to: parseNumber(parts[2], 'a to'),
  };
}

/** A triplet with every reference kept as the text the backend wrote (a number or a name). */
export function parseNamedTriplet(link) {
  const parts = tripletParts(link);
  if (parts === null || parts.some((part) => typeof part !== 'string')) {
    throw new MalformedMessageError(`expected (index: type from to), found ${formatLink(link)}`);
  }
  return { index: link.id, type: parts[0], from: parts[1], to: parts[2] };
}

function parseSide(side, parse) {
  if (typeof side !== 'string' && side.id === null) {
    if (side.values.length === 0) return null;
    if (side.values.length === 1) return parse(side.values[0]);
  }
  if (typeof side !== 'string' && side.id !== null) return parse(side);
  throw new MalformedMessageError(`expected a change side, found ${formatLink(side)}`);
}

function parseChangesWith(document, parse) {
  return document.map((change) => {
    if (typeof change === 'string' || change.id !== null || change.values.length !== 2) throw malformed('(before) (after)', document);
    return { before: parseSide(change.values[0], parse), after: parseSide(change.values[1], parse) };
  });
}

export const replies = {
  count(document) {
    const [link] = document;
    if (document.length !== 1 || typeof link === 'string' || link.id !== 'count' || link.values.length !== 1) {
      throw malformed('(count: N)', document);
    }
    return parseNumber(link.values[0], 'a count');
  },
  links: (document) => document.map(parseLink),
  namedLinks: (document) => document.map(parseNamedLink),
  changes: (document) => parseChangesWith(document, parseLink),
  namedChanges: (document) => parseChangesWith(document, parseNamedLink),
  triplets: (document) => document.map(parseTriplet),
  namedTriplets: (document) => document.map(parseNamedTriplet),
  tripletChanges: (document) => parseChangesWith(document, parseTriplet),
  namedTripletChanges: (document) => parseChangesWith(document, parseNamedTriplet),
  name(document) {
    if (document.length === 0) return null;
    const [link] = document;
    if (document.length !== 1 || typeof link === 'string' || link.id !== 'name' || link.values.length !== 1 || typeof link.values[0] !== 'string') {
      throw malformed('(name: text)', document);
    }
    return link.values[0];
  },
  link(document) {
    if (document.length === 0) return null;
    const [link] = document;
    if (document.length !== 1 || typeof link === 'string' || link.id !== 'link' || link.values.length !== 1) {
      throw malformed('(link: N)', document);
    }
    return parseNumber(link.values[0], 'a link');
  },
  /** `(subscription: N)` as a number. */
  subscription(document) {
    const [link] = document;
    if (document.length !== 1 || typeof link === 'string' || link.id !== 'subscription' || link.values.length !== 1) {
      throw malformed('(subscription: N)', document);
    }
    const subscription = parseNumber(link.values[0], 'a subscription');
    if (subscription > MAX_SUBSCRIPTION) throw new MalformedMessageError(`subscription ${subscription} exceeds ${MAX_SUBSCRIPTION}`);
    return Number(subscription);
  },
  empty(document) {
    if (document.length !== 0) throw malformed('an empty reply', document);
  },
};

function notificationNumber(document) {
  const [link] = document;
  if (document.length === 0 || typeof link === 'string' || link.id !== 'notify' || link.values.length !== 1) return null;
  const [value] = link.values;
  return typeof value === 'string' && DIGITS.test(value) ? BigInt(value) : null;
}

/** Whether a document is a push: its first link is `(notify: N)`, N a number. */
export function isNotification(document) {
  return notificationNumber(document) !== null;
}

/** Whether the text of a message is a push, without parsing every reply in full. */
export function isNotificationText(text) {
  return text.startsWith('(notify:') && isNotification(parseDocument(text));
}

/** `(notify: N)` and its `(before) (after)` lines as `{ subscription, changes }`. */
export function parseNotification(document) {
  const subscription = notificationNumber(document);
  if (subscription === null || subscription > MAX_SUBSCRIPTION) throw malformed('(notify: N) (before) (after) ...', document);
  return { subscription: Number(subscription), changes: replies.changes(document.slice(1)) };
}

/** A push of a server in triplet mode: `(notify: N)` and its changes of triplets. */
export function parseTripletNotification(document) {
  const subscription = notificationNumber(document);
  if (subscription === null || subscription > MAX_SUBSCRIPTION) throw malformed('(notify: N) (before) (after) ...', document);
  return { subscription: Number(subscription), changes: replies.tripletChanges(document.slice(1)) };
}
