// The edges of the triplet encoding: doublets it does not know, the store
// below a layer and writes it has to refuse. The cases of
// rust/tests/triplets_edges_tests.rs.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import {
  DoubletRole,
  matchTripletsQuery,
  processTripletsQuery,
  triplet,
  TripletQuery,
  TripletsOverDoublets,
} from '../src/triplets.js';
import { LinkStorage } from '../src/link-storage.js';
import { parseReference } from '../src/triplets-query.js';
import { store } from './helpers/triplets.mjs';

function asksThroughALentStore(triplets) {
  const markers = triplets.tripletMarkers();
  const point = triplets.createTripletPoint();
  triplets.createTripletPointAt(50);
  const created = triplets.createTriplet(point, 50, point);
  assert.equal(triplets.isTriplet(created), true);
  const got = triplets.getTriplet(created);
  assert.deepEqual(got, triplet(created, point, 50, point));
  assert.equal(triplets.roleOf(markers.pair), DoubletRole.Marker);
  assert.deepEqual(triplets.eachTriplet(TripletQuery.byFrom(50)), [got]);
  assert.deepEqual(triplets.updateTriplet(created, point, 0, 0), got);
  assert.equal(triplets.deleteTriplet(created).length, 1);
}

/** Two triplets on an empty store: `(5: 5 0 0)`, `(6: 5 5 5)` and `named`. */
function seeded() {
  const triplets = store();
  processTripletsQuery(triplets, '() ((5: 5 0 0) (6: 5 5 5) (named: 5 0 0))');
  return triplets;
}

describe('the triplet encoding', () => {
  it('stays a store of triplets when it is lent', () => {
    const triplets = store();
    asksThroughALentStore(triplets);
    assert.equal(triplets.isTriplet(50), true);
  });

  it('keeps the doublets reachable under the triplets', () => {
    const links = store();
    const first = links.create(0, 0);
    assert.equal(links.ensureCreated(first + 5), first + 5);
    assert.equal(links.referenceExists(first), true);
    assert.equal(links.claims(first), null);
    assert.equal(links.nextCreatedAddresses(1).length, 1);
    const pair = links.getOrCreate(first, first);
    assert.equal(links.getOrCreate(first, first), pair);
    assert.equal(links.search(first, first), pair);
    assert.equal(links.setName(first, 'named'), first);
    assert.equal(links.getName(first), 'named');
    assert.equal(links.getByName('named'), first);
    links.removeName(first);
    assert.equal(links.getByName('named'), null);
    const made = links.getOrCreateNamed('made');
    assert.equal(links.getOrCreateNamed('made'), made);
    links.save();
    // Writes go to the store below, with what they change.
    const observed = [];
    links.updateObserved(first, first, 0, (before, after) => observed.push([before, after]));
    links.deleteObserved(first, (before, after) => observed.push([before, after]));
    assert.equal(observed.length > 1, true);
    assert.equal(links.exists(first), false);
    assert.equal(links.tryEnsureCreated(first), first);
  });

  it('calls a doublet it does not know foreign', () => {
    const links = store();
    const markers = links.tripletMarkers();
    const kind = links.createTripletPoint();
    const from = links.createTripletPoint();
    const to = links.createTripletPoint();
    const created = links.createTriplet(kind, from, to);
    const duplicate = links.createTriplet(kind, from, to);

    const pair = links.getLink(created).target;
    const wrapper = links.getLink(pair).source;
    assert.equal(links.roleOf(pair), DoubletRole.Pair);
    assert.equal(links.roleOf(wrapper), DoubletRole.Wrapper);
    const duplicatePair = links.getLink(duplicate).target;
    assert.equal(links.roleOf(duplicatePair), DoubletRole.DuplicatePair);

    const ofNothing = links.create(0, kind);
    const ofEmpty = links.create(markers.nothing, kind);
    const ofMissing = links.create(9_999, kind);
    const toMissing = links.create(kind, 9_998);
    const toMarker = links.create(kind, markers.pair);
    const plain = links.create(kind, from);
    const onDuplicate = links.create(duplicatePair, kind);
    const sourceOfMissing = links.create(9_997, kind);
    const onMissing = links.create(sourceOfMissing, kind);
    for (const doublet of [
      ofNothing,
      ofEmpty,
      ofMissing,
      toMissing,
      toMarker,
      plain,
      onDuplicate,
      onMissing,
    ]) {
      assert.equal(links.roleOf(doublet), DoubletRole.Foreign, `doublet ${doublet}`);
      assert.equal(links.getTriplet(doublet), null, `doublet ${doublet}`);
    }
    assert.equal(links.roleOf(9_000), null);
    assert.equal(links.getTriplet(markers.nothing), null);
  });

  it('calls a doublet pointing back through a marker or nothing foreign', () => {
    const links = store();
    const markers = links.tripletMarkers();
    const kind = links.createTripletPoint();
    for (const marker of [markers.nothing, 0]) {
      const doublet = links.create(kind, kind);
      const back = links.create(marker, doublet);
      links.update(doublet, kind, back);
      assert.equal(links.roleOf(doublet), DoubletRole.Foreign, String(marker));
    }
  });

  it('leaves out of a query by type or parts what is not a triplet', () => {
    const links = store();
    const markers = links.tripletMarkers();
    const kind = links.createTripletPoint();
    const from = links.createTripletPoint();
    const first = links.createTriplet(kind, from, from);
    const second = links.createTriplet(kind, from, from);
    const pair = links.getLink(first).target;

    // A type made of a marker holds no triplets, and neither does nothing.
    assert.deepEqual(links.eachTriplet(TripletQuery.byType(markers.pair)), []);
    assert.deepEqual(links.eachTriplet(TripletQuery.byType(0)), []);
    // Doublets of somebody else on the pair and on the type are no triplets.
    links.create(0, pair);
    links.create(pair, 9_996);
    links.create(kind, 9_995);
    const found = links.eachTriplet(new TripletQuery(null, null, from, from));
    assert.deepEqual(
      found.map((value) => value.index),
      [first, second],
    );
    const ofKind = links.eachTriplet(TripletQuery.byType(kind));
    assert.ok(ofKind.every((value) => [first, second, kind].includes(value.index)));
    // Parts nothing was ever made of hold no triplets either.
    assert.deepEqual(links.eachTriplet(TripletQuery.byFrom(9_000)), []);
    assert.deepEqual(links.eachTriplet(new TripletQuery(null, null, 9_000, from)), []);
    assert.deepEqual(links.eachTriplet(new TripletQuery(null, null, from, 9_000)), []);
  });

  it('refuses a write that would merge a triplet away', () => {
    const links = store();
    const kind = links.createTripletPoint();
    const from = links.createTripletPoint();
    const created = links.createTriplet(kind, from, from);
    const before = links.getLink(created);
    // Somebody else's doublet already is what the triplet would become.
    const taken = links.create(kind, created);
    assert.throws(() => links.updateTriplet(created, kind, 0, 0), {
      message: `The doublet (${kind} ${created}) is already ${taken}: triplet ${created} would be merged into it.`,
    });
    assert.deepEqual(links.getLink(created), before);
    assert.throws(() => links.updateTriplet(taken, kind, 0, 0), {
      message: `Link ${taken} is not a triplet.`,
    });
    assert.throws(() => links.deleteTriplet(taken), {
      message: `Link ${taken} is not a triplet.`,
    });
  });

  it('moves an update to a taken pair to a duplicate pair and back', () => {
    const links = store();
    const kind = links.createTripletPoint();
    const from = links.createTripletPoint();
    const to = links.createTripletPoint();
    const first = links.createTriplet(kind, from, to);
    const second = links.createTriplet(kind, to, from);
    links.updateTriplet(second, kind, from, to);
    assert.deepEqual(links.getTriplet(second), triplet(second, kind, from, to));
    // Away from the shared pair and back to it.
    links.updateTriplet(first, kind, from, 0);
    links.updateTriplet(first, kind, from, to);
    assert.deepEqual(links.getTriplet(first), triplet(first, kind, from, to));
  });
});

describe('triplet queries', () => {
  it('refuse a wrong shape by name', () => {
    const SHAPE = 'A triplets query is ((restriction) (substitution)).';
    for (const [text, expected] of [
      ['((1: 2 3 4))', SHAPE],
      ['(x: (1 2 3) (1 2 3))', SHAPE],
      ['(30)', SHAPE],
      ['(1 2 3)\n(1 2 3)\n(1 2 3)', SHAPE],
      ['((1 2)) ()', 'A triplet pattern has three parts, type, from and to: (1 2) has 2.'],
      [
        '((1 2 3 4 5)) ()',
        'A triplet pattern has three parts, type, from and to: (1 2 3 4 5) has 5.',
      ],
      ['((1 2 (3 4))) ()', 'A part of a triplet pattern is a reference, not (3 4).'],
      ['((1 2 (x: 3 4))) ()', 'A part of a triplet pattern is a reference, not ("x": 3 4).'],
      ['() ((99))', 'A triplet pattern has three parts, type, from and to: (99) has 1.'],
      ['() ((0: 5 0 0))', 'A triplet cannot be at address 0.'],
      ['() ((77: unknown 0 0))', 'The type part unknown of a substitution is not a triplet.'],
      ['() ((77: 5 unknown 0))', 'The from part unknown of a substitution is not a triplet.'],
      ['((($i: 5 0 0)) (($i: 1 0 0)))', 'The type part 1 of a substitution is not a triplet.'],
      ['()', SHAPE],
      ['(() x)', 'A triplet pattern has three parts, type, from and to: "x" has 0.'],
      // The markers have names, but are not triplets.
      ['() ((Pair: 5 0 0))', 'Link 1 is not a triplet.'],
      ['() ((Nothing: 5 0 0))', 'Link 2 is not a triplet.'],
    ]) {
      assert.throws(() => processTripletsQuery(seeded(), text), { message: expected }, text);
    }
    for (const [restriction, message] of [
      ['(1 2 3)\n(1 2 3)', 'A restriction is one link of patterns: (1 2 3)\n(1 2 3)'],
      ['((1 2))', 'A triplet pattern has three parts, type, from and to: (1 2) has 2.'],
      ['((1: (2 3)))', 'A triplet pattern has three parts, type, from and to: (1: (2 3)) has 1.'],
    ]) {
      assert.throws(
        () => matchTripletsQuery(seeded(), restriction, '$i'),
        { message },
        restriction,
      );
    }
  });

  it('match a pattern without an index, and replace it by a new triplet', () => {
    const triplets = seeded();
    const [created, deleted] = processTripletsQuery(triplets, '(((5 5 5)) ((5 5 5)))');
    assert.equal(created.before, null);
    assert.deepEqual(created.after, triplet(created.after.index, 5, 5, 5));
    assert.deepEqual(deleted, { before: triplet(6, 5, 5, 5), after: null });
    assert.equal(triplets.isTriplet(6), false);
  });

  it('read references as names, variables, wildcards and addresses', () => {
    assert.deepEqual(parseReference('*'), { kind: 'Any' });
    assert.deepEqual(parseReference('$x'), { kind: 'Variable', name: '$x' });
    assert.deepEqual(parseReference('7'), { kind: 'Address', address: 7 });
    assert.deepEqual(parseReference('"7"'), { kind: 'Name', name: '7' });
    // A name outside a quote envelope, as a caller other than the query parser may pass.
    assert.deepEqual(parseReference('alice'), { kind: 'Name', name: 'alice' });
  });

  it('do nothing for an empty document', () => {
    assert.deepEqual(processTripletsQuery(seeded(), ''), []);
  });

  it('match the values of a variable each once', () => {
    const triplets = seeded();
    assert.deepEqual(matchTripletsQuery(triplets, '(($i: 5 * *))', '$i'), [3, 5, 6]);
    assert.deepEqual(matchTripletsQuery(triplets, '(($i: 5 $f *))', '$f'), [0, 5]);
    assert.deepEqual(matchTripletsQuery(triplets, '(($i: 5 0 0))', '$other'), []);
  });

  it('write a name or variable written twice as one triplet', () => {
    for (const text of [
      '() (($v: 5 0 0) ($v: 5 0 0))',
      '() ((n: 5 0 0) (n: 5 0 0))',
      '((6: 5 5 5)) ((6: 5 5 0) (6: 5 0 0))',
    ]) {
      assert.equal(processTripletsQuery(seeded(), text).length, 1, text);
    }
    // The second pattern wrote the triplet last, and that is the one reported.
    const triplets = seeded();
    const changes = processTripletsQuery(triplets, '((6: 5 5 5)) ((6: 5 5 0) (6: 5 0 0))');
    assert.deepEqual(changes[0].after, triplet(6, 5, 0, 0));
    assert.deepEqual(triplets.getTriplet(6), triplet(6, 5, 0, 0));
  });

  it('let a variable new in the substitution define a triplet others use', () => {
    const triplets = seeded();
    const changes = processTripletsQuery(triplets, '() (($new: 5 0 0) (72: $new 0 0))');
    assert.equal(changes.length, 2);
    const created = changes[0].after.index;
    assert.deepEqual(triplets.getTriplet(72), triplet(72, created, 0, 0));
  });

  it('delete a matched triplet the substitution leaves out', () => {
    const triplets = seeded();
    processTripletsQuery(triplets, '((6: 5 5 5)) ((88: 5 0 0))');
    assert.equal(triplets.isTriplet(6), false);
    assert.equal(triplets.isTriplet(88), true);
  });

  it('take a side of one pattern and sides that are two links', () => {
    // One pattern with an index stands for a list of one.
    assert.equal(processTripletsQuery(seeded(), '((6: * * *) (6: 5 0 0))').length, 1);
    // The two sides may also be separate links of the document.
    assert.equal(processTripletsQuery(seeded(), '((6: * * *))\n((6: 5 0 0))').length, 1);
  });

  it('refuse a part named by nothing', () => {
    assert.throws(() => processTripletsQuery(seeded(), '() ((9: nameless 0 0))'), /nameless/);
  });

  it('refuse to write at a doublet that is not a triplet', () => {
    const triplets = seeded();
    const doublet = triplets.create(0, 0);
    assert.throws(() => processTripletsQuery(triplets, `() ((${doublet}: 5 0 0))`), {
      message: `Link ${doublet} is a doublet, not a triplet.`,
    });
  });
});

/** A store whose names are out of reach: `getByName` fails. */
class NamesDown extends TripletsOverDoublets {
  getByName(_name) {
    throw new Error('names are down');
  }
}

/** A store that does not see the next point it creates as a triplet, once. */
class SlowPoints extends TripletsOverDoublets {
  createTripletPoint() {
    this.unseen = super.createTripletPoint();
    return this.unseen;
  }

  getTriplet(index) {
    if (index !== this.unseen) return super.getTriplet(index);
    this.unseen = null;
    return null;
  }
}

/** A store that answers a triplet at 99, where there is no doublet. */
class Phantom extends TripletsOverDoublets {
  getTriplet(index) {
    return index === 99 ? triplet(99, 3, 0, 0) : super.getTriplet(index);
  }
}

const over = (Store) => Store.withNamedMarkers(LinkStorage.inMemory().withIndexedLookups());

describe('a store that breaks its promises', () => {
  it('answers no name when names fail', () => {
    const triplets = over(NamesDown);
    triplets.createTripletPoint();
    assert.deepEqual(processTripletsQuery(triplets, '((($i: alice * *)) ())'), []);
    assert.deepEqual(matchTripletsQuery(triplets, '(($i: 3 * *))', '$i'), [3]);
  });

  it('takes a reserved point it does not see for a new point', () => {
    const triplets = over(SlowPoints);
    const root = triplets.createTripletPoint();
    triplets.unseen = null;
    const [change] = processTripletsQuery(triplets, `() (($v: ${root} 0 0))`);
    assert.equal(change.before, null);
    assert.deepEqual(change.after, triplet(change.after.index, root, 0, 0));
  });

  it('refuses to update a triplet without its doublet', () => {
    const triplets = over(Phantom);
    triplets.createTripletPoint();
    assert.throws(() => triplets.updateTriplet(99, 3, 3, 0), {
      message: 'Link 99 does not exist.',
    });
  });
});
