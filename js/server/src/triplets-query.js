// Substitution queries over triplets: `((restriction) (substitution))`,
// where every pattern has three parts, `(index: type from to)` or
// `(type from to)`. Port of rust/src/triplets/query.rs.
//
// - A restriction pattern matches triplets; the patterns are a conjunction
//   over their variables (`$name`), and `*` matches anything.
// - A substitution pattern whose index is the index of a matched triplet
//   updates it, keeping its address; `*` in a part keeps that part.
// - A matched triplet no substitution pattern names is deleted, with the
//   triplets that use it.
// - Any other substitution pattern creates a triplet: at its address, under
//   its name, bound to its variable, or anywhere when it has no index.
//
// So `((i: T f t)) ((i: T f t2))` moves the to-part of `i`,
// `(() ((T f t)))` creates, `(((i: T f t)) ())` deletes and a query whose
// substitution repeats its restriction only reads.
//
// A reference is a `{ kind, ... }`: `Any`, `Variable` with its `name` (`$`
// included), `Address` with its `address`, or `Name` with its `name`.
import { parseU32 } from './link.js';
import { hasValues } from './parser.js';
import { name as nameInEnvelope, parse } from './query-parser.js';
import { triplet, tripletChange, TripletQuery } from './triplets-restriction.js';

const ANY = Object.freeze({ kind: 'Any' });

/** The reference a pattern part or index `text` is. */
export function parseReference(text) {
  const name = nameInEnvelope(text);
  if (name !== text) return { kind: 'Name', name };
  if (text === '*') return ANY;
  if (text.startsWith('$')) return { kind: 'Variable', name: text };
  const address = parseU32(text);
  return address == null ? { kind: 'Name', name: text } : { kind: 'Address', address };
}

/**
 * The patterns of one side of a query: `()` has none, `(i: T f t)` is one
 * and `((…) (…))` is a list of them.
 */
export function patternsOf(value) {
  const values = value.values ?? [];
  if (value.id != null || values.some((part) => !hasValues(part))) return [patternOf(value)];
  return values.map(patternOf);
}

/** The pattern `{ index, parts }` of `(index: type from to)` or `(type from to)`, `index` `null` when there is none. */
export function patternOf(value) {
  const values = value.values ?? [];
  if (values.length !== 3) {
    throw new Error(
      `A triplet pattern has three parts, type, from and to: ${describe(value)} has ${values.length}.`,
    );
  }
  const parts = values.map((part) => {
    if (part.id == null || hasValues(part)) {
      throw new Error(`A part of a triplet pattern is a reference, not ${describe(part)}.`);
    }
    return parseReference(part.id);
  });
  return { index: value.id == null ? null : parseReference(value.id), parts };
}

function describe(value) {
  const values = (value.values ?? []).map(describe).join(' ');
  if (value.id == null) return `(${values})`;
  return hasValues(value) ? `(${value.id}: ${values})` : value.id;
}

function nameOf(triplets, name) {
  try {
    return triplets.getByName(name) || null;
  } catch {
    return null;
  }
}

const SHAPE = 'A triplets query is ((restriction) (substitution)).';

/**
 * Runs a substitution query and returns the changes it made (`{ before,
 * after }`), in order: creations and updates as the substitution lists them,
 * then deletions.
 */
export function processTripletsQuery(triplets, query) {
  const parsed = parse(query);
  if (parsed.length === 0) return [];
  let sides = parsed;
  if (parsed.length === 1) {
    const [only] = parsed;
    sides = only.id == null ? (only.values ?? []) : [];
  }
  if (sides.length !== 2) throw new Error(SHAPE);
  const restrictions = patternsOf(sides[0]);
  const substitutions = patternsOf(sides[1]);
  const solutions = [];
  if (restrictions.length === 0) solutions.push([new Map(), []]);
  else solve(triplets, restrictions, new Map(), [], solutions);
  const changes = [];
  const deletes = [];
  for (const [bindings, matched] of solutions) {
    const kept = new Set(substitute(triplets, substitutions, bindings, changes));
    for (const index of matched) {
      if (!kept.has(index) && !deletes.includes(index)) deletes.push(index);
    }
  }
  for (const index of deletes) {
    if (triplets.isTriplet(index)) {
      for (const deleted of triplets.deleteTriplet(index)) {
        changes.push(tripletChange(deleted, null));
      }
    }
  }
  return changes;
}

const IMPOSSIBLE = Symbol('impossible');

/** The value a reference restricts its place to: `null` for any, `IMPOSSIBLE` for an unknown name. */
function restrictionOf(triplets, reference, bindings) {
  switch (reference.kind) {
    case 'Any':
      return null;
    case 'Variable':
      return bindings.get(reference.name) ?? null;
    case 'Address':
      return reference.address;
    default:
      return nameOf(triplets, reference.name) ?? IMPOSSIBLE;
  }
}

function bind(bindings, reference, value) {
  if (reference.kind !== 'Variable') return true;
  const bound = bindings.get(reference.name);
  if (bound != null) return bound === value;
  bindings.set(reference.name, value);
  return true;
}

/**
 * The values of `$variable` under which every pattern of `restriction`
 * matches a triplet, each once, in the order found: a read-only substitution
 * query, as the GraphQL layer asks them (issue #153).
 */
export function matchTripletsQuery(triplets, restriction, variable) {
  const parsed = parse(restriction);
  if (parsed.length !== 1) {
    throw new Error(`A restriction is one link of patterns: ${restriction}`);
  }
  const solutions = [];
  solve(triplets, patternsOf(parsed[0]), new Map(), [], solutions);
  const values = solutions
    .map(([bindings]) => bindings.get(variable))
    .filter((value) => value != null);
  return [...new Set(values)];
}

/**
 * Every assignment of the variables under which each restriction pattern
 * matches a triplet, with the triplets they matched, pushed to `solutions`
 * as `[bindings, matched]`.
 */
function solve(triplets, patterns, bindings, matched, solutions) {
  if (patterns.length === 0) {
    solutions.push([bindings, [...matched]]);
    return;
  }
  const [pattern, ...rest] = patterns;
  const references = [pattern.index ?? ANY, ...pattern.parts];
  const values = [];
  for (const reference of references) {
    const value = restrictionOf(triplets, reference, bindings);
    if (value === IMPOSSIBLE) return;
    values.push(value);
  }
  for (const found of triplets.eachTriplet(new TripletQuery(...values))) {
    const next = new Map(bindings);
    const parts = [found.index, found.typeId, found.from, found.to];
    if (references.every((reference, at) => bind(next, reference, parts[at]))) {
      matched.push(found.index);
      solve(triplets, rest, next, matched, solutions);
      matched.pop();
    }
  }
}

/** What a substitution pattern with the index `reference` writes to: `{ kind: 'Existing' | 'Address' | 'New', ... }`. */
function targetOf(triplets, reference, bindings, defined) {
  if (reference == null) return { kind: 'New', reference: null };
  switch (reference.kind) {
    case 'Any':
      throw new Error('A substitution cannot create or update the index *.');
    case 'Variable': {
      const bound = bindings.get(reference.name);
      if (bound != null) return { kind: 'Existing', index: bound };
      defined.variables.add(reference.name);
      return { kind: 'New', reference };
    }
    case 'Name': {
      const id = nameOf(triplets, reference.name);
      if (id != null) return { kind: 'Existing', index: id };
      defined.names.add(reference.name);
      return { kind: 'New', reference };
    }
    default: {
      const { address } = reference;
      if (address === 0) throw new Error('A triplet cannot be at address 0.');
      if (triplets.isTriplet(address)) return { kind: 'Existing', index: address };
      if (triplets.exists(address)) {
        throw new Error(`Link ${address} is a doublet, not a triplet.`);
      }
      defined.addresses.add(address);
      return { kind: 'Address', address };
    }
  }
}

/** Whether the part `part` at `position` of a pattern writing to `target` refers to a triplet. */
function isKnown(triplets, part, position, target, bindings, defined) {
  switch (part.kind) {
    case 'Any':
      return target.kind === 'Existing';
    case 'Variable':
      return bindings.has(part.name) || defined.variables.has(part.name);
    case 'Name':
      return nameOf(triplets, part.name) != null || defined.names.has(part.name);
    default:
      if (part.address === 0) return position > 0;
      return triplets.referenceIsTriplet(part.address) || defined.addresses.has(part.address);
  }
}

const POSITIONS = ['type', 'from', 'to'];

function describeReference(reference) {
  switch (reference.kind) {
    case 'Any':
      return '*';
    case 'Address':
      return String(reference.address);
    default:
      return reference.name;
  }
}

/** The triplet a `New` target is when a name or a variable defined twice already made it. */
function knownIndex(triplets, reference, bindings) {
  if (reference == null) return null;
  if (reference.kind === 'Name') return nameOf(triplets, reference.name);
  return bindings.get(reference.name) ?? null;
}

/** The index a target writes to, reserving the triplet first when it is new. */
function reserve(triplets, target, bindings, created) {
  if (target.kind === 'Existing') return target.index;
  if (target.kind === 'Address') {
    if (!triplets.isTriplet(target.address)) {
      triplets.createTripletPointAt(target.address);
      created.add(target.address);
    }
    return target.address;
  }
  const { reference } = target;
  const known = knownIndex(triplets, reference, bindings);
  if (known != null) return known;
  const index = triplets.createTripletPoint();
  if (reference?.kind === 'Name') triplets.setName(index, reference.name);
  else if (reference != null) bindings.set(reference.name, index);
  created.add(index);
  return index;
}

function partValue(triplets, part, current, bindings) {
  switch (part.kind) {
    case 'Any':
      return current;
    case 'Variable':
      return bindings.get(part.name);
    case 'Address':
      return part.address;
    default: {
      const id = nameOf(triplets, part.name);
      if (id == null) throw new Error(`The name ${part.name} is not a triplet.`);
      return id;
    }
  }
}

/**
 * Applies the substitution patterns under `bindings` (a `Map` of variables
 * it extends): updates the triplets they name, creates the others, pushes
 * the changes to `changes` and returns the index each pattern wrote, in the
 * order of the patterns.
 *
 * Every triplet to create is reserved first as a point `(i: i i)`, which is
 * a triplet, so the patterns may refer to each other in any order; then each
 * gets its parts. Every reference is checked before anything is written.
 */
export function substitute(triplets, patterns, bindings, changes) {
  const defined = { names: new Set(), variables: new Set(), addresses: new Set() };
  const targets = patterns.map((pattern) => {
    const target = targetOf(triplets, pattern.index, bindings, defined);
    if (target.kind === 'Existing' && !triplets.isTriplet(target.index)) {
      throw new Error(`Link ${target.index} is not a triplet.`);
    }
    return target;
  });
  patterns.forEach((pattern, at) => {
    pattern.parts.forEach((part, position) => {
      if (!isKnown(triplets, part, position, targets[at], bindings, defined)) {
        throw new Error(
          `The ${POSITIONS[position]} part ${describeReference(part)} of a substitution is not a triplet.`,
        );
      }
    });
  });

  const created = new Set();
  const indices = targets.map((target) => reserve(triplets, target, bindings, created));

  const kept = new Set();
  patterns.forEach((pattern, at) => {
    const index = indices[at];
    const before = triplets.getTriplet(index);
    const current = before ?? triplet(index, index, 0, 0);
    const currents = [current.typeId, current.from, current.to];
    const parts = pattern.parts.map((part, position) =>
      partValue(triplets, part, currents[position], bindings),
    );
    triplets.updateTriplet(index, ...parts);
    const after = triplets.getTriplet(index);
    // A triplet two patterns write is reported once, as it ends up.
    const change = kept.has(index)
      ? changes.findLast((each) => each.after != null && each.after.index === index)
      : undefined;
    if (change != null) change.after = after;
    else changes.push(tripletChange(created.has(index) ? null : before, after));
    kept.add(index);
  });
  return indices;
}
