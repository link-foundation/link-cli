// What the triplet tests share: the helpers of
// rust/tests/triplets_over_doublets_tests.rs.
import assert from 'node:assert/strict';
import { LinkStorage } from '../../src/link-storage.js';
import {
  DoubletRole,
  importTripletsLino,
  isAuxiliary,
  processTripletsQuery,
  triplet,
  TripletsOverDoublets,
} from '../../src/triplets.js';

/** Triplets over an in-memory store with indexed lookups, as in Rust. */
export const store = () =>
  TripletsOverDoublets.withNamedMarkers(LinkStorage.inMemory().withIndexedLookups());

/** Every doublet is a marker, a triplet, or a wrapper or pair still in use. */
export function assertNoOrphans(triplets, allowForeign = false) {
  for (const value of triplets.allLinks()) {
    const role = triplets.roleOf(value.index);
    assert.notEqual(role, null, 'a stored doublet has a role');
    if (isAuxiliary(role)) {
      assert.ok(triplets.doubletUsages(value.index) > 0, `orphan ${role.kind} ${value.index}`);
    } else if (role === DoubletRole.Foreign) {
      assert.ok(allowForeign, `a doublet that is no triplet: ${JSON.stringify(value)}`);
    }
  }
}

/** `Type`, `Person`, `knows`, `alice`, `bob` and `carol`. */
export function people(triplets) {
  const root = triplets.createTripletPoint();
  const person = triplets.createTriplet(root, 0, 0);
  const knows = triplets.createTriplet(root, 0, 0);
  const alice = triplets.createTriplet(person, 0, 0);
  const bob = triplets.createTriplet(person, 0, 0);
  const carol = triplets.createTriplet(person, 0, 0);
  return { root, person, knows, alice, bob, carol };
}

export const PEOPLE = `(Type: Type 0 0)
(Person: Type 0 0)
(knows: Type 0 0)
(alice: Person 0 0)
(bob: Person 0 0)
(carol: Person 0 0)
(r: knows alice bob)
`;

export function namedPeople() {
  const triplets = store();
  importTripletsLino(triplets, PEOPLE);
  return triplets;
}

/** The link named `name`, which must exist. */
export function id(triplets, name) {
  const found = triplets.getByName(name);
  assert.notEqual(found, null, name);
  return found;
}

/** The triplet of four names, `0` standing for an empty part. */
export function named(triplets, names) {
  const part = (name) => (name === '0' ? 0 : id(triplets, name));
  return triplet(id(triplets, names[0]), id(triplets, names[1]), part(names[2]), part(names[3]));
}

/** A query whose result leaves no orphan behind. */
export function query(triplets, text) {
  const changes = processTripletsQuery(triplets, text);
  assertNoOrphans(triplets);
  return changes;
}

/** `{ before, after }` as the queries report it. */
export const change = (before, after) => ({ before, after });
