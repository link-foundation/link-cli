// Triplets stored in doublets (issue #151): the store cases of
// rust/tests/triplets_over_doublets_tests.rs, with the same pseudo-random
// sequence in the parity test.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { link } from '../src/link.js';
import { LinkStorage } from '../src/link-storage.js';
import {
  DoubletRole,
  NOTHING_MARKER_NAME,
  PAIR_MARKER_NAME,
  triplet,
  TripletMarkers,
  TripletQuery,
  TripletsOverDoublets,
} from '../src/triplets.js';
import { assertNoOrphans, people, store } from './helpers/triplets.mjs';

describe('TripletsOverDoublets', () => {
  it('keeps its markers as named points', () => {
    const triplets = store();
    const markers = triplets.tripletMarkers();
    assert.equal(triplets.getByName(PAIR_MARKER_NAME), markers.pair);
    assert.equal(triplets.getByName(NOTHING_MARKER_NAME), markers.nothing);
    for (const marker of [markers.pair, markers.nothing]) {
      assert.deepEqual(triplets.getLink(marker), link(marker, marker, marker));
      assert.equal(triplets.roleOf(marker), DoubletRole.Marker);
      assert.equal(triplets.isTriplet(marker), false);
    }
    // The same markers are found again.
    const again = TripletsOverDoublets.withNamedMarkers(triplets.intoInner());
    assert.deepEqual(again.tripletMarkers(), markers);
    assert.equal(again.links(), triplets.links());
    assert.equal(again.linksMut(), triplets.links());
  });

  it('can have its markers chosen by address', () => {
    const links = LinkStorage.inMemory().withIndexedLookups();
    const pair = links.tryCreate(0, 0);
    const nothing = links.tryCreate(0, 0);
    const triplets = new TripletsOverDoublets(links, pair, nothing);
    assert.deepEqual(triplets.tripletMarkers(), new TripletMarkers(pair, nothing));
    assert.equal(triplets.isTriplet(triplets.createTripletPoint()), true);
  });

  it('stores nodes and links', () => {
    const triplets = store();
    const markers = triplets.tripletMarkers();
    const p = people(triplets);
    // A node (i: T 0 0) is the doublet (i: T i), a value of #148.
    assert.deepEqual(triplets.getLink(p.root), link(p.root, p.root, p.root));
    assert.deepEqual(triplets.getLink(p.alice), link(p.alice, p.person, p.alice));
    assert.deepEqual(triplets.getTriplet(p.alice), triplet(p.alice, p.person, 0, 0));
    assert.deepEqual(triplets.getTriplet(p.root), triplet(p.root, p.root, 0, 0));

    const created = triplets.createTriplet(p.knows, p.alice, p.bob);
    assert.deepEqual(triplets.getTriplet(created), triplet(created, p.knows, p.alice, p.bob));
    // (link: knows pair), pair = (wrapper bob), wrapper = (Pair alice).
    const outer = triplets.getLink(created);
    assert.equal(outer.source, p.knows);
    const pair = triplets.getLink(outer.target);
    assert.equal(pair.target, p.bob);
    assert.equal(triplets.roleOf(pair.index), DoubletRole.Pair);
    const wrapper = triplets.getLink(pair.source);
    assert.deepEqual(wrapper, link(wrapper.index, markers.pair, p.alice));
    assert.equal(triplets.roleOf(wrapper.index), DoubletRole.Wrapper);
    assert.equal(triplets.roleOf(10_000), null);
    assert.equal(triplets.getTriplet(wrapper.index), null);
    assert.equal(triplets.getTriplet(0), null);
    assert.equal(triplets.allLinks().length, 11);
    assertNoOrphans(triplets);
  });

  it('writes zero parts as Nothing', () => {
    const triplets = store();
    const markers = triplets.tripletMarkers();
    const p = people(triplets);
    const fromOnly = triplets.createTriplet(p.knows, p.alice, 0);
    const toOnly = triplets.createTriplet(p.knows, 0, p.bob);
    assert.deepEqual(triplets.getTriplet(fromOnly), triplet(fromOnly, p.knows, p.alice, 0));
    assert.deepEqual(triplets.getTriplet(toOnly), triplet(toOnly, p.knows, 0, p.bob));
    const outer = triplets.getLink(toOnly);
    const pair = triplets.getLink(outer.target);
    assert.equal(triplets.getLink(pair.source).target, markers.nothing);
    assert.deepEqual(triplets.eachTriplet(new TripletQuery(null, p.knows, 0, null)), [
      triplet(toOnly, p.knows, 0, p.bob),
    ]);
    assert.deepEqual(triplets.eachTriplet(new TripletQuery(null, null, p.alice, 0)), [
      triplet(fromOnly, p.knows, p.alice, 0),
    ]);
    // Nodes have neither: an empty from finds them as well.
    assert.equal(triplets.countTriplets(TripletQuery.byFrom(0)), 7);
    assert.equal(triplets.countTriplets(TripletQuery.byTo(0)), 7);
    assert.equal(triplets.countTriplets(new TripletQuery(null, null, 0, 0)), 6);
    assertNoOrphans(triplets);
  });

  it('gives duplicates their own address', () => {
    const triplets = store();
    const p = people(triplets);
    const doublets = triplets.allLinks().length;
    const first = triplets.createTriplet(p.knows, p.alice, p.bob);
    const second = triplets.createTriplet(p.knows, p.alice, p.bob);
    const third = triplets.createTriplet(p.knows, p.alice, p.bob);
    assert.notEqual(first, second);
    assert.notEqual(second, third);
    // The second and third go through duplicate pairs (pair i).
    const pair = triplets.getLink(first).target;
    const duplicate = triplets.getLink(triplets.getLink(second).target);
    assert.deepEqual(duplicate, link(duplicate.index, pair, second));
    assert.equal(triplets.roleOf(duplicate.index), DoubletRole.DuplicatePair);
    const expected = [first, second, third].map((index) => triplet(index, p.knows, p.alice, p.bob));
    assert.deepEqual(triplets.eachTriplet(TripletQuery.byFrom(p.alice)), expected);
    assert.deepEqual(triplets.eachTriplet(TripletQuery.byTo(p.bob)), expected);
    assert.deepEqual(
      triplets.eachTriplet(new TripletQuery(null, p.knows, p.alice, p.bob)),
      expected,
    );
    assert.equal(triplets.countTriplets(new TripletQuery(null, null, null, p.bob)), 3);

    triplets.deleteTriplet(first);
    assert.deepEqual(triplets.eachTriplet(TripletQuery.byFrom(p.alice)), expected.slice(1));
    assertNoOrphans(triplets);
    triplets.deleteTriplet(third);
    triplets.deleteTriplet(second);
    assert.equal(triplets.allLinks().length, doublets);
    assertNoOrphans(triplets);
  });

  it('keeps the address on an update and collects the old pair', () => {
    const triplets = store();
    const p = people(triplets);
    const doublets = triplets.allLinks().length;
    const created = triplets.createTriplet(p.knows, p.alice, p.bob);
    const oldPair = triplets.getLink(created).target;
    const before = triplets.updateTriplet(created, p.knows, p.alice, p.carol);
    assert.deepEqual(before, triplet(created, p.knows, p.alice, p.bob));
    assert.deepEqual(triplets.getTriplet(created), triplet(created, p.knows, p.alice, p.carol));
    assert.equal(triplets.exists(oldPair), false, 'the old pair is collected');
    assert.equal(triplets.allLinks().length, doublets + 3);

    // A new type and new parts, then a node and back: the address stays.
    triplets.updateTriplet(created, p.person, p.bob, p.alice);
    assert.deepEqual(triplets.getTriplet(created), triplet(created, p.person, p.bob, p.alice));
    triplets.updateTriplet(created, p.person, 0, 0);
    assert.deepEqual(triplets.getTriplet(created), triplet(created, p.person, 0, 0));
    assert.equal(triplets.allLinks().length, doublets + 1);
    triplets.updateTriplet(created, created, created, created);
    assert.deepEqual(triplets.getTriplet(created), triplet(created, created, created, created));
    assertNoOrphans(triplets);
    triplets.updateTriplet(created, p.knows, p.alice, p.bob);
    assert.equal(triplets.allLinks().length, doublets + 3);
    assertNoOrphans(triplets);
    const unchanged = triplets.updateTriplet(created, p.knows, p.alice, p.bob);
    assert.deepEqual(unchanged, triplet(created, p.knows, p.alice, p.bob));
    assert.equal(triplets.getTriplet(p.person).typeId, p.root);
  });

  it('keeps a shared pair while it is used', () => {
    const triplets = store();
    const p = people(triplets);
    const knows = triplets.createTriplet(p.knows, p.alice, p.bob);
    const met = triplets.createTriplet(p.person, p.alice, p.bob);
    const pair = triplets.getLink(knows).target;
    assert.equal(triplets.getLink(met).target, pair, 'one pair, two triplets');
    triplets.updateTriplet(knows, p.knows, p.alice, p.carol);
    assert.equal(triplets.exists(pair), true, 'the pair is still used');
    triplets.deleteTriplet(met);
    assert.equal(triplets.exists(pair), false);
    assertNoOrphans(triplets);
    // An update into a slot another triplet holds takes a duplicate pair, and
    // an update out of it collects the duplicate pair.
    const first = triplets.createTriplet(p.knows, p.bob, p.carol);
    const second = triplets.createTriplet(p.person, p.bob, p.carol);
    triplets.updateTriplet(second, p.knows, p.bob, p.carol);
    assert.equal(triplets.roleOf(triplets.getLink(second).target), DoubletRole.DuplicatePair);
    assert.equal(triplets.countTriplets(TripletQuery.byTo(p.carol)), 3);
    triplets.updateTriplet(second, p.person, p.bob, p.carol);
    assert.equal(triplets.getLink(second).target, triplets.getLink(first).target);
    assertNoOrphans(triplets);
  });

  it('cascades a delete to the triplets that use the deleted one', () => {
    const triplets = store();
    const p = people(triplets);
    const markersAndNodes = triplets.allLinks().length;
    const ab = triplets.createTriplet(p.knows, p.alice, p.bob);
    const bc = triplets.createTriplet(p.knows, p.bob, p.carol);
    const about = triplets.createTriplet(p.knows, ab, bc);
    const deleted = triplets.deleteTriplet(p.alice);
    assert.deepEqual(
      deleted.map((value) => value.index),
      [p.alice, ab, about],
    );
    assert.equal(triplets.isTriplet(bc), true);
    assertNoOrphans(triplets);
    // A cycle: two triplets that use each other.
    const first = triplets.createTriplet(p.knows, p.bob, 0);
    const second = triplets.createTriplet(p.knows, first, 0);
    triplets.updateTriplet(first, p.knows, second, 0);
    triplets.deleteTriplet(second);
    assert.equal(triplets.exists(first), false);
    triplets.deleteTriplet(bc);
    assert.equal(triplets.allLinks().length, markersAndNodes - 1);
    assertNoOrphans(triplets);
    // The root takes every triplet with it.
    triplets.deleteTriplet(p.root);
    assert.equal(triplets.allLinks().length, 2, 'only the markers are left');
  });

  it('refuses parts that are not triplets', () => {
    const triplets = store();
    const markers = triplets.tripletMarkers();
    const p = people(triplets);
    const created = triplets.createTriplet(p.knows, p.alice, p.bob);
    const pair = triplets.getLink(created).target;
    const refused = [
      [() => triplets.createTriplet(0, 0, 0), 'The type of a triplet cannot be 0.'],
      [
        () => triplets.createTriplet(markers.pair, 0, 0),
        `The type ${markers.pair} is not a triplet.`,
      ],
      [() => triplets.createTriplet(pair, 0, 0), `The type ${pair} is not a triplet.`],
      [() => triplets.createTriplet(p.knows, 10_000, 0), 'The from part 10000 is not a triplet.'],
      [
        () => triplets.createTriplet(p.knows, 0, markers.nothing),
        `The to part ${markers.nothing} is not a triplet.`,
      ],
      [() => triplets.updateTriplet(pair, p.knows, 0, 0), `Link ${pair} is not a triplet.`],
      [() => triplets.updateTriplet(created, 0, 0, 0), 'The type of a triplet cannot be 0.'],
      [() => triplets.deleteTriplet(pair), `Link ${pair} is not a triplet.`],
      [() => triplets.deleteTriplet(10_000), 'Link 10000 is not a triplet.'],
    ];
    for (const [write, message] of refused) assert.throws(write, { message });
    assert.deepEqual(triplets.getTriplet(created), triplet(created, p.knows, p.alice, p.bob));
  });
});

/** The same pseudo-random sequence as in C# and Rust: a 64-bit LCG. */
class Random {
  #state;

  constructor(seed) {
    this.#state = BigInt(seed);
  }

  next(bound) {
    this.#state = (this.#state * 6364136223846793005n + 1442695040888963407n) & 0xffffffffffffffffn;
    return Number((this.#state >> 33n) % BigInt(bound));
  }
}

const part = (random, ids) => (random.next(4) === 0 ? 0 : ids[random.next(ids.length)]);

/** A triplet store kept in a map: what the decorator must agree with. */
class ReferenceStore {
  constructor() {
    this.triplets = new Map();
  }

  insert(index, typeId, from, to) {
    const known = this.triplets.has(index);
    this.triplets.set(index, triplet(index, typeId, from, to));
    return known;
  }

  ids() {
    return [...this.triplets.keys()].sort((left, right) => left - right);
  }

  each(query) {
    return this.ids()
      .map((index) => this.triplets.get(index))
      .filter((value) => query.matches(value));
  }

  delete(index) {
    const pending = [index];
    while (pending.length > 0) {
      const deleted = pending.pop();
      if (!this.triplets.delete(deleted)) continue;
      for (const user of this.each(TripletQuery.any())) {
        if (user.typeId === deleted || user.from === deleted || user.to === deleted) {
          pending.push(user.index);
        }
      }
    }
  }
}

describe('eachTriplet', () => {
  it('agrees with a triplet store on a thousand triplets', () => {
    const triplets = store();
    const reference = new ReferenceStore();
    const random = new Random(151);
    const root = triplets.createTripletPoint();
    reference.insert(root, root, 0, 0);
    const protectedIds = new Set([root]);
    for (let at = 0; at < 10; at++) {
      const typeId = triplets.createTriplet(root, 0, 0);
      reference.insert(typeId, root, 0, 0);
      protectedIds.add(typeId);
    }
    let creates = 0;
    let updates = 0;
    let deletes = 0;
    for (let step = 0; ; step++) {
      if (step >= 1600 && reference.triplets.size >= 1000) break;
      const ids = reference.ids();
      const choice = step >= 1600 ? 0 : random.next(10);
      if (choice <= 6) {
        const typeId = ids[random.next(ids.length)];
        const from = part(random, ids);
        const to = part(random, ids);
        const index = triplets.createTriplet(typeId, from, to);
        assert.equal(reference.insert(index, typeId, from, to), false);
        creates++;
      } else if (choice <= 8) {
        const index = ids[random.next(ids.length)];
        if (protectedIds.has(index)) continue;
        const typeId = ids[random.next(ids.length)];
        const from = part(random, ids);
        const to = part(random, ids);
        triplets.updateTriplet(index, typeId, from, to);
        reference.insert(index, typeId, from, to);
        updates++;
      } else {
        const index = ids[random.next(ids.length)];
        if (!protectedIds.has(index)) {
          const deleted = triplets.deleteTriplet(index);
          const before = reference.triplets.size;
          reference.delete(index);
          assert.equal(before - reference.triplets.size, deleted.length);
          deletes++;
        }
      }
    }
    assert.ok(creates > 1000 && updates > 100 && deletes > 50, `${creates} ${updates} ${deletes}`);
    assert.ok(reference.triplets.size >= 1000);

    assert.deepEqual(triplets.eachTriplet(TripletQuery.any()), reference.each(TripletQuery.any()));
    const ids = reference.ids();
    for (const id of [...ids, 0]) {
      for (const query of [
        TripletQuery.byType(id),
        TripletQuery.byFrom(id),
        TripletQuery.byTo(id),
        TripletQuery.byIndex(id),
      ]) {
        const expected = reference.each(query);
        assert.deepEqual(triplets.eachTriplet(query), expected, JSON.stringify(query));
        assert.equal(triplets.countTriplets(query), expected.length);
      }
    }
    for (let at = 0; at < 200; at++) {
      const pick = () => part(random, ids);
      const typeId = pick();
      const from = pick();
      const query = new TripletQuery(null, typeId, from, pick());
      assert.deepEqual(triplets.eachTriplet(query), reference.each(query), JSON.stringify(query));
      const some = reference.triplets.get(ids[random.next(ids.length)]);
      for (const derived of [
        new TripletQuery(null, some.typeId, some.from, null),
        new TripletQuery(null, some.typeId, null, some.to),
        new TripletQuery(null, null, some.from, some.to),
        new TripletQuery(null, some.typeId, some.from, some.to),
      ]) {
        assert.deepEqual(
          triplets.eachTriplet(derived),
          reference.each(derived),
          JSON.stringify(derived),
        );
      }
    }
    assertNoOrphans(triplets);

    while (reference.triplets.size > 0) {
      const [index] = reference.ids();
      triplets.deleteTriplet(index);
      reference.delete(index);
      assert.equal(triplets.countTriplets(TripletQuery.any()), reference.triplets.size);
    }
    assert.equal(triplets.allLinks().length, 2, 'only the markers are left');
  });
});
