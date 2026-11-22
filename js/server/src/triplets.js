// Triplets `(index: type from to)` stored in doublets, with an associative
// API over any `NamedTypeLinks` (issue #151). Port of
// rust/src/triplets/mod.rs.
//
// The encoding keeps the convention of the type hierarchy of issue #148,
// *the type of a link is its source*, and adds two marker points, `Pair`
// (`P`) and `Nothing` (`N`, which stands for the empty part `0`):
//
// | triplet            | doublets                                                        |
// |--------------------|-----------------------------------------------------------------|
// | `(i: T 0 0)`, node | `(i: T i)`                                                      |
// | `(i: T f t)`       | wrapper `w = (P f)`, pair `p = (w t)`, outer `(i: T p)`         |
// | the same, again    | the outer `(T p)` is taken, so a duplicate pair `d = (p i)` and `(i: T d)` |
//
// A zero `f` or `t` is written as `N`. Wrappers and pairs are shared by every
// triplet with the same parts, so they are collected as soon as nothing uses
// them any more. A triplet's address is the address of its outer doublet,
// which an update rewrites in place: the address stays and the old pair is
// collected. A delete removes, like a doublet delete here, every triplet that
// uses the deleted one as its type, from or to.
//
// Every doublet is classified by its sources alone:
//
// - `P` and `N` are the markers;
// - `(w: P x)` is a wrapper;
// - `(p: w x)` whose source is a wrapper is a pair;
// - `(d: p i)` whose source is a pair is a duplicate pair;
// - `(i: T y)` is a triplet when `y` is `i`, a pair, or a duplicate pair
//   `(p i)` that points back at it;
// - anything else, the `Extends` wiring of a `TypesDecorator` say, is a
//   doublet of somebody else and is not a triplet.
//
// `Triplets` is the interface, a mixin whose every read and write is a
// default method over `NamedTypeLinks` that goes through `this`, so a layer
// can override any of them and the others see it. `TripletsOverDoublets` is
// the decorator that implements it over any other layer.
import { NamedTypeLinks } from './named-type-links.js';
import { compareTriplets, triplet, TripletQuery } from './triplets-restriction.js';
import { ensurePoint } from './types.js';

export { compareTriplets, triplet, tripletChange } from './triplets-restriction.js';
export { TripletQuery } from './triplets-restriction.js';
export { matchTripletsQuery, processTripletsQuery } from './triplets-query.js';
export { importTripletsLino, tripletsToLino } from './triplets-lino.js';

/** The name of the marker point of wrappers. */
export const PAIR_MARKER_NAME = 'Pair';

/** The name of the marker point that stands for the empty part `0`. */
export const NOTHING_MARKER_NAME = 'Nothing';

/** The two marker points of the encoding: `pair` (`P`) and `nothing` (`N`). */
export class TripletMarkers {
  constructor(pair, nothing) {
    this.pair = pair;
    this.nothing = nothing;
    Object.freeze(this);
  }

  /** The points named `Pair` and `Nothing`, each created as a point `(x: x x)` and named when missing. */
  static ensure(links) {
    const pair = ensurePoint(links, PAIR_MARKER_NAME);
    return new TripletMarkers(pair, ensurePoint(links, NOTHING_MARKER_NAME));
  }

  /** Whether `id` is one of the markers. */
  contains(id) {
    return id === this.pair || id === this.nothing;
  }

  encode(part) {
    return part === 0 ? this.nothing : part;
  }

  decode(part) {
    return part === this.nothing ? 0 : part;
  }
}

const role = (kind) => Object.freeze({ kind });

/**
 * What a doublet is in the encoding: one of the constant roles, or
 * `DoubletRole.Triplet(value)`, a `{ kind: 'Triplet', triplet }`.
 */
export const DoubletRole = Object.freeze({
  /** `Pair` or `Nothing`. */
  Marker: role('Marker'),
  /** `(w: P f)`, the from-part of the pairs whose source it is. */
  Wrapper: role('Wrapper'),
  /** `(p: w t)`, the from-part and to-part of a triplet. */
  Pair: role('Pair'),
  /** `(d: p i)`, the pair of a triplet whose outer `(T p)` another triplet took. */
  DuplicatePair: role('DuplicatePair'),
  /** A doublet the encoding does not know: not a triplet. */
  Foreign: role('Foreign'),
  /** The outer doublet of a triplet. */
  Triplet: (value) => Object.freeze({ kind: 'Triplet', triplet: value }),
});

/** Whether the role is a wrapper, a pair or a duplicate pair. */
export const isAuxiliary = (doubletRole) =>
  doubletRole === DoubletRole.Wrapper ||
  doubletRole === DoubletRole.Pair ||
  doubletRole === DoubletRole.DuplicatePair;

const leadsOn = (doubletRole) =>
  doubletRole === DoubletRole.Pair || doubletRole === DoubletRole.DuplicatePair;

const present = (value) => value != null;

function stored(links, id) {
  return id === 0 ? null : links.getLink(id);
}

function isWrapper(links, markers, id) {
  return id !== 0 && !markers.contains(id) && stored(links, id)?.source === markers.pair;
}

function isPair(links, markers, id) {
  if (id === 0 || markers.contains(id)) return false;
  const value = stored(links, id);
  return value != null && isWrapper(links, markers, value.source);
}

/** The triplet `(index: type from to)` whose parts are the pair `pair`, whose source is a wrapper. */
function onPair(links, markers, value, pair) {
  const wrapper = stored(links, pair.source);
  return DoubletRole.Triplet(
    triplet(value.index, value.source, markers.decode(wrapper.target), markers.decode(pair.target)),
  );
}

/** What `value` is in the encoding; see the module comment. */
function roleOf(links, markers, value) {
  if (markers.contains(value.index)) return DoubletRole.Marker;
  if (value.source === markers.pair) return DoubletRole.Wrapper;
  if (value.source === 0 || value.source === markers.nothing) return DoubletRole.Foreign;
  const source = stored(links, value.source);
  if (source == null) return DoubletRole.Foreign;
  if (source.source === markers.pair) return DoubletRole.Pair;
  if (isWrapper(links, markers, source.source)) return DoubletRole.DuplicatePair;
  // The source is a duplicate pair: nothing of ours has one.
  if (isPair(links, markers, source.source)) return DoubletRole.Foreign;
  if (value.target === value.index) {
    return DoubletRole.Triplet(triplet(value.index, value.source, 0, 0));
  }
  const target = stored(links, value.target);
  if (target == null || markers.contains(target.index)) return DoubletRole.Foreign;
  if (isWrapper(links, markers, target.source)) return onPair(links, markers, value, target);
  if (target.target === value.index && isPair(links, markers, target.source)) {
    return onPair(links, markers, value, stored(links, target.source));
  }
  return DoubletRole.Foreign;
}

const tripletsOf = (links, markers, values) =>
  values
    .map((value) => roleOf(links, markers, value))
    .filter((doubletRole) => doubletRole.kind === 'Triplet')
    .map((doubletRole) => doubletRole.triplet);

function ensureType(triplets, typeId, itself) {
  if (typeId === 0) throw new Error('The type of a triplet cannot be 0.');
  if (typeId === itself || triplets.isTriplet(typeId)) return;
  throw new Error(`The type ${typeId} is not a triplet.`);
}

function ensurePart(triplets, part, itself, name) {
  if (part === 0 || part === itself || triplets.isTriplet(part)) return;
  throw new Error(`The ${name} part ${part} is not a triplet.`);
}

/** The pair of `from` and `to`, with its wrapper, created when missing. */
function pairOf(links, markers, from, to) {
  const wrapper = links.tryGetOrCreate(markers.pair, markers.encode(from));
  return links.tryGetOrCreate(wrapper, markers.encode(to));
}

/** The pair of `from` and `to`, when there is one. */
function findPair(links, markers, from, to) {
  const wrapper = links.search(markers.pair, markers.encode(from));
  return wrapper == null ? null : links.search(wrapper, markers.encode(to));
}

/**
 * The triplets whose parts are the pair `pair`: the outers `(T pair)` and the
 * triplets of the duplicate pairs `(pair i)`.
 */
function tripletsOnPair(links, markers, pair) {
  const outers = links.queryLinks(null, null, pair);
  const duplicated = links.queryLinks(null, pair, null).flatMap((duplicate) => {
    const outer = stored(links, duplicate.target);
    return outer != null && outer.target === duplicate.index ? [outer] : [];
  });
  return [...tripletsOf(links, markers, outers), ...tripletsOf(links, markers, duplicated)];
}

/** Sorted by the derived order of the Rust `Triplet`, each once. */
function sortedUnique(found) {
  const sorted = [...found].sort(compareTriplets);
  return sorted.filter((value, at) => at === 0 || compareTriplets(sorted[at - 1], value) !== 0);
}

/** The triplets other than `index` whose type, from or to is `index`. */
function usersOf(triplets, index) {
  const users = [
    ...triplets.eachTriplet(TripletQuery.byType(index)),
    ...triplets.eachTriplet(TripletQuery.byFrom(index)),
    ...triplets.eachTriplet(TripletQuery.byTo(index)),
  ];
  return sortedUnique(users.filter((user) => user.index !== index));
}

/**
 * `first` and every triplet that uses it, directly or through another one of
 * them: what deleting `first` deletes, `first` first.
 */
export function cascadeOf(triplets, first) {
  const deleted = [first];
  const seen = new Set([first.index]);
  for (let at = 0; at < deleted.length; at++) {
    for (const user of usersOf(triplets, deleted[at].index)) {
      if (!seen.has(user.index)) {
        seen.add(user.index);
        deleted.push(user);
      }
    }
  }
  return deleted;
}

/**
 * Writes the outer doublet `(index: source target)`, refusing to merge it
 * into another doublet that already is `(source target)`: the store would
 * delete `index` instead.
 */
function setOuter(links, index, source, target) {
  const existing = links.search(source, target);
  if (existing != null && existing !== index) {
    throw new Error(
      `The doublet (${source} ${target}) is already ${existing}: triplet ${index} would be merged into it.`,
    );
  }
  links.update(index, source, target);
}

/**
 * Deletes each wrapper, pair or duplicate pair of `candidates` (`[link,
 * role]` pairs) that nothing uses, then what it was made of, as long as
 * nothing else uses that either. A candidate the store has already deleted
 * leads on to what it was made of.
 */
function collect(triplets, markers, candidates) {
  const pending = [];
  for (const [value, doubletRole] of candidates) {
    if (triplets.exists(value.index)) pending.push(value.index);
    else if (leadsOn(doubletRole)) pending.push(value.source);
  }
  while (pending.length > 0) {
    const id = pending.pop();
    const value = stored(triplets, id);
    if (value == null) continue;
    const doubletRole = roleOf(triplets, markers, value);
    if (!isAuxiliary(doubletRole) || triplets.doubletUsages(id) > 0) continue;
    triplets.delete(id);
    if (leadsOn(doubletRole)) pending.push(value.source);
  }
}

/**
 * The reads and writes of triplets stored in doublets, as a mixin over a
 * `NamedTypeLinks` class.
 *
 * Only `tripletMarkers()` is required; everything else is a default method
 * that reads and writes through `this`, so a layer that wraps a
 * `TripletsOverDoublets` can mix this in by forwarding the markers and see
 * every doublet the encoding writes. Any default can be overridden: the
 * permissions decorator checks a triplet as a whole, above the wrappers and
 * pairs it is stored in.
 *
 * The names end in `Triplet` because a triplet store is still a doublet
 * store: `create`, `update` and `delete` keep working on the doublets
 * underneath.
 */
export const Triplets = (Base) =>
  class Triplets extends Base {
    /** What the doublet `id` is in the encoding; `null` when it is missing. */
    roleOf(id) {
      const value = stored(this, id);
      return value == null ? null : roleOf(this, this.tripletMarkers(), value);
    }

    /** The triplet at `index`, `null` when `index` is not a triplet. */
    getTriplet(index) {
      const doubletRole = this.roleOf(index);
      return doubletRole?.kind === 'Triplet' ? doubletRole.triplet : null;
    }

    /** Whether `index` is a triplet. */
    isTriplet(index) {
      return this.getTriplet(index) != null;
    }

    /**
     * Whether a reference to `index` names a triplet: the same as
     * `isTriplet`, but for a view that hides triplets, where one the subject
     * may refer to counts as well (issue #161).
     */
    referenceIsTriplet(index) {
      return this.isTriplet(index);
    }

    /**
     * Creates `(i: type from to)` and returns `i`. `type` must be a triplet;
     * `from` and `to` must be triplets or `0`.
     */
    createTriplet(typeId, from, to) {
      const markers = this.tripletMarkers();
      ensureType(this, typeId, null);
      ensurePart(this, from, null, 'from');
      ensurePart(this, to, null, 'to');
      if (from === 0 && to === 0) {
        const index = this.tryCreate(0, 0);
        setOuter(this, index, typeId, index);
        return index;
      }
      const pair = pairOf(this, markers, from, to);
      const free = this.search(typeId, pair) == null;
      const index = this.tryCreate(0, 0);
      // The outer `(type pair)` is taken: a duplicate pair `(pair index)` stands in.
      setOuter(this, index, typeId, free ? pair : this.tryGetOrCreate(pair, index));
      return index;
    }

    /** Creates a triplet `(i: i 0 0)` that is its own type: the root of a hierarchy has to start somewhere. */
    createTripletPoint() {
      const index = this.tryCreate(0, 0);
      this.update(index, index, index);
      return index;
    }

    /**
     * Creates the point `(address: address 0 0)` at an address nothing uses,
     * the way a substitution query defines a triplet at an address it names.
     */
    createTripletPointAt(address) {
      this.tryEnsureCreated(address);
      this.update(address, address, address);
    }

    /**
     * Makes the triplet at `index` `(index: type from to)`, keeping its
     * address, and returns the triplet as it was.
     *
     * `type` must be a triplet or `index` itself; `from` and `to` must be
     * triplets, `index` itself or `0`. The pair the triplet no longer uses is
     * collected when nothing else uses it.
     */
    updateTriplet(index, typeId, from, to) {
      const markers = this.tripletMarkers();
      const before = this.getTriplet(index);
      if (before == null) throw new Error(`Link ${index} is not a triplet.`);
      ensureType(this, typeId, index);
      ensurePart(this, from, index, 'from');
      ensurePart(this, to, index, 'to');
      if (before.typeId === typeId && before.from === from && before.to === to) return before;
      const outer = stored(this, index);
      if (outer == null) throw new Error(`Link ${index} does not exist.`);
      const oldTarget = outer.target;
      const newTarget =
        from === 0 && to === 0 ? index : this.#targetOf(markers, index, typeId, from, to);
      setOuter(this, index, typeId, newTarget);
      if (oldTarget !== newTarget && oldTarget !== index) {
        const old = [stored(this, oldTarget)].filter(present);
        collect(
          this,
          markers,
          old.map((value) => [value, roleOf(this, markers, value)]),
        );
      }
      return before;
    }

    /** The pair of a triplet with parts, or its duplicate pair when another triplet took `(type pair)`. */
    #targetOf(markers, index, typeId, from, to) {
      const pair = pairOf(this, markers, from, to);
      const taken = this.search(typeId, pair);
      return taken == null || taken === index ? pair : this.tryGetOrCreate(pair, index);
    }

    /**
     * Deletes the triplet at `index` and every triplet that uses a deleted
     * one as its type, from or to; returns them all, `index` first.
     *
     * Wrappers and pairs nothing uses any more are collected, so no doublet
     * is left behind.
     */
    deleteTriplet(index) {
      const markers = this.tripletMarkers();
      const first = this.getTriplet(index);
      if (first == null) throw new Error(`Link ${index} is not a triplet.`);
      const deleted = cascadeOf(this, first);
      // The wrappers and pairs that touch a deleted triplet, as they are now:
      // a store that deletes what uses a deleted doublet may take some of
      // them along, and their sources must still be collected.
      const auxiliary = [];
      for (const value of deleted) {
        const touching = [
          ...this.queryLinks(null, null, value.index),
          ...[stored(this, value.index)]
            .filter(present)
            .map((outer) => stored(this, outer.target))
            .filter(present),
        ];
        for (const doublet of touching) {
          const doubletRole = roleOf(this, markers, doublet);
          if (isAuxiliary(doubletRole)) auxiliary.push([doublet, doubletRole]);
        }
      }
      // Users go first, so a triplet is usually unused by the time it goes;
      // what still uses it (a cycle, a doublet of somebody else) the store
      // deletes with it, as it does with every doublet.
      for (const value of [...deleted].reverse()) {
        if (this.exists(value.index)) this.delete(value.index);
      }
      collect(this, markers, auxiliary);
      return deleted;
    }

    /**
     * The triplets that match `query`, a `TripletQuery`, in the order of
     * their addresses.
     *
     * A restriction by index is one lookup; by from (and to) it goes through
     * the wrapper of the from-part and its pairs, by to through the pairs
     * that end at it, by type through the doublets whose source is the type.
     * Only a query that restricts none of them, or only an empty from or to,
     * reads every doublet.
     */
    eachTriplet(query) {
      return sortedUnique(this.#candidates(query).filter((value) => query.matches(value)));
    }

    #candidates(query) {
      const markers = this.tripletMarkers();
      const { index, typeId, from, to } = query;
      if (index != null) return [this.getTriplet(index)].filter(present);
      if (from != null && to != null && (from !== 0 || to !== 0)) {
        const pair = findPair(this, markers, from, to);
        return pair == null ? [] : tripletsOnPair(this, markers, pair);
      }
      if (from != null && from !== 0) {
        const wrapper = this.search(markers.pair, from);
        if (wrapper == null) return [];
        return this.queryLinks(null, wrapper, null).flatMap((pair) =>
          tripletsOnPair(this, markers, pair.index),
        );
      }
      if (to != null && to !== 0) {
        return this.queryLinks(null, null, to)
          .filter((pair) => isWrapper(this, markers, pair.source))
          .flatMap((pair) => tripletsOnPair(this, markers, pair.index));
      }
      if (typeId != null) {
        if (typeId === 0 || markers.contains(typeId)) return [];
        return tripletsOf(this, markers, this.queryLinks(null, typeId, null));
      }
      return tripletsOf(this, markers, this.allLinks());
    }

    /** The number of triplets that match `query`. */
    countTriplets(query) {
      return this.eachTriplet(query).length;
    }

    /** How many other doublets use the doublet `id` as their source or target. */
    doubletUsages(id) {
      const other = (value) => value.index !== id;
      return (
        this.queryLinks(null, id, null).filter(other).length +
        this.queryLinks(null, null, id).filter(other).length
      );
    }
  };

/**
 * A decorator that stores triplets `(index: type from to)` in the doublets
 * of the layer it wraps; see the module comment.
 */
export class TripletsOverDoublets extends Triplets(NamedTypeLinks) {
  #links;
  #markers;

  /** Triplets whose wrappers start at `pair` and whose empty part is `nothing`. */
  constructor(links, pair, nothing) {
    super();
    this.#links = links;
    this.#markers = new TripletMarkers(pair, nothing);
  }

  static withMarkers(links, markers) {
    return new this(links, markers.pair, markers.nothing);
  }

  /** Triplets whose markers are the points named `Pair` and `Nothing`, created when missing. */
  static withNamedMarkers(links) {
    return this.withMarkers(links, TripletMarkers.ensure(links));
  }

  tripletMarkers() {
    return this.#markers;
  }

  /** The store below. */
  links() {
    return this.#links;
  }

  /** The store below, to write to it directly. */
  linksMut() {
    return this.#links;
  }

  /** The store below, for when this layer is no longer needed. */
  intoInner() {
    return this.#links;
  }

  create(source, target) {
    return this.#links.create(source, target);
  }

  ensureCreated(id) {
    return this.#links.ensureCreated(id);
  }

  tryEnsureCreated(id) {
    return this.#links.tryEnsureCreated(id);
  }

  getLink(id) {
    return this.#links.getLink(id);
  }

  exists(id) {
    return this.#links.exists(id);
  }

  tryCreate(source, target) {
    return this.#links.tryCreate(source, target);
  }

  tryGetOrCreate(source, target) {
    return this.#links.tryGetOrCreate(source, target);
  }

  referenceExists(id) {
    return this.#links.referenceExists(id);
  }

  claims(id) {
    return this.#links.claims(id);
  }

  nextCreatedAddresses(count) {
    return this.#links.nextCreatedAddresses(count);
  }

  updateObserved(id, source, target, observer) {
    return this.#links.updateObserved(id, source, target, observer);
  }

  deleteObserved(id, observer) {
    return this.#links.deleteObserved(id, observer);
  }

  allLinks() {
    return this.#links.allLinks();
  }

  queryLinks(index = null, source = null, target = null) {
    return this.#links.queryLinks(index, source, target);
  }

  search(source, target) {
    return this.#links.search(source, target);
  }

  getOrCreate(source, target) {
    return this.#links.getOrCreate(source, target);
  }

  getName(id) {
    return this.#links.getName(id);
  }

  setName(id, name) {
    return this.#links.setName(id, name);
  }

  getByName(name) {
    return this.#links.getByName(name);
  }

  removeName(id) {
    return this.#links.removeName(id);
  }

  save() {
    return this.#links.save();
  }

  getOrCreateNamed(name) {
    return this.#links.getOrCreateNamed(name);
  }
}
