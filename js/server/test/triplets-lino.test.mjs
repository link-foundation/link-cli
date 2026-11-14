// Triplets in Links Notation: the Lino cases of
// rust/tests/triplets_over_doublets_tests.rs and the file store of
// rust/tests/triplets_over_doublets_tests/substitution.rs.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { LinkStorage } from '../src/link-storage.js';
import {
  importTripletsLino,
  matchTripletsQuery,
  processTripletsQuery,
  triplet,
  TripletQuery,
  TripletsOverDoublets,
  tripletsToLino,
} from '../src/triplets.js';
import {
  assertNoOrphans,
  change,
  id,
  named,
  namedPeople,
  PEOPLE,
  people,
  store,
} from './helpers/triplets.mjs';

describe('tripletsToLino and importTripletsLino', () => {
  it('keep names and addresses on a round trip', () => {
    const triplets = namedPeople();
    assert.equal(triplets.countTriplets(TripletQuery.any()), 7);
    const r = named(triplets, ['r', 'knows', 'alice', 'bob']);
    assert.deepEqual(triplets.getTriplet(r.index), r);
    assert.equal(tripletsToLino(triplets), PEOPLE);

    // Without names a triplet is its address, and the import keeps it.
    const numbered = store();
    const p = people(numbered);
    numbered.createTriplet(p.knows, p.alice, p.bob);
    const gone = numbered.createTriplet(p.knows, p.alice, p.carol);
    numbered.createTriplet(p.knows, p.alice, p.bob);
    numbered.createTriplet(p.knows, 0, p.carol);
    numbered.deleteTriplet(gone);
    const text = tripletsToLino(numbered);
    const copy = store();
    const imported = importTripletsLino(copy, text);
    assert.equal(imported.length, numbered.countTriplets(TripletQuery.any()));
    assert.deepEqual(
      copy.eachTriplet(TripletQuery.any()),
      numbered.eachTriplet(TripletQuery.any()),
    );
    assert.equal(tripletsToLino(copy), text);
    assertNoOrphans(copy);

    // Lines may come in any order, and a line without an index is new.
    const forward = store();
    const lines = importTripletsLino(
      forward,
      '(r: knows alice bob)\n(knows: Type 0 0)\n(alice: Type 0 0)\n(bob: Type 0 0)\n(Type: Type 0 0)\n(knows bob alice)\n',
    );
    assert.equal(lines.length, 6);
    assert.deepEqual(forward.getTriplet(lines[0]), named(forward, ['r', 'knows', 'alice', 'bob']));
    const last = forward.getTriplet(lines[5]);
    assert.deepEqual(
      [last.typeId, last.from, last.to],
      [id(forward, 'knows'), id(forward, 'bob'), id(forward, 'alice')],
    );
    assertNoOrphans(forward);
  });

  it('writes the numbered round trip as Rust does', () => {
    const numbered = store();
    const p = people(numbered);
    numbered.createTriplet(p.knows, p.alice, p.bob);
    numbered.createTriplet(p.knows, 0, p.carol);
    assert.equal(
      tripletsToLino(numbered),
      '(3: 3 0 0)\n(4: 3 0 0)\n(5: 3 0 0)\n(6: 4 0 0)\n(7: 4 0 0)\n(8: 4 0 0)\n(11: 5 6 7)\n(14: 5 0 8)\n',
    );
    // A document of one line is that line, wrapped once more or not.
    for (const text of ['((x: 3 0 0))', '(x: 3 0 0)']) {
      const [created] = importTripletsLino(numbered, text);
      assert.deepEqual(numbered.getTriplet(created), triplet(created, 3, 0, 0));
    }
    assert.deepEqual(importTripletsLino(numbered, ''), []);
  });

  it('refuses what is not a triplet', () => {
    const triplets = namedPeople();
    const markers = triplets.tripletMarkers();
    const before = tripletsToLino(triplets);
    for (const [text, message] of [
      ['(x: nobody 0 0)', 'The type part nobody of a substitution is not a triplet.'],
      [
        '(x: Type 0)',
        'A triplet pattern has three parts, type, from and to: ("x": "Type" 0) has 2.',
      ],
      ['()', 'A triplet pattern has three parts, type, from and to: () has 0.'],
      ['(x: $t 0 0)', 'A triplet in Links Notation has no variables or wildcards.'],
      ['(x: Type * 0)', 'A triplet in Links Notation has no variables or wildcards.'],
      ['(x: 0 0 0)', 'The type part 0 of a substitution is not a triplet.'],
      [`(${markers.pair}: Type 0 0)`, `Link ${markers.pair} is a doublet, not a triplet.`],
      [
        `(x: Type ${markers.nothing} 0)`,
        `The from part ${markers.nothing} of a substitution is not a triplet.`,
      ],
    ]) {
      assert.throws(() => importTripletsLino(triplets, text), { message }, text);
    }
    assert.equal(tripletsToLino(triplets), before);
  });

  it('keeps quoted references names in imports and queries', () => {
    for (const [reference, name] of [
      ["'0'", '0'],
      ['"1"', '1'],
      ['`9999`', '9999'],
      ["'*'", '*'],
      ["'$item'", '$item'],
      ['~1{30}', '0'],
      [`'"0"'`, '"0"'],
      ["'x\\y'", 'x\\y'],
      ["'a\nb'", 'a\nb'],
      ["'#tag'", '#tag'],
      ["'~value'", '~value'],
      ["'  '", '  '],
      [`"a'b\`c"`, "a'b`c"],
      [`\`a'b"c\``, `a'b"c`],
    ]) {
      const triplets = store();
      const three = `${reference} ${reference} ${reference}`;
      importTripletsLino(triplets, `(Type: Type 0 0)\n(${reference}: Type 0 0)\n(r: ${three})\n`);
      const namedId = id(triplets, name);
      const r = id(triplets, 'r');
      const before = triplet(r, namedId, namedId, namedId);
      assert.deepEqual(triplets.getTriplet(r), before, reference);

      const text = tripletsToLino(triplets);
      const copy = store();
      importTripletsLino(copy, text);
      assert.equal(tripletsToLino(copy), text, reference);
      assert.notEqual(copy.getByName(name), null, reference);

      const created = store();
      processTripletsQuery(created, `() ((Type: Type 0 0) (${reference}: Type 0 0) (r: ${three}))`);
      assert.equal(tripletsToLino(created), text, reference);
      assertNoOrphans(created);

      assert.deepEqual(matchTripletsQuery(triplets, `(($r: ${three}))`, '$r'), [r], reference);
      const changes = processTripletsQuery(
        triplets,
        `((r: ${three})) ((r: ${reference} 0 ${reference}))`,
      );
      assert.deepEqual(changes, [change(before, triplet(r, namedId, 0, namedId))], reference);
      // Quoted names used as substitution indices are looked up too.
      processTripletsQuery(triplets, `() ((${reference}: Type 0 0))`);
      assert.equal(id(triplets, name), namedId);
      assertNoOrphans(triplets);
    }
  });

  it('keeps triplets in a file store across a reopen', () => {
    const directory = mkdtempSync(join(tmpdir(), 'clink-triplets-'));
    try {
      const path = join(directory, 'db.links');
      const written = TripletsOverDoublets.withNamedMarkers(new LinkStorage(path));
      importTripletsLino(written, PEOPLE);
      written.save();
      const text = tripletsToLino(written);
      const reopened = TripletsOverDoublets.withNamedMarkers(new LinkStorage(path));
      assert.equal(tripletsToLino(reopened), text);
      assert.equal(text, PEOPLE);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });
});
