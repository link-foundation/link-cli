// Networks joined into one and networks under other addresses (issue #149):
// UnionLinks, RemapLinks and LinksExternalIds. The first cases are the twins
// of rust/tests/union_and_remap_links_tests.rs and of the C#
// UnionAndRemapLinksTests.cs; the rest cover what only JavaScript adds (ids
// that are not addresses, subscriptions through a remap).

import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import test from 'node:test';
import {
  AddressTable,
  ANY,
  ExternalIdMap,
  LinksDecorator,
  LinksExternalIds,
  NotFoundError,
  OffsetAddressMap,
  RemapLinks,
  UnionLinks,
  UnsupportedError,
} from '../src/index.js';
import { all, make, MemoryLinks } from './memory-links.mjs';

function world() {
  const local = new MemoryLinks('local');
  const server = new MemoryLinks('server');
  return { local, server, union: new UnionLinks([local, server]) };
}

test('union reads the links of every member', async () => {
  const { local, server, union } = world();
  await make(server, 1n, 1n);
  await make(server, 2n, 2n);
  await make(server, 2n, 1n);
  const local1 = await make(local, 3n, 3n);
  assert.deepEqual(await all(union), [
    [local1, 3n, 3n],
    [2n, 2n, 2n],
    [3n, 2n, 1n],
  ]);
  assert.equal(await union.count(), 3n);
  assert.equal(await union.count([3n]), 1n);
  assert.deepEqual(await all(union, [ANY, 2n, ANY]), [
    [2n, 2n, 2n],
    [3n, 2n, 1n],
  ]);
  assert.deepEqual(await all(union, [ANY, ANY, 3n]), [[local1, 3n, 3n]]);
  assert.equal(await union.ownerOf(1n), 0);
  assert.equal(await union.ownerOf(2n), 1);
  assert.equal(await union.ownerOf(99n), null);
});

test('union: the first member shadows the rest', async () => {
  const { local, server, union } = world();
  await make(server, 1n, 1n);
  await make(local, 2n, 2n);
  assert.deepEqual(await all(union), [[1n, 2n, 2n]]);
  assert.equal(await union.count(), 1n);
  assert.deepEqual(await all(union, [ANY, 1n, ANY]), []);
});

test('union creates in the primary member and writes where the link lives', async () => {
  const { local, server, union } = world();
  await make(server, 1n, 1n);
  await make(server, 2n, 2n);
  const created = (await union.create(0n, 0n)).index;
  await union.update(created, 2n, 2n);
  assert.deepEqual(await all(local), [[created, 2n, 2n]]);
  await union.update(2n, 2n, created);
  assert.deepEqual(await all(server), [
    [1n, 1n, 1n],
    [2n, 2n, created],
  ]);
  await union.delete(2n);
  assert.deepEqual(await all(server), [[1n, 1n, 1n]]);
  assert.deepEqual(await all(union), [[created, 2n, 2n]]);
});

test('union refuses writes to many links at once', async () => {
  const { union } = world();
  // A write names one link; the null link is none.
  await assert.rejects(union.delete(0n), NotFoundError);
  await assert.rejects(union.update(0n, 2n, 2n), { name: 'NotFoundError', outcome: 'not-sent' });
});

test('union finds names in member order', async () => {
  const { local, server, union } = world();
  await make(server, 1n, 1n);
  await make(server, 2n, 2n);
  const type = await make(local, 1n, 1n);
  await server.setName(2n, 'Note');
  await server.setName(1n, 'Shadowed');
  assert.equal(await union.getByName('Note'), 2n);
  assert.equal(await union.getName(2n), 'Note');
  assert.equal(await union.getByName('Shadowed'), null);
  await local.setName(type, 'Note');
  assert.equal(await union.getByName('Note'), type);
  await union.setName(2n, 'Remote');
  assert.equal(await server.getByName('Remote'), 2n);
  assert.equal(await local.getByName('Remote'), null);
  await union.removeName(2n);
  assert.equal(await server.getName(2n), null);
});

function remapped() {
  const inner = new MemoryLinks();
  return { inner, remap: new RemapLinks(inner, new OffsetAddressMap(1000n)) };
}

test('remap moves every address by an offset', async () => {
  const { inner, remap } = remapped();
  const point = await make(inner, 1n, 1n);
  assert.deepEqual(await all(remap), [[1001n, 1001n, 1001n]]);
  const created = (await remap.create(0n, 0n)).index;
  assert.equal(created, 1002n);
  await remap.update(created, 1001n, created);
  assert.deepEqual(await all(inner), [
    [point, point, point],
    [2n, 1n, 2n],
  ]);
  assert.deepEqual(await all(remap, [ANY, 1001n, 1002n]), [[1002n, 1001n, 1002n]]);
  await remap.delete(1002n);
  assert.deepEqual(await all(inner), [[point, point, point]]);
});

test('remap finds nothing and writes nothing outside its network', async () => {
  const { inner, remap } = remapped();
  const point = await make(inner, 1n, 1n);
  assert.deepEqual(await all(remap, [ANY, 1n, ANY]), []);
  assert.equal(await remap.count([1n]), 0n);
  assert.deepEqual(await all(remap, [ANY, 5n, ANY]), []);
  await assert.rejects(remap.update(1001n, 5n, 1001n), {
    name: 'AddressRangeError',
    message: /not of this network/,
  });
  await assert.rejects(remap.delete(5n), { name: 'NotFoundError', outcome: 'not-sent' });
  assert.deepEqual(await all(inner), [[point, point, point]]);
});

test('remap reports changes under its addresses', async () => {
  const { remap } = remapped();
  const created = await remap.create(0n, 0n);
  assert.deepEqual(created, { index: 1001n, source: 0n, target: 0n });
  const changes = await remap.update(1001n, 1001n, 1001n);
  assert.deepEqual(changes.at(-1), {
    before: { index: 1001n, source: 0n, target: 0n },
    after: { index: 1001n, source: 1001n, target: 1001n },
  });
});

test('remap names under its addresses', async () => {
  const { inner, remap } = remapped();
  await make(inner, 1n, 1n);
  assert.equal(await remap.setName(1001n, 'Type'), 1001n);
  assert.equal(await inner.getByName('Type'), 1n);
  assert.equal(await remap.getByName('Type'), 1001n);
  assert.equal(await remap.getName(1001n), 'Type');
  assert.equal(await remap.getName(1n), null);
  await remap.removeName(1001n);
  assert.equal(await remap.getByName('Type'), null);
});

test('an address table gives addresses in the order they are seen', async () => {
  const table = new AddressTable({ first: 500n });
  table.add(900n, 2n);
  const inner = new MemoryLinks();
  const remap = new RemapLinks(inner, table);
  await make(inner, 1n, 1n);
  await make(inner, 1n, 2n);
  assert.deepEqual(await all(remap), [
    [500n, 500n, 500n],
    [900n, 500n, 900n],
  ]);
  assert.equal((await remap.create(0n, 0n)).index, 501n);
  assert.deepEqual(table.pairs(), [
    [500n, 1n],
    [501n, 3n],
    [900n, 2n],
  ]);
  assert.throws(() => table.add(500n, 7n), /mapped to 1 already/);
  assert.throws(() => table.add(600n, 1n), /mapped from 500 already/);
  assert.throws(() => new AddressTable({ first: 0n }), { name: 'AddressRangeError' });
  assert.throws(() => new AddressTable(), TypeError);
});

test('a union of remapped networks shadows nothing', async () => {
  const local = new MemoryLinks('local');
  const server = new MemoryLinks('server');
  await make(local, 1n, 1n);
  await make(server, 1n, 1n);
  const offset = 1n << 24n;
  const remote = offset + 1n;
  const union = new UnionLinks([local, new RemapLinks(server, new OffsetAddressMap(offset))]);
  assert.deepEqual(await all(union), [
    [1n, 1n, 1n],
    [remote, remote, remote],
  ]);
  const typed = await make(union, remote, 1n);
  assert.deepEqual(await all(local, [ANY, remote, ANY]), [[typed, remote, 1n]]);
  await union.update(remote, remote, remote);
  assert.equal(await union.ownerOf(remote), 1);
  await assert.rejects(union.update(remote, remote, typed), { name: 'AddressRangeError' });
});

async function externalIdsBindIdsOfOtherSystemsToLinks(tableApart) {
  const data = new MemoryLinks('data');
  const table = tableApart ? new MemoryLinks('table') : data;
  const ids = new LinksExternalIds('uuid');
  const first = '0b4a3c1e-8f6d-4d2a-9a51-7c1f2e3d4b5a';
  const second = '7e9d1a2b-3c4d-4e5f-8a9b-0c1d2e3f4a5b';
  const ensure = (id) => ids.ensure(table, tableApart ? data : null, id);
  assert.equal(await ids.find(table, first), null);
  const link = await ensure(first);
  assert.equal(await ensure(first), link);
  assert.equal(await ids.find(table, first), link);
  assert.equal(await ids.idOf(table, link), first);
  assert.deepEqual(await all(data, [link]), [[link, link, link]]);
  await ids.bind(table, second, link);
  assert.equal(await ids.find(table, first), null);
  assert.equal(await ids.idOf(table, link), second);
  const other = await make(data, link, link);
  await ids.bind(table, second, other);
  assert.equal(await ids.idOf(table, link), null);
  assert.equal(await ids.find(table, second), other);
  assert.equal(await ids.unbind(table, second), true);
  assert.equal(await ids.unbind(table, second), false);
  assert.equal(await ids.idOf(table, other), null);
  assert.throws(() => new LinksExternalIds('a:b'), TypeError);
  for (let address = 1n; address <= 6n; address += 1n)
    await ids.bind(table, `n${address}`, address);
  for (let address = 1n; address <= 6n; address += 1n) {
    assert.equal(await ids.find(table, `n${address}`), address);
    assert.equal(await ids.idOf(table, address), `n${address}`);
  }
}

test('external ids bind ids of other systems to links in the data', () =>
  externalIdsBindIdsOfOtherSystemsToLinks(false));

test('external ids bind ids of other systems to links in a table apart', () =>
  externalIdsBindIdsOfOtherSystemsToLinks(true));

// What only JavaScript adds: above a remap, ids that are not addresses.

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

test('a remap reads a network under UUIDs and writes it back under its own addresses', async () => {
  const inner = new MemoryLinks();
  const table = new AddressTable({ newId: randomUUID });
  const remap = new RemapLinks(inner, table);
  const type = (await remap.create(0n, 0n)).index;
  assert.match(type, UUID);
  await remap.update(type, type, type);
  const note = (await remap.create(type, 0n)).index;
  assert.notEqual(note, type);
  assert.deepEqual(await all(inner), [
    [1n, 1n, 1n],
    [2n, 1n, 0n],
  ]);
  assert.deepEqual(await all(remap, [ANY, type, ANY]).then((links) => links.length), 2);
  assert.deepEqual(await all(remap, [note]), [[note, type, 0n]]);
  await remap.setName(type, 'Type');
  assert.equal(await remap.getByName('Type'), type);
  assert.deepEqual(table.pairs().length, 2);
  // An id the table does not know is no link of the network.
  const unknown = randomUUID();
  assert.deepEqual(await all(remap, [unknown]), []);
  await assert.rejects(remap.delete(unknown), NotFoundError);
  await assert.rejects(remap.create(unknown, type), { name: 'AddressRangeError' });
  const deleted = await remap.delete(type);
  assert.deepEqual(new Set(deleted.map((change) => change.before.index)), new Set([type, note]));
});

test('external ids keep the UUIDs of a remap in links, across remaps', async () => {
  const data = new MemoryLinks('data');
  const table = new MemoryLinks('table');
  let counter = 0;
  const newId = () => `00000000-0000-4000-8000-${String((counter += 1)).padStart(12, '0')}`;
  const remap = new RemapLinks(data, new ExternalIdMap(table, 'uuid', { newId }));
  const point = (await remap.create(0n, 0n)).index;
  await remap.update(point, point, point);
  assert.equal(point, '00000000-0000-4000-8000-000000000001');
  assert.equal(await new LinksExternalIds('uuid').find(table, point), 1n);
  // Another remap over the same table reads the same ids.
  const again = new RemapLinks(
    data,
    new ExternalIdMap(table, 'uuid', { newId: () => assert.fail('no new id needed') }),
  );
  assert.deepEqual(await all(again), [[point, point, point]]);
  // The default ids are random UUIDs.
  const random = new RemapLinks(new MemoryLinks(), new ExternalIdMap(new MemoryLinks(), 'uuid'));
  assert.match((await random.create(0n, 0n)).index, UUID);
});

test('composed layers state what they translate and refuse the rest up front', async () => {
  const { local, union } = world();
  assert.equal(union.capabilities.sharding, true);
  assert.equal(union.capabilities.subscriptions, false);
  assert.equal(union.capabilities.backend, 'union(local, server)');
  assert.equal(union.require('sharding'), union);
  assert.throws(() => union.require('subscriptions'), UnsupportedError);
  assert.throws(() => union.require('unknown'), TypeError);
  await assert.rejects(union.query('() ((1 1))'), UnsupportedError);
  await assert.rejects(union.requestText('(count: ())'), UnsupportedError);
  await assert.rejects(union.exportLinks(), UnsupportedError);
  await assert.rejects(union.subscribe([]), UnsupportedError);
  assert.throws(() => union.onNotification(() => {}), UnsupportedError);
  await assert.rejects(union.batch([{ op: 'query', text: '()' }]), UnsupportedError);
  // Triplets belong to one store: forwarded, they would read under its addresses.
  assert.equal(union.capabilities.triplets, false);
  await assert.rejects(union.createTriplet(1, 0, 0), UnsupportedError);
  await assert.rejects(union.eachTriplets(), UnsupportedError);
  assert.throws(() => union.onTripletNotification(() => {}), UnsupportedError);
  await assert.rejects(
    union.batch([{ op: 'createTriplet', type: 1, from: 0, to: 0 }]),
    UnsupportedError,
  );
  const batch = await union.batch([
    { op: 'create', source: 0, target: 0 },
    { op: 'update', index: 1, source: 1, target: 1 },
    { op: 'delete', index: 7 },
    { op: 'create', source: 0, target: 0 },
  ]);
  assert.deepEqual([batch.completed, batch.ok, batch.failed, batch.notSent], [false, 2, 0, 2]);
  assert.equal(batch.results[2].status, 'not-sent');
  assert.ok(batch.firstError instanceof NotFoundError);
  assert.deepEqual(await all(local), [[1n, 1n, 1n]]);

  const remap = new RemapLinks(local, new OffsetAddressMap(10n));
  assert.equal(remap.capabilities.maxAddress, local.capabilities.maxAddress + 10n);
  assert.equal(remap.capabilities.operations.includes('query'), false);
  await assert.rejects(remap.query('() ((1 1))'), UnsupportedError);
  assert.equal(remap.capabilities.triplets, false);
  assert.equal(remap.capabilities.operations.includes('eachTriplets'), false);
  await assert.rejects(remap.createTripletPoint(), UnsupportedError);
  await assert.rejects(remap.eachTriplets(), UnsupportedError);
  await assert.rejects(remap.subscribeTriplets([]), UnsupportedError);
  assert.throws(() => remap.onTripletNotification(() => {}), UnsupportedError);
  await assert.rejects(remap.batch([{ op: 'deleteTriplet', index: 1 }]), UnsupportedError);
  assert.equal((await remap.batch([{ op: 'count', restriction: [11n] }])).results[0].value, 1n);
  await union[Symbol.asyncDispose]();
  assert.equal(local.closed, 1);
});

test('a remap subscribes below and pushes changes under its addresses', async () => {
  const inner = new MemoryLinks();
  const pushed = [];
  inner.subscribe = async (restriction) => {
    pushed.push(restriction);
    return 1;
  };
  let listener;
  inner.onNotification = (next) => {
    listener = next;
    return () => {};
  };
  inner.receiveNotification = async () => ({
    subscription: 1,
    changes: [{ before: null, after: { index: 1n, source: 1n, target: 0n } }],
  });
  const remap = new RemapLinks(inner, new OffsetAddressMap(100n));
  assert.equal(await remap.subscribe([ANY, 101n, ANY]), 1);
  assert.deepEqual(pushed, [[ANY, 1n, ANY]]);
  await assert.rejects(remap.subscribe([5n]), { name: 'AddressRangeError' });
  assert.deepEqual(await remap.receiveNotification(), {
    subscription: 1,
    changes: [{ before: null, after: { index: 101n, source: 101n, target: 0n } }],
  });
  const received = new Promise((resolve) => remap.onNotification(resolve));
  listener({
    subscription: 1,
    changes: [{ before: { index: 2n, source: 1n, target: 2n }, after: null }],
  });
  assert.deepEqual(await received, {
    subscription: 1,
    changes: [{ before: { index: 102n, source: 101n, target: 102n }, after: null }],
  });
});

/** A layer of rules over any links client: here, writes only to links from `owner`. */
class OwnedWrites extends LinksDecorator {
  constructor(links, owner) {
    super(links);
    this.owner = owner;
  }

  async #check(index) {
    const [link] = await this.links.each([index]);
    if (link && link.source !== this.owner)
      throw new UnsupportedError(`forbidden: link ${index} is not of ${this.owner}`);
  }

  async update(index, source, target, options) {
    await this.#check(index);
    return super.update(index, source, target, options);
  }

  async delete(index, options) {
    await this.#check(index);
    return super.delete(index, options);
  }
}

test('a layer of rules over a union restricts every member, under it one member', async () => {
  const { local, server, union } = world();
  const mine = await make(server, 1n, 1n);
  const theirs = await make(server, 2n, 2n);
  await make(local, 9n, 9n); // shadows the server's link 1
  const owner = 9n;
  const over = new OwnedWrites(union, owner);
  // Over the union the rules see every member: the local link and the server's.
  await over.update(1n, 9n, 1n);
  await assert.rejects(over.delete(theirs), /forbidden/);
  assert.deepEqual(await all(server), [
    [mine, 1n, 1n],
    [theirs, 2n, 2n],
  ]);
  // Under the union, only the server's links are restricted.
  const under = new UnionLinks([local, new OwnedWrites(server, owner)]);
  await under.update(1n, 1n, 1n);
  await assert.rejects(under.delete(theirs), /forbidden/);
  await server.update(theirs, owner, 2n);
  await under.delete(theirs);
  assert.deepEqual(await all(under), [[1n, 1n, 1n]]);
});
