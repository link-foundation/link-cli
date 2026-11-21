// LinksUndoLog and UndoableLinksDecorator: the cases of
// rust/tests/undoable_links_edges_tests.rs, over the in-memory store.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { link } from '../src/link.js';
import { LinkStorage } from '../src/link-storage.js';
import {
  keepNamesBeforeUpdate,
  LinksUndoLog,
  UndoableLinksDecorator,
  usagesOf,
} from '../src/undoable-links.js';

const store = () => LinkStorage.inMemory();
const parts = (value) => [value.source, value.target];

describe('LinksUndoLog', () => {
  it('is empty until it keeps a change or a name', () => {
    const links = store();
    const id = links.create(0, 0);
    const log = new LinksUndoLog();
    assert.equal(log.isEmpty(), true);
    log.keepName(links, id);
    assert.equal(log.isEmpty(), false);
    log.clear();
    assert.equal(log.isEmpty(), true);
    log.record(link(1, 2, 3), link(1, 2, 3));
    assert.equal(log.isEmpty(), true, 'a change that changes nothing is not kept');
    log.record(null, null);
    assert.equal(log.isEmpty(), false);
    assert.deepEqual(log.changes(), [[null, null]]);
    assert.deepEqual(log.undo(links), []);
    assert.equal(log.isEmpty(), true);
  });

  it('reads the null link an observer reports as no link', () => {
    const log = new LinksUndoLog();
    log.recordObserved(link(0, 0, 0), link(4, 1, 1));
    log.recordObserved(link(4, 1, 1), link(0, 0, 0));
    assert.deepEqual(log.changes(), [
      [null, link(4, 1, 1)],
      [link(4, 1, 1), null],
    ]);
  });

  it('tolerates links that are already gone and restores empty links', () => {
    const links = store();
    const log = new LinksUndoLog();
    log.record(null, link(40, 1, 1));
    log.record(link(41, 1, 1), link(41, 2, 2));
    log.record(link(42, 0, 0), null);
    assert.deepEqual(log.undo(links), [40]);
    assert.equal(links.exists(40), false);
    assert.equal(links.exists(41), false);
    assert.deepEqual(parts(links.getLink(42)), [0, 0]);

    // A deleted link that exists again gets its parts back, at the same address.
    const again = links.create(7, 7);
    const second = new LinksUndoLog();
    second.record(link(again, 8, 9), null);
    second.undo(links);
    assert.deepEqual(parts(links.getLink(again)), [8, 9]);
  });

  it('undoes a creation, an update and a deletion made on the store', () => {
    const links = store();
    const kept = links.create(1, 1);
    const doomed = links.create(2, 2);
    const log = new LinksUndoLog();
    const made = links.create(3, 3);
    log.record(null, links.getLink(made));
    log.record(links.getLink(kept), link(kept, 5, 5));
    links.update(kept, 5, 5);
    log.record(links.getLink(doomed), null);
    links.delete(doomed);
    assert.deepEqual(log.undo(links), [made]);
    assert.equal(links.exists(made), false);
    assert.deepEqual(parts(links.getLink(kept)), [1, 1]);
    assert.deepEqual(parts(links.getLink(doomed)), [2, 2]);
  });

  it('takes names given by writes and gives back the old ones', () => {
    const links = store();
    const [plain, called, moved] = [links.create(0, 0), links.create(0, 0), links.create(0, 0)];
    links.setName(called, 'old');
    links.setName(moved, 'kept');
    const log = new LinksUndoLog();
    log.keepName(links, plain);
    log.keepName(links, called);
    log.keepName(links, moved);
    log.keepName(links, moved);
    links.setName(plain, 'given');
    links.setName(called, 'new');
    log.undo(links);
    assert.equal(links.getName(plain), null);
    assert.equal(links.getName(called), 'old');
    assert.equal(links.getName(moved), 'kept');
    assert.equal(links.getByName('given'), null);
  });

  it('drops the name of a link that is gone', () => {
    const links = store();
    const id = links.create(0, 0);
    links.setName(id, 'gone');
    const log = new LinksUndoLog();
    log.keepName(links, id);
    links.removeName(id);
    links.delete(id);
    log.undo(links);
    assert.equal(links.exists(id), false);
    assert.equal(links.getName(id), null);
  });

  it('keeps a name holder only when someone holds it', () => {
    const links = store();
    const id = links.create(0, 0);
    const log = new LinksUndoLog();
    log.keepHolderOf(links, 'nobody');
    assert.equal(log.isEmpty(), true);
    links.setName(id, 'held');
    log.keepHolderOf(links, 'held');
    assert.equal(log.isEmpty(), false);
  });

  it('finds usages through parts and stops at cycles', () => {
    const links = store();
    const base = links.create(0, 0);
    const middle = links.create(base, 0);
    const top = links.create(0, middle);
    links.update(base, top, 0);
    const usages = usagesOf(links, base);
    assert.equal(usages.shift(), base);
    assert.deepEqual(
      usages.sort((left, right) => left - right),
      [middle, top],
    );
  });

  it('keeps the names of the usages of an update that merges', () => {
    const links = store();
    const equal = links.create(5, 6);
    const moved = links.create(5, 7);
    const usage = links.create(moved, 0);
    links.setName(usage, 'usage');

    const plain = new LinksUndoLog();
    keepNamesBeforeUpdate(plain, links, moved, 8, 9);
    links.removeName(usage);
    plain.undo(links);
    assert.equal(links.getName(usage), null, 'usage was not kept');

    links.setName(usage, 'usage');
    const merging = new LinksUndoLog();
    keepNamesBeforeUpdate(merging, links, moved, 5, 6);
    links.removeName(usage);
    merging.undo(links);
    assert.equal(links.getName(usage), 'usage');
    assert.equal(links.exists(equal), true);

    // An update onto itself is not a merge.
    const itself = new LinksUndoLog();
    keepNamesBeforeUpdate(itself, links, equal, 5, 6);
    assert.equal(itself.isEmpty(), false);
  });
});

describe('UndoableLinksDecorator', () => {
  it('forwards what it does not keep', () => {
    const links = store();
    const undo = new UndoableLinksDecorator(links);
    assert.equal(undo.recording(), false);
    assert.equal(undo.queryLimits(), null);
    const reserved = new Set([7]);
    const reserving = Object.create(links, { reserveQueryAllocations: { value: () => reserved } });
    assert.equal(new UndoableLinksDecorator(reserving).reserveQueryAllocations(), reserved);
    const id = undo.create(0, 0);
    assert.equal(undo.referenceExists(id), true);
    assert.equal(undo.getLink(id).index, id);
    assert.deepEqual(undo.nextCreatedAddresses(1), [id + 1]);
    assert.equal(undo.getByName('nobody'), null);
    assert.equal(undo.log().isEmpty(), true);
    assert.equal(undo.inner(), links);
    assert.deepEqual(undo.allLinks(), [link(id, 0, 0)]);
    assert.deepEqual(undo.queryLinks(id), [link(id, 0, 0)]);
    assert.deepEqual(undo.queryLinks(), [link(id, 0, 0)]);
    assert.equal(undo.search(0, 0), id);
  });

  it('keeps no write outside a request', () => {
    const undo = new UndoableLinksDecorator(store());
    const id = undo.create(0, 0);
    undo.update(id, 1, 1);
    undo.setName(id, 'free');
    undo.removeName(id);
    undo.delete(id);
    assert.equal(undo.log().isEmpty(), true);
    assert.deepEqual(undo.rollback(), []);
    assert.equal(undo.exists(id), false);
  });

  it('keeps and undoes every way of creating', () => {
    const undo = new UndoableLinksDecorator(store());
    const existing = undo.create(1, 1);
    undo.begin();
    assert.equal(undo.recording(), true);
    assert.equal(undo.ensureCreated(0), 0);
    assert.equal(undo.ensureCreated(existing), existing);
    assert.equal(undo.tryEnsureCreated(existing), existing);
    assert.equal(undo.log().isEmpty(), true, 'nothing was created yet');
    assert.throws(() => undo.tryEnsureCreated(0), {
      message: 'Invalid link format: Cannot ensure unsupported link address 0',
    });
    const ensured = undo.ensureCreated(10);
    const tried = undo.tryEnsureCreated(11);
    const made = undo.tryCreate(2, 2);
    const got = undo.getOrCreate(3, 3);
    assert.equal(undo.getOrCreate(3, 3), got);
    const created = undo.rollback();
    assert.equal(undo.recording(), false);
    for (const id of [ensured, tried, made, got]) {
      assert.ok(created.includes(id), `${id} in ${created}`);
      assert.equal(undo.exists(id), false);
    }
    assert.equal(undo.exists(existing), true);
  });

  it('commits the writes and forgets how to undo them', () => {
    const undo = new UndoableLinksDecorator(store());
    undo.begin();
    const id = undo.create(1, 1);
    undo.setName(id, 'stays');
    undo.commit();
    assert.equal(undo.log().isEmpty(), true);
    assert.deepEqual(undo.rollback(), []);
    assert.equal(undo.getName(id), 'stays');
    undo.save();
  });

  it('undoes an update, a delete and a stolen name', () => {
    const undo = new UndoableLinksDecorator(store());
    const [kept, doomed, holder] = [undo.create(1, 1), undo.create(2, 2), undo.create(3, 3)];
    undo.setName(doomed, 'doomed');
    undo.setName(holder, 'shared');
    undo.begin();
    const seen = [];
    undo.updateObserved(kept, 9, 9, (before, after) => seen.push([before, after]));
    undo.deleteObserved(doomed, (before, after) => seen.push([before, after]));
    undo.removeName(holder);
    undo.setName(kept, 'shared');
    assert.deepEqual(seen, [
      [link(kept, 1, 1), link(kept, 9, 9)],
      [link(doomed, 2, 2), link(doomed, 0, 0)],
      [link(doomed, 0, 0), link(0, 0, 0)],
    ]);
    undo.rollback();
    assert.deepEqual(parts(undo.getLink(kept)), [1, 1]);
    assert.equal(undo.getName(doomed), 'doomed');
    assert.equal(undo.getName(kept), null);
    assert.equal(undo.getByName('shared'), holder);
  });

  it('keeps a creation the store below cannot show as an empty link', () => {
    const links = store();
    const blind = Object.create(links, {
      getLink: { value: () => null },
      create: { value: (source, target) => links.create(source, target) },
    });
    const undo = new UndoableLinksDecorator(blind);
    undo.begin();
    const id = undo.create(4, 4);
    assert.deepEqual(undo.log().changes(), [[null, link(id, 0, 0)]]);
  });
});
