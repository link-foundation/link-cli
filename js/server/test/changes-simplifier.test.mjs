// Tests of `simplifyChanges` (src/changes-simplifier.js), the port of
// rust/src/changes_simplifier.rs, with the cases of
// rust/tests/changes_simplifier_tests.rs and the steps the stores report.
import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { simplifyChanges } from '../src/changes-simplifier.js';
import { link, NULL_LINK } from '../src/link.js';

describe('simplifyChanges', () => {
  it('reports nothing for no changes and a matched link unchanged', () => {
    assert.deepEqual(simplifyChanges([]), []);
    const matched = link(1, 2, 3);
    assert.deepEqual(simplifyChanges([[matched, matched]]), [[matched, matched]]);
  });

  it('folds the steps of one address into its first before and last after', () => {
    // A creation is reported as `(0: 0 0) -> (i: 0 0)`, then `(i: 0 0) -> (i: s t)`.
    const changes = [
      [NULL_LINK, link(1, 0, 0)],
      [link(1, 0, 0), link(1, 1, 1)],
      [link(2, 2, 1), link(2, 1, 2)],
    ];
    assert.deepEqual(simplifyChanges(changes), [
      [NULL_LINK, link(1, 1, 1)],
      [link(2, 2, 1), link(2, 1, 2)],
    ]);
  });

  it('skips steps between null links, which belong to no address', () => {
    const changes = [
      [NULL_LINK, NULL_LINK],
      [NULL_LINK, link(3, 0, 0)],
      [NULL_LINK, NULL_LINK],
      [link(3, 0, 0), link(3, 3, 3)],
    ];
    assert.deepEqual(simplifyChanges(changes), [[NULL_LINK, link(3, 3, 3)]]);
    assert.deepEqual(simplifyChanges([[NULL_LINK, NULL_LINK]]), []);
  });

  it('drops a link created and deleted within the query', () => {
    const changes = [
      [NULL_LINK, link(4, 4, 4)],
      [link(4, 4, 4), link(4, 0, 0)],
      [link(4, 0, 0), NULL_LINK],
    ];
    assert.deepEqual(simplifyChanges(changes), []);
  });

  it('orders by the after state, deletions first, keeping the order of equal ones', () => {
    const changes = [
      [link(2, 2, 2), link(2, 1, 2)],
      [link(5, 5, 5), NULL_LINK],
      [link(1, 1, 2), link(1, 2, 1)],
      [link(6, 6, 6), NULL_LINK],
    ];
    assert.deepEqual(simplifyChanges(changes), [
      [link(5, 5, 5), NULL_LINK],
      [link(6, 6, 6), NULL_LINK],
      [link(1, 1, 2), link(1, 2, 1)],
      [link(2, 2, 2), link(2, 1, 2)],
    ]);
  });
});
