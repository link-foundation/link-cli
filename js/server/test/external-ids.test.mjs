// The ids other systems give our links: the cases of
// rust/tests/input_edges_tests.rs and rust/tests/union_and_remap_links_tests.rs
// on `LinksExternalIds`, with the table in the data and apart from it.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import { EXTERNAL_ID_NAME, LinksExternalIds } from '../src/external-ids.js';
import { LinkStorage } from '../src/link-storage.js';

const store = () => LinkStorage.inMemory();
const triple = (link) => [link.index, link.source, link.target];

describe('LinksExternalIds', () => {
  it('refuses what it cannot name', () => {
    assert.throws(() => new LinksExternalIds(''), {
      message: 'A scheme is not empty and has no colon: ""',
    });
    assert.throws(() => new LinksExternalIds('a:b'), {
      message: 'A scheme is not empty and has no colon: "a:b"',
    });
    const ids = new LinksExternalIds('uuid');
    assert.equal(ids.scheme(), 'uuid');
    assert.throws(() => ids.nameOf(''), { message: 'An id is not empty.' });
    assert.equal(ids.nameOf('x'), 'uuid:x');
    assert.throws(() => ids.bind(store(), 'x', 0), { message: 'The null link has no id.' });
    assert.equal(EXTERNAL_ID_NAME, 'ExternalId');
  });

  it('answers only for the scheme the table holds', () => {
    const storage = store();
    const ids = new LinksExternalIds('uuid');
    const link = storage.tryCreate(1, 1);
    assert.equal(ids.idOf(storage, link), null);
    ids.bind(storage, 'x', link);
    assert.equal(ids.idOf(storage, 0), null);
    const other = storage.tryCreate(2, 2);
    assert.equal(ids.idOf(storage, other), null);
    // Another scheme of the same table has no link yet.
    assert.equal(new LinksExternalIds('url').idOf(storage, link), null);
    // A link that only carries the name of a scheme is not that scheme.
    const isbn = new LinksExternalIds('isbn');
    storage.setName(other, 'isbn');
    assert.equal(isbn.idOf(storage, link), null);
    assert.equal(isbn.find(storage, 'x'), null);
    // A pair named as an id of the scheme, but not starting at the scheme.
    storage.setName(other, 'uuid:y');
    assert.equal(ids.find(storage, 'y'), null);
    // A pair of the scheme whose name is not of the scheme.
    const point = ids.schemeLink(storage);
    const stray = storage.tryCreate(point, other);
    assert.equal(ids.idOf(storage, other), null);
    storage.setName(stray, 'url:z');
    assert.equal(ids.idOf(storage, other), null);
  });

  for (const apart of [false, true]) {
    it(`binds ids of other systems to links, the table ${apart ? 'apart' : 'in the data'}`, () => {
      const data = store();
      const table = apart ? store() : data;
      const ids = new LinksExternalIds('uuid');
      const FIRST = '0b4a3c1e-8f6d-4d2a-9a51-7c1f2e3d4b5a';
      const SECOND = '7e9d1a2b-3c4d-4e5f-8a9b-0c1d2e3f4a5b';
      const ensure = (id) => ids.ensure(table, apart ? data : null, id);
      assert.equal(ids.find(table, FIRST), null);
      const link = ensure(FIRST);
      assert.equal(ensure(FIRST), link);
      assert.equal(ids.find(table, FIRST), link);
      assert.equal(ids.idOf(table, link), FIRST);
      assert.deepEqual(triple(data.getLink(link)), [link, link, link]);
      ids.bind(table, SECOND, link);
      assert.equal(ids.find(table, FIRST), null);
      assert.equal(ids.idOf(table, link), SECOND);
      const other = data.tryCreate(link, link);
      ids.bind(table, SECOND, other);
      assert.equal(ids.idOf(table, link), null);
      assert.equal(ids.find(table, SECOND), other);
      assert.ok(ids.unbind(table, SECOND));
      assert.ok(!ids.unbind(table, SECOND));
      assert.equal(ids.idOf(table, other), null);
      for (let address = 1; address <= 6; address += 1) {
        data.ensureCreated(address);
        ids.bind(table, `n${address}`, address);
      }
      for (let address = 1; address <= 6; address += 1) {
        assert.equal(ids.find(table, `n${address}`), address);
        assert.equal(ids.idOf(table, address), `n${address}`);
      }
    });
  }
});
