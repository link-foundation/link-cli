// A triplet store behind a permissions view: the store-level cases of
// rust/tests/triplets_refusals_tests.rs, and every rule of
// rust/src/triplets/permissions.rs.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { NotFoundError } from '../src/errors.js';
import { AccessPolicy } from '../src/permissions.js';
import { Forbidden } from '../src/permissions-named-links.js';
import { processTripletsQuery, triplet, TripletQuery, tripletsToLino } from '../src/triplets.js';
import { TripletsPermissionsDecorator } from '../src/triplets-permissions.js';
import { people, store } from './helpers/triplets.mjs';

/** A policy of four questions, each about the link asked for. */
class Rules extends AccessPolicy {
  constructor({
    read = () => true,
    write = () => true,
    create = true,
    reference = () => false,
  } = {}) {
    super();
    this.rules = { read, write, create, reference };
  }

  canRead(_data, _subject, value) {
    return this.rules.read(value);
  }

  canWrite(_data, _subject, value) {
    return this.rules.write(value);
  }

  canCreate(_data, _subject) {
    return this.rules.create;
  }

  canReference(_data, _subject, value) {
    return this.rules.reference(value);
  }
}

/** Everything may be written and read; only some links may be referred to. */
const onlySomeReferences = (mine) =>
  new Rules({
    read: (value) => mine.has(value.index),
    reference: (value) => mine.has(value.index),
  });

/** `Type` 3, `Person` 4, `knows` 5, `alice` 6, `bob` 7, `carol` 8 and `r` 11: `knows alice bob`. */
function world(rules) {
  const triplets = store();
  const p = people(triplets);
  const r = triplets.createTriplet(p.knows, p.alice, p.bob);
  return { triplets, ...p, r, view: new TripletsPermissionsDecorator(triplets, rules, 0) };
}

const notFound = (id) => (error) =>
  error instanceof NotFoundError && error.message === `Link not found: ${id}`;
const forbidden = (what) => (error) =>
  error instanceof Forbidden && error.message === `Forbidden: ${what}`;

describe('TripletsPermissionsDecorator', () => {
  it('answers alike for what it may not refer to', () => {
    const triplets = store();
    const markers = triplets.tripletMarkers();
    const hidden = triplets.createTripletPoint();
    const visibleType = triplets.createTripletPoint();
    const own = triplets.createTriplet(visibleType, 0, 0);
    const mine = new Set([visibleType, own, 90]);
    const view = new TripletsPermissionsDecorator(triplets, onlySomeReferences(mine), 0);

    assert.deepEqual(view.tripletMarkers(), markers);
    // A hidden type and a free address both answer as a missing link.
    assert.throws(() => view.createTriplet(hidden, 0, 0), { message: /^Link not found/ });
    assert.throws(() => view.updateTriplet(own, hidden, 0, 0), { message: /^Link not found/ });
    assert.throws(() => view.createTripletPointAt(91), { message: /^Link not found/ });

    // An address the subject may name can be taken once.
    view.createTripletPointAt(90);
    assert.equal(view.isTriplet(90), true);
    assert.throws(() => view.createTripletPointAt(90), { message: /exists/ });
  });

  it('shows only the triplets the subject may read', () => {
    const { view, root, person, knows, alice, bob, carol, r } = world(
      new Rules({ read: (value) => value.index !== bob }),
    );
    assert.equal(view.getTriplet(bob), null);
    assert.equal(view.roleOf(bob), null);
    assert.equal(view.roleOf(999), null);
    assert.deepEqual(view.roleOf(1), { kind: 'Marker' });
    assert.deepEqual(view.getTriplet(alice), triplet(alice, person, 0, 0));
    assert.deepEqual(
      view.eachTriplet(TripletQuery.any()).map((value) => value.index),
      [root, person, knows, alice, carol, r],
    );
    assert.equal(view.countTriplets(TripletQuery.byType(person)), 2);
    assert.equal(view.countTriplets(TripletQuery.byTo(bob)), 1);
  });

  it('counts a hidden triplet it may refer to as a triplet', () => {
    const { view, alice, bob, carol } = world(
      new Rules({
        read: (value) => value.index !== bob && value.index !== carol,
        reference: (value) => value.index === bob || value.index === 1,
      }),
    );
    assert.equal(view.referenceIsTriplet(alice), true);
    assert.equal(view.referenceIsTriplet(bob), true);
    assert.equal(view.referenceIsTriplet(carol), false);
    assert.equal(view.referenceIsTriplet(1), false);
    assert.equal(view.referenceIsTriplet(999), false);
  });

  it('refuses a create without the rights to create and to write the type', () => {
    const closed = world(new Rules({ create: false }));
    assert.throws(() => closed.view.createTriplet(closed.root, 0, 0), forbidden('create'));
    assert.throws(() => closed.view.createTripletPoint(), forbidden('create'));
    assert.throws(() => closed.view.createTripletPointAt(90), forbidden('create'));

    const { view, triplets, person, knows, alice, carol } = world(
      new Rules({ write: (value) => value.source !== person }),
    );
    const before = tripletsToLino(triplets);
    assert.throws(() => view.createTriplet(person, 0, 0), forbidden('create'));
    assert.throws(() => view.createTriplet(0, 0, 0), {
      message: 'The type of a triplet cannot be 0.',
    });
    assert.equal(tripletsToLino(triplets), before);
    const created = view.createTriplet(knows, alice, carol);
    assert.deepEqual(view.getTriplet(created), triplet(created, knows, alice, carol));
    assert.equal(view.created().has(created), true);
  });

  it('refuses parts it may not refer to as missing ones', () => {
    const { view, triplets, knows, bob } = world(
      new Rules({ read: (value) => value.index !== bob }),
    );
    const before = tripletsToLino(triplets);
    assert.throws(() => view.createTriplet(bob, 0, 0), notFound(bob));
    assert.throws(() => view.createTriplet(knows, bob, 0), {
      message: `The from part ${bob} is not a triplet.`,
    });
    assert.throws(() => view.createTriplet(knows, 0, bob), {
      message: `The to part ${bob} is not a triplet.`,
    });
    assert.throws(() => view.createTriplet(knows, 1, 0), {
      message: 'The from part 1 is not a triplet.',
    });
    assert.equal(tripletsToLino(triplets), before);

    // A hidden link the subject may refer to is not a free address.
    const referable = world(
      new Rules({ read: (value) => value.index !== 7, reference: (value) => value.index === 7 }),
    );
    assert.throws(() => referable.view.createTripletPointAt(7), { message: 'Link 7 exists' });
  });

  it('keeps a point it created, still only its own type, as its own', () => {
    const { view, triplets } = world(new Rules({ read: () => false }));
    const point = view.createTripletPoint();
    assert.deepEqual(view.getTriplet(point), triplet(point, point, 0, 0));
    // Typed, the triplet is no longer the subject's to read.
    const typed = view.createTriplet(point, 0, 0);
    assert.equal(view.getTriplet(typed), null);
    assert.equal(triplets.isTriplet(typed), true);
    assert.deepEqual(view.created(), new Set([point, typed]));
    // Deleting the point takes the typed triplet along, and the claims.
    const deleted = view.deleteTriplet(point);
    assert.deepEqual(
      deleted.map((value) => value.index),
      [point, typed],
    );
    assert.deepEqual(view.created(), new Set());
    assert.equal(triplets.isTriplet(point), false);
  });

  it('refuses an update without the right to write it before and after', () => {
    const missing = world(new Rules({ read: (value) => value.index !== 7 }));
    assert.throws(() => missing.view.updateTriplet(999, 3, 0, 0), {
      message: 'Link 999 is not a triplet.',
    });
    assert.throws(() => missing.view.updateTriplet(7, 3, 0, 0), {
      message: 'Link 7 is not a triplet.',
    });

    const locked = world(new Rules({ write: (value) => value.index !== 11 }));
    assert.throws(() => locked.view.updateTriplet(11, 5, 6, 8), forbidden('update 11'));

    // The triplet as it will be: typed `Person`, or empty, which is its own target.
    const { view, triplets, person, knows, alice, bob, r } = world(
      new Rules({
        write: (value) =>
          value.source !== person && !(value.source === knows && value.target === 11),
      }),
    );
    const before = tripletsToLino(triplets);
    assert.throws(() => view.updateTriplet(r, person, alice, bob), forbidden(`update ${r}`));
    assert.throws(() => view.updateTriplet(r, knows, 0, 0), forbidden(`update ${r}`));
    assert.throws(() => view.updateTriplet(r, 0, alice, bob), {
      message: 'The type of a triplet cannot be 0.',
    });
    assert.throws(() => view.updateTriplet(r, knows, alice, 999), {
      message: 'The to part 999 is not a triplet.',
    });
    assert.equal(tripletsToLino(triplets), before);
    // The update answers the triplet as it was.
    assert.deepEqual(view.updateTriplet(r, knows, alice, 0), triplet(r, knows, alice, bob));
    assert.deepEqual(view.getTriplet(r), triplet(r, knows, alice, 0));
  });

  it('lets a part the triplet holds stay, but not come back', () => {
    const { view, knows, alice, bob, carol, r } = world(
      new Rules({ read: (value) => value.index !== 6 && value.index !== 8 }),
    );
    // `carol` is hidden and not held: a missing type.
    assert.throws(() => view.updateTriplet(r, carol, alice, bob), notFound(carol));
    // `alice` is hidden but held, as the type, the from or the to part.
    view.updateTriplet(r, alice, alice, bob);
    assert.deepEqual(view.getTriplet(r), triplet(r, alice, alice, bob));
    view.updateTriplet(r, knows, bob, alice);
    view.updateTriplet(r, knows, bob, bob);
    assert.throws(() => view.updateTriplet(r, knows, alice, bob), {
      message: `The from part ${alice} is not a triplet.`,
    });
    assert.throws(() => view.updateTriplet(r, alice, bob, bob), notFound(alice));
  });

  it('refuses a delete that takes along a triplet it may not write', () => {
    const { view, triplets, alice, r } = world(new Rules({ write: (value) => value.index !== 11 }));
    assert.throws(() => view.deleteTriplet(999), { message: 'Link 999 is not a triplet.' });
    const before = tripletsToLino(triplets);
    assert.throws(() => view.deleteTriplet(alice), forbidden(`delete ${alice}`));
    assert.equal(tripletsToLino(triplets), before);
    assert.equal(triplets.isTriplet(r), true);
  });

  it('runs substitution queries as the subject', () => {
    const { view, triplets, knows, alice, bob, carol } = world(
      new Rules({ read: (value) => value.index !== bob }),
    );
    const [created] = processTripletsQuery(view, '() ((n: 5 6 8))');
    assert.deepEqual(created.after, triplet(created.after.index, knows, alice, carol));
    assert.equal(triplets.getName(created.after.index), 'n');
    assert.throws(() => processTripletsQuery(view, '() ((5 6 7))'), {
      message: `The to part ${bob} of a substitution is not a triplet.`,
    });
    // A restriction sees the triplets the subject reads, whatever their parts.
    assert.deepEqual(
      processTripletsQuery(view, '((($i: 5 6 $t)) (($i: 5 6 $t)))').map((each) => each.after.to),
      [bob, carol],
    );
  });
});
