// The links map with its exact-pair, source and target indexes.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { IndexedLinks } from '../src/indexed-links.js';

const link = (index, source, target) => ({ index, source, target });
const indexes = (values) => values.map((value) => value.index);

function filled() {
  const links = new IndexedLinks();
  for (const value of [link(4, 1, 2), link(1, 1, 1), link(3, 2, 1), link(2, 1, 2)]) {
    links.insert(value);
  }
  return links;
}

describe('indexed links', () => {
  it('keeps the links by address', () => {
    const links = filled();
    assert.equal(links.size, 4);
    assert.equal(links.has(3), true);
    assert.deepEqual(links.get(3), link(3, 2, 1));
    assert.equal(links.get(9), undefined);
    assert.deepEqual(indexes(links.values()), [1, 2, 3, 4]);
    assert.equal(links.values(), links.values());
  });

  it('answers patterns from its indexes', () => {
    const links = filled();
    assert.equal(links.search(1, 2), 2);
    assert.equal(links.search(2, 2), null);
    assert.deepEqual(indexes(links.query()), [1, 2, 3, 4]);
    assert.deepEqual(indexes(links.query(null, 1, 2)), [2, 4]);
    assert.deepEqual(indexes(links.query(null, 1)), [1, 2, 4]);
    assert.deepEqual(indexes(links.query(null, null, 1)), [1, 3]);
    assert.deepEqual(indexes(links.query(null, 9)), []);
    assert.deepEqual(indexes(links.query(3)), [3]);
    assert.deepEqual(indexes(links.query(3, 2, 1)), [3]);
    assert.deepEqual(indexes(links.query(3, 1)), []);
    assert.deepEqual(indexes(links.query(3, null, 2)), []);
    assert.deepEqual(indexes(links.query(9)), []);
  });

  it('follows replacements and removals', () => {
    const links = filled();
    assert.deepEqual(links.insert(link(2, 2, 2)), link(2, 1, 2));
    assert.equal(links.insert(link(5, 1, 2)), undefined);
    assert.equal(links.search(1, 2), 4);
    assert.deepEqual(links.remove(4), link(4, 1, 2));
    assert.equal(links.remove(4), undefined);
    assert.equal(links.search(1, 2), 5);
    assert.deepEqual(indexes(links.query(null, 2)), [2, 3]);
    assert.deepEqual(indexes(links.values()), [1, 2, 3, 5]);
  });

  it('copies independently', () => {
    const links = filled();
    const copy = links.copy();
    copy.remove(1);
    assert.deepEqual(indexes(copy.values()), [2, 3, 4]);
    assert.deepEqual(indexes(links.values()), [1, 2, 3, 4]);
    assert.deepEqual(indexes(copy.query(null, null, 2)), [2, 4]);
  });
});
