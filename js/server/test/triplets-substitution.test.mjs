// Substitution queries over triplets: the cases of
// rust/tests/triplets_over_doublets_tests/substitution.rs.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { processTripletsQuery, TripletQuery, tripletsToLino } from '../src/triplets.js';
import { change, id, named, namedPeople, query } from './helpers/triplets.mjs';

describe('processTripletsQuery', () => {
  it('updates the to-part in place', () => {
    const triplets = namedPeople();
    const before = named(triplets, ['r', 'knows', 'alice', 'bob']);
    const after = named(triplets, ['r', 'knows', 'alice', 'carol']);
    // The query of the issue: ((i: T f t)) ((i: T f t2)), by address.
    const text = `(((${before.index}: ${before.typeId} ${before.from} ${before.to})) ((${after.index}: ${after.typeId} ${after.from} ${after.to})))`;
    assert.deepEqual(query(triplets, text), [change(before, after)]);
    assert.deepEqual(triplets.getTriplet(before.index), after);
    // And by name, with a variable.
    const changes = query(triplets, '((($r: knows alice carol)) (($r: knows alice bob)))');
    assert.deepEqual(changes[0].after, before);
  });

  it('reads, creates and deletes', () => {
    const triplets = namedPeople();
    const r = named(triplets, ['r', 'knows', 'alice', 'bob']);
    // A substitution that repeats the restriction only reads.
    assert.deepEqual(query(triplets, '((($i: knows $f $t)) (($i: knows $f $t)))'), [change(r, r)]);
    assert.equal(query(triplets, '((($p: Person 0 0)) (($p: Person 0 0)))').length, 3);

    // A create for every match, with the variable of the match.
    const created = query(triplets, '((($p: Person 0 0)) (($p: Person 0 0) (knows $p bob)))');
    assert.equal(created.filter((each) => each.before == null).length, 3);
    const bob = id(triplets, 'bob');
    assert.equal(triplets.countTriplets(TripletQuery.byTo(bob)), 4);

    // No restriction: a create; a new name is a new triplet.
    const changes = query(triplets, '(() ((likes: Type 0 0) (likes carol alice)))');
    assert.equal(changes.length, 2);
    assert.ok(changes.every((each) => each.before == null));
    const likes = id(triplets, 'likes');
    assert.equal(triplets.countTriplets(TripletQuery.byType(likes)), 1);

    // No substitution: a delete, with whatever used the deleted triplet.
    const deleted = query(triplets, '((($i: knows $p bob)) ())');
    assert.equal(deleted.length, 4);
    assert.ok(deleted.every((each) => each.after == null));
    assert.equal(triplets.countTriplets(TripletQuery.byTo(bob)), 0);
    assert.equal(triplets.isTriplet(r.index), false);
  });

  it('changes the type part', () => {
    const triplets = namedPeople();
    query(triplets, '(() ((likes: Type 0 0)))');
    let changes = query(triplets, '((($i: knows $f $t)) (($i: likes $f $t)))');
    assert.deepEqual(changes[0].after, named(triplets, ['r', 'likes', 'alice', 'bob']));
    // `*` keeps a part, and the parts may swap.
    changes = query(triplets, '((($i: likes $f $t)) (($i: * $t $f)))');
    assert.deepEqual(changes[0].after, named(triplets, ['r', 'likes', 'bob', 'alice']));
    // A conjunction: who is liked by someone of type Person.
    changes = query(
      triplets,
      '((($i: likes $f $t) ($f: Person 0 0)) (($i: likes $f $t) ($f: Person 0 0)))',
    );
    assert.equal(changes.length, 2);
  });

  it('refuses unknown references', () => {
    const triplets = namedPeople();
    const before = tripletsToLino(triplets);
    for (const [text, message] of [
      ['(() ((nobody alice bob)))', 'The type part nobody of a substitution is not a triplet.'],
      ['(() ((knows $x bob)))', 'The from part $x of a substitution is not a triplet.'],
      ['(() ((knows * bob)))', 'The from part * of a substitution is not a triplet.'],
      ['(() ((*: knows alice bob)))', 'A substitution cannot create or update the index *.'],
      [
        '(() ((knows alice)))',
        'A triplet pattern has three parts, type, from and to: ("knows" "alice") has 2.',
      ],
      ['(() ((0 alice bob)))', 'The type part 0 of a substitution is not a triplet.'],
      [
        '((($i: knows $f $t)) (($i: knows $f $nobody)))',
        'The to part $nobody of a substitution is not a triplet.',
      ],
    ]) {
      assert.throws(() => processTripletsQuery(triplets, text), { message }, text);
    }
    assert.equal(tripletsToLino(triplets), before);
    // A restriction by an unknown name matches nothing.
    assert.deepEqual(query(triplets, '((($i: nobody $f $t)) ())'), []);
  });
});
