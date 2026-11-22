// Triplets over a store with a type hierarchy: works_with_the_type_hierarchy
// of rust/tests/triplets_over_doublets_tests.rs.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { LinkStorage } from '../src/link-storage.js';
import { triplet, TripletQuery, TripletsOverDoublets } from '../src/triplets.js';
import { TypesDecorator } from '../src/types.js';
import { assertNoOrphans } from './helpers/triplets.mjs';

describe('TripletsOverDoublets over TypesDecorator', () => {
  it('works with the type hierarchy', () => {
    const types = TypesDecorator.withNamedMarkers(LinkStorage.inMemory());
    const triplets = TripletsOverDoublets.withNamedMarkers(types);
    const root = triplets.links().markers().root;
    assert.deepEqual(triplets.getTriplet(root), triplet(root, root, 0, 0));
    const person = triplets.linksMut().defineRoot();
    const pub = triplets.linksMut().defineRoot();
    const admin = triplets.linksMut().define(person, [pub]);
    const boss = triplets.createTriplet(admin, 0, 0);
    assert.equal(triplets.linksMut().isA(boss, person), true);
    assert.equal(triplets.linksMut().isA(boss, pub), true);
    assert.deepEqual(
      triplets
        .eachTriplet(TripletQuery.byType(root))
        .filter((value) => value.index !== root && value.index > pub - 2),
      [triplet(person, root, 0, 0), triplet(pub, root, 0, 0)],
    );
    const knows = triplets.createTriplet(root, 0, 0);
    const link = triplets.createTriplet(knows, boss, admin);
    assert.deepEqual(triplets.eachTriplet(TripletQuery.byTo(admin)), [
      triplet(link, knows, boss, admin),
    ]);
    assertNoOrphans(triplets, true);
  });
});
