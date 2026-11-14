// The doublets unit store of unit-store*.js: the memory a store lives in, the
// store itself with its trees and free list (checked against a plain model of
// it), and the resolution decorators. The bytes it writes are checked against
// the Rust store by unit-store-golden.test.mjs and the differential tests.
import assert from 'node:assert/strict';
import { closeSync, openSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, it } from 'node:test';

import { AddressOverflowError, NotFoundError } from '../src/errors.js';
import { DEFAULT_PAGE_SIZE, RESERVE_STEP, UnitStore } from '../src/unit-store.js';
import { DoubletsError, part } from '../src/unit-store-doublets.js';
import {
  AddressType,
  checkAddress,
  Field,
  FileUnitMemory,
  fitsAddress,
  Header,
  HeapUnitMemory,
  MIN_FILE_SIZE,
  PAGE_SIZE,
} from '../src/unit-store-memory.js';
import {
  CascadeUniquenessAndUsagesResolver,
  CascadeUsagesResolver,
  NonNullContentsLinkDeletionResolver,
  withAutomaticUniquenessAndUsagesResolution,
} from '../src/unit-store-resolution.js';
import { scratchDirectory } from './helpers/storage.mjs';
import { seededRandom } from './helpers/unit-store-trace.mjs';

const { U32, U64 } = AddressType;
const parts = (value) => [value.index, value.source, value.target];
const rows = (links, query = []) => links.eachIter(query).map(parts);

/** A heap store holding `pairs`, created in order. */
function storeOf(pairs, type = U64) {
  const store = UnitStore.inMemory({ addressType: type });
  for (const [source, target] of pairs) store.createLink(source, target);
  return store;
}

/** Every `(before, after)` a write reports. */
function recorder() {
  const changes = [];
  const handler = (before, after) => {
    changes.push([parts(before), parts(after)]);
  };
  return { changes, handler };
}

describe('address types', () => {
  it('describe the usize and u32 references', () => {
    assert.equal(U64.width, 8);
    assert.equal(U64.max, Number.MAX_SAFE_INTEGER);
    assert.equal(U64.any, 2 ** 64);
    assert.equal(U32.width, 4);
    assert.equal(U32.any, 0xfffffffc);
    assert.equal(U32.maxInner, 0xfffffff9);
  });

  it('check addresses', () => {
    assert.equal(fitsAddress(U32, 0xffffffff), true);
    assert.equal(fitsAddress(U32, 2 ** 32), false);
    assert.equal(fitsAddress(U64, -1), false);
    assert.equal(fitsAddress(U64, 1.5), false);
    assert.equal(fitsAddress(U64, 2 ** 53), false);
    assert.equal(checkAddress(U32, 7), 7);
    assert.throws(() => checkAddress(U32, 2 ** 32), AddressOverflowError);
  });
});

describe('the file memory', () => {
  it('pads a short file to a page and covers the items it grows to', () => {
    const path = join(scratchDirectory(), 'db.links');
    const memory = FileUnitMemory.open(path, U32);
    assert.equal(statSync(path).size, MIN_FILE_SIZE);
    assert.equal(memory.capacity, 0);
    memory.grow(200);
    assert.equal(memory.capacity, 200);
    assert.equal(statSync(path).size, 200 * 32);
    memory.grow(100);
    assert.equal(memory.capacity, 200);
    memory.set(150, Field.Target, 0xffffffff);
    memory.commit();
    assert.equal(readFileSync(path).readUInt32LE(150 * 32 + 4), 0xffffffff);
    memory.close();
    memory.close();
  });

  it('zeroes a trailing partial item the file held before growing', () => {
    const path = join(scratchDirectory(), 'db.links');
    writeFileSync(path, Buffer.alloc(100, 0xff));
    const memory = FileUnitMemory.open(path, U64);
    memory.grow(4);
    memory.close();
    const bytes = readFileSync(path);
    assert.equal(bytes.length, MIN_FILE_SIZE);
    assert.deepEqual([...bytes.subarray(0, 64)], new Array(64).fill(0xff));
    assert.equal(
      bytes.subarray(64).every((byte) => byte === 0),
      true,
    );
  });

  it('reads u64 fields up to 2^53 and reports larger ones', () => {
    const path = join(scratchDirectory(), 'db.links');
    const bytes = Buffer.alloc(MIN_FILE_SIZE);
    bytes.writeBigUInt64LE(2n ** 53n - 1n, 64);
    bytes.writeBigUInt64LE(2n ** 53n, 72);
    writeFileSync(path, bytes);
    const memory = FileUnitMemory.open(path, U64);
    assert.equal(memory.get(1, Field.Source), Number.MAX_SAFE_INTEGER);
    assert.throws(() => memory.get(1, Field.Target), {
      name: 'AddressOverflowError',
      message: `Address ${2n ** 53n} does not fit into the configured link address type`,
    });
    memory.set(2, Field.Source, Number.MAX_SAFE_INTEGER);
    assert.equal(memory.get(2, Field.Source), Number.MAX_SAFE_INTEGER);
    memory.close();
  });

  it('writes a range that spans pages and drops its cache once it grows large', () => {
    const path = join(scratchDirectory(), 'db.links');
    const memory = FileUnitMemory.open(path, U64);
    memory.grow(2);
    memory.zero(PAGE_SIZE - 8, PAGE_SIZE + 8);
    for (let page = 0; page <= 300; page++) memory.get((page * PAGE_SIZE) / 64, Field.Source);
    assert.equal(memory.pages.size, 301);
    memory.commit();
    assert.equal(memory.pages.size, 0);
    memory.close();
  });

  it('sees what another writer changed after a reload', () => {
    const path = join(scratchDirectory(), 'db.links');
    const memory = FileUnitMemory.open(path, U32);
    memory.grow(4);
    assert.equal(memory.get(1, Field.Source), 0);
    const bytes = readFileSync(path);
    bytes.writeUInt32LE(9, 32);
    writeFileSync(path, bytes);
    assert.equal(memory.get(1, Field.Source), 0);
    memory.reload();
    assert.equal(memory.get(1, Field.Source), 9);
    memory.sync();
    memory.close();
  });

  it(
    'closes the file when it cannot be extended',
    { skip: process.platform !== 'linux' && 'needs a /dev/null that cannot be truncated' },
    () => {
      assert.throws(() => FileUnitMemory.open('/dev/null', U64), { code: 'EINVAL' });
    },
  );

  it('keeps heap items sparsely', () => {
    const memory = new HeapUnitMemory(U32);
    memory.grow(10);
    memory.grow(5);
    assert.equal(memory.capacity, 10);
    assert.equal(memory.get(3, Field.Source), 0);
    memory.set(3, Field.Source, 4);
    memory.set(3, Field.Target, 5);
    assert.equal(memory.get(3, Field.Target), 5);
    assert.equal(memory.items.size, 1);
    memory.commit();
    memory.sync();
    memory.reload();
    memory.close();
  });
});

describe('the unit store', () => {
  it('opens a file the way the Rust store maps it', () => {
    const path = join(scratchDirectory(), 'db.links');
    const store = UnitStore.open(path, { addressType: U32 });
    assert.equal(statSync(path).size, RESERVE_STEP * 32);
    assert.equal(store.header(Header.Reserved), RESERVE_STEP - 1);
    assert.equal(store.createPoint(), 1);
    store.sync();
    store.close();
    const again = UnitStore.open(path, { addressType: U32 });
    assert.deepEqual(parts(again.getLink(1)), [1, 1, 1]);
    again.close();
    const wide = UnitStore.open(join(scratchDirectory(), 'wide.links'));
    assert.equal(wide.type, U64);
    assert.equal(wide.memory.capacity, Math.max(DEFAULT_PAGE_SIZE, RESERVE_STEP));
    wide.close();
  });

  it('closes the file when its header cannot be read', () => {
    const path = join(scratchDirectory(), 'db.links');
    const bytes = Buffer.alloc(MIN_FILE_SIZE);
    bytes.writeBigUInt64LE(2n ** 60n, 0);
    writeFileSync(path, bytes);
    assert.throws(() => UnitStore.open(path), AddressOverflowError);
    // The descriptor was closed: the file can be replaced and opened again.
    writeFileSync(path, Buffer.alloc(0));
    UnitStore.open(path).close();
  });

  it('grows by a step when one reserved item is left', () => {
    const store = UnitStore.inMemory({ addressType: U32 });
    store.setHeader(Header.Allocated, RESERVE_STEP - 3);
    store.create();
    assert.equal(store.memory.capacity, RESERVE_STEP);
    store.create();
    assert.equal(store.memory.capacity, 2 * RESERVE_STEP);
    assert.equal(store.header(Header.Reserved), 2 * RESERVE_STEP);
  });

  it('reports the end of the address space', () => {
    const narrow = UnitStore.inMemory({ addressType: U32 });
    narrow.setHeader(Header.Allocated, U32.maxInner);
    assert.throws(() => narrow.create(), {
      name: 'DoubletsError',
      message:
        'Doublets store error: limit for the number of links in the storage has been reached: 4294967289',
    });
    const wide = UnitStore.inMemory();
    wide.setHeader(Header.Allocated, U64.max);
    assert.throws(() => wide.create(), {
      message: `Address ${2 ** 53} does not fit into the configured link address type`,
    });
    assert.ok(new DoubletsError('x') instanceof Error);
  });

  it('rejects queries it cannot answer', () => {
    const store = storeOf([[1, 1]], U32);
    assert.throws(() => store.countLinks([1, 1, 1, 1]), RangeError);
    assert.throws(() => store.eachLinks([1, 2, 3, 4], () => {}), /at most 3 parts, not 4/);
    assert.throws(() => store.getLink(2 ** 32), AddressOverflowError);
    assert.throws(() => store.countLinks([U32.any, -1]), AddressOverflowError);
    assert.throws(() => store.update(1, 2 ** 32, 1), AddressOverflowError);
    assert.throws(() => store.update(1, 1, 0.5), AddressOverflowError);
    assert.throws(() => store.update(5, 1, 1), NotFoundError);
    assert.throws(() => store.delete(5), { message: 'Link not found: 5' });
    assert.equal(part([], 2), 0);
  });

  it('counts and visits links by every query shape', () => {
    const store = storeOf([
      [1, 1],
      [1, 2],
      [2, 1],
      [0, 3],
      [3, 0],
    ]);
    const { any } = store;
    const count = (...query) => store.countLinks(query);
    assert.equal(store.count(), 5);
    assert.equal(count(any), 5);
    assert.equal(count(2), 1);
    assert.equal(count(9), 0);
    assert.equal(count(any, any), 5);
    assert.equal(count(any, 1), 4);
    assert.equal(count(9, 1), 0);
    assert.equal(count(2, any), 1);
    assert.equal(count(2, 2), 1);
    assert.equal(count(3, 1), 1);
    assert.equal(count(2, 3), 0);
    assert.equal(count(any, any, any), 5);
    assert.equal(count(any, any, 1), 2);
    assert.equal(count(any, 1, any), 2);
    assert.equal(count(any, 1, 2), 1);
    assert.equal(count(any, 2, 2), 0);
    assert.equal(count(9, any, any), 0);
    assert.equal(count(2, 1, any), 1);
    assert.equal(count(2, any, 2), 1);
    assert.equal(count(2, 2, any), 0);
    assert.equal(count(2, any, 1), 0);

    assert.deepEqual(rows(store), [
      [1, 1, 1],
      [2, 1, 2],
      [3, 2, 1],
      [4, 0, 3],
      [5, 3, 0],
    ]);
    assert.equal(rows(store, [any]).length, 5);
    assert.deepEqual(rows(store, [3]), [[3, 2, 1]]);
    assert.deepEqual(rows(store, [9]), []);
    assert.equal(rows(store, [any, any]).length, 5);
    assert.deepEqual(rows(store, [any, 3]), [
      [5, 3, 0],
      [4, 0, 3],
    ]);
    assert.deepEqual(rows(store, [9, 1]), []);
    assert.deepEqual(rows(store, [3, any]), [[3, 2, 1]]);
    assert.deepEqual(rows(store, [3, 2]), [[3, 2, 1]]);
    assert.deepEqual(rows(store, [3, 1]), [[3, 2, 1]]);
    assert.deepEqual(rows(store, [3, 3]), []);
    assert.equal(rows(store, [any, any, any]).length, 5);
    // A tree is walked in pre-order, from its root.
    assert.deepEqual(rows(store, [any, any, 1]), [
      [3, 2, 1],
      [1, 1, 1],
    ]);
    assert.deepEqual(rows(store, [any, 1, any]), [
      [2, 1, 2],
      [1, 1, 1],
    ]);
    assert.deepEqual(rows(store, [any, 2, 1]), [[3, 2, 1]]);
    assert.deepEqual(rows(store, [any, 2, 2]), []);
    assert.deepEqual(rows(store, [9, any, any]), []);
    assert.deepEqual(rows(store, [2, 1, any]), [[2, 1, 2]]);
    assert.deepEqual(rows(store, [2, any, 2]), [[2, 1, 2]]);
    assert.deepEqual(rows(store, [2, 2, 2]), []);
    assert.deepEqual(rows(store, [2, 1, 1]), []);
  });

  it('stops a walk when the handler returns false', () => {
    const store = storeOf([
      [1, 1],
      [1, 2],
      [2, 1],
    ]);
    const { any } = store;
    const first = (query) => {
      const seen = [];
      const finished = store.eachLinks(query, (value) => {
        seen.push(value.index);
        return false;
      });
      return [finished, seen];
    };
    assert.deepEqual(first([]), [false, [1]]);
    assert.deepEqual(first([2]), [false, [2]]);
    assert.deepEqual(first([any, 1]), [false, [2]]);
    assert.deepEqual(first([any, 2]), [false, [3]]);
    assert.deepEqual(first([2, 2]), [false, [2]]);
    assert.deepEqual(first([any, 1, any]), [false, [2]]);
    assert.deepEqual(first([2, 1, 2]), [false, [2]]);
    assert.equal(store.find([any, 9, 9]), null);
    assert.equal(store.search(2, 1), 3);
    assert.equal(store.search(2, 2), null);
    assert.equal(store.getOrCreate(2, 1), 3);
    assert.equal(store.getOrCreate(2, 2), 4);
  });

  it('reports every change of a write', () => {
    const store = UnitStore.inMemory({ addressType: U32 });
    const { changes, handler } = recorder();
    store.createLinkWith(0, 0, handler);
    store.updateWith(1, 1, 1, handler);
    store.deleteWith(1, handler);
    assert.deepEqual(changes, [
      [
        [0, 0, 0],
        [1, 0, 0],
      ],
      [
        [1, 0, 0],
        [1, 0, 0],
      ],
      [
        [1, 0, 0],
        [1, 1, 1],
      ],
      [
        [1, 1, 1],
        [0, 0, 0],
      ],
    ]);
    assert.equal(store.exist(0), false);
    assert.equal(store.exist(U32.maxInner + 1), false);
    assert.equal(store.tryGetLink(store.create()).index, 1);
  });

  it('reuses freed addresses last freed first and gives back the trailing ones', () => {
    const store = storeOf(new Array(6).fill([0, 0]), U32);
    store.delete(2);
    store.delete(4);
    store.delete(3);
    assert.equal(store.header(Header.Free), 3);
    assert.equal(store.isUnused(3), true);
    assert.equal(store.isUnused(7), true);
    assert.equal(store.create(), 3);
    store.delete(6);
    store.delete(5);
    assert.equal(store.header(Header.Allocated), 3);
    assert.equal(store.header(Header.Free), 1);
    assert.equal(store.header(Header.LastFree), 2);
    store.delete(3);
    store.delete(1);
    assert.equal(store.header(Header.Allocated), 0);
    assert.equal(store.header(Header.Free), 0);
    assert.equal(store.header(Header.FirstFree), 0);
  });

  it('cannot tell two links with the same parts apart, as Rust cannot', () => {
    const store = storeOf([
      [5, 5],
      [5, 5],
    ]);
    store.update(2, 0, 0);
    assert.throws(() => store.update(1, 0, 0), {
      name: 'DoubletsError',
      message: 'Doublets store error: link 1 is missing from the sources tree',
    });
    const corrupt = {
      name: 'DoubletsError',
      message:
        'Doublets store error: the store is corrupt: link 1 is missing from the sources tree',
    };
    assert.throws(() => store.getLink(1), corrupt);
    assert.throws(() => store.count(), corrupt);
    assert.throws(() => store.eachIter([]), corrupt);
    assert.throws(() => store.createPoint(), corrupt);
    assert.throws(
      () => store.delete(2),
      (error) => error.cause.corrupt === true,
    );
  });

  it('stays usable after an error that leaves the trees whole', () => {
    const store = storeOf([[1, 1]]);
    assert.throws(() => store.delete(7), NotFoundError);
    assert.deepEqual(store.eachIter([]).map(parts), [[1, 1, 1]]);
    assert.equal(store.createPoint(), 2);
  });

  it('reports a link its trees do not hold', () => {
    const corrupt = (pairs, garbage) => {
      const store = storeOf(pairs);
      store.setHeader(Header.Allocated, 2);
      for (const [field, value] of garbage) store.memory.set(2, field, value);
      return () => store.update(2, 0, 0);
    };
    const left = [
      [Field.Source, 1],
      [Field.Target, 1],
      [Field.SizeAsSource, 1],
    ];
    assert.throws(corrupt([[9, 9]], left), /link 2 is missing from the sources tree/);
    const right = [
      [Field.Source, 0],
      [Field.Target, 9],
    ];
    assert.throws(corrupt([[1, 1]], right), /link 2 is missing from the targets tree/);
  });
});

/**
 * Random writes to a raw store against a model of it: the links, and the
 * free list it reuses last freed first. Pairs stay unique, as they must
 * without the resolvers, and the queries check both trees after every step.
 */
function modelRun(seed, type, steps) {
  const random = seededRandom(seed);
  const store = UnitStore.inMemory({ addressType: type });
  const { any } = store;
  const live = new Map();
  let free = [];
  let allocated = 0;
  const value = () => Math.floor(random() * 24);
  // The trees hold no null part, so a query by a null part finds nothing.
  const nonNull = () => 1 + Math.floor(random() * 23);
  const taken = (source, target) =>
    (source !== 0 || target !== 0) &&
    [...live.values()].some(([s, t]) => s === source && t === target);
  for (let step = 0; step < steps; step++) {
    const roll = random();
    const [source, target] = [value(), value()];
    const index = 1 + Math.floor(random() * (allocated + 1));
    if (roll < 0.5) {
      if (taken(source, target)) continue;
      const created = store.createLink(source, target);
      const expected = free.length > 0 ? free.shift() : ++allocated;
      assert.equal(created, expected);
      live.set(created, [source, target]);
    } else if (roll < 0.75) {
      if (!live.has(index) || taken(source, target)) continue;
      store.update(index, source, target);
      live.set(index, [source, target]);
    } else {
      if (!live.has(index)) {
        assert.throws(() => store.delete(index), NotFoundError);
        continue;
      }
      store.delete(index);
      live.delete(index);
      if (index < allocated) free.unshift(index);
      else {
        allocated = index - 1;
        while (allocated > 0 && !live.has(allocated)) {
          free = free.filter((element) => element !== allocated);
          allocated--;
        }
      }
    }
    const sorted = [...live].sort(([left], [right]) => left - right);
    assert.deepEqual(
      rows(store),
      sorted.map(([at, [s, t]]) => [at, s, t]),
    );
    assert.equal(store.header(Header.Allocated), allocated);
    assert.equal(store.header(Header.Free), free.length);
    const probe = nonNull();
    const bySource = sorted.filter(([, [s]]) => s === probe).map(([at]) => at);
    const byTarget = sorted.filter(([, [, t]]) => t === probe).map(([at]) => at);
    const indexes = (query) =>
      store
        .eachIter(query)
        .map((found) => found.index)
        .sort((left, right) => left - right);
    assert.deepEqual(indexes([any, probe, any]), bySource);
    assert.deepEqual(indexes([any, any, probe]), byTarget);
    assert.equal(store.countLinks([any, probe, any]), bySource.length);
    assert.equal(store.countLinks([any, any, probe]), byTarget.length);
    assert.equal(store.countLinks([any, probe]), bySource.length + byTarget.length);
    for (const [at, [s, t]] of sorted) {
      // `search` walks the sources tree only, as in Rust.
      if (s !== 0 || t !== 0) assert.equal(store.search(s, t), s === 0 ? null : at);
    }
  }
  return live.size;
}

describe('the trees and the free list against a model', () => {
  for (const [seed, type] of [
    [1, U32],
    [2, U64],
    [3, U32],
  ]) {
    it(`agree on seed ${seed} (${type.name})`, () => {
      assert.ok(modelRun(seed, type, 900) > 0);
    });
  }
});

describe('the resolution decorators', () => {
  it('forward everything they do not resolve', () => {
    const path = join(scratchDirectory(), 'db.links');
    const links = withAutomaticUniquenessAndUsagesResolution(UnitStore.open(path));
    assert.ok(links instanceof CascadeUniquenessAndUsagesResolver);
    assert.ok(links.links instanceof NonNullContentsLinkDeletionResolver);
    assert.ok(links.links.links instanceof CascadeUsagesResolver);
    assert.equal(links.type, U64);
    assert.equal(links.createPoint(), 1);
    assert.equal(links.createLink(1, 0), 2);
    assert.equal(links.count(), 2);
    assert.deepEqual(rows(links, [links.any, 1, links.any]), [
      [1, 1, 1],
      [2, 1, 0],
    ]);
    assert.deepEqual(parts(links.getLink(2)), [2, 1, 0]);
    links.sync();
    links.reload();
    links.close();
    const fd = openSync(path, 'r');
    closeSync(fd);
  });

  it('merge a link updated into a duplicate of another one', () => {
    const links = withAutomaticUniquenessAndUsagesResolution(UnitStore.inMemory());
    links.createLink(0, 0);
    links.createLink(1, 1);
    links.createLink(2, 2);
    links.createLink(3, 1);
    links.createLink(1, 3);
    links.createLink(3, 3);
    const { changes, handler } = recorder();
    links.updateWith(3, 1, 1, handler);
    // 6 became (2, 2), a duplicate of 3, so it was merged into 3 in turn.
    assert.deepEqual(rows(links), [
      [1, 0, 0],
      [2, 1, 1],
      [4, 2, 1],
      [5, 1, 2],
    ]);
    assert.deepEqual(changes.at(-1), [
      [2, 1, 1],
      [2, 1, 1],
    ]);
    const unchanged = recorder();
    links.updateWith(2, 1, 1, unchanged.handler);
    assert.deepEqual(unchanged.changes, [
      [
        [2, 1, 1],
        [2, 1, 1],
      ],
    ]);
    links.updateWith(1, 9, 9, () => {});
    assert.deepEqual(parts(links.getLink(1)), [1, 9, 9]);
  });

  it('cascade a deletion to the usages, each once', () => {
    const links = withAutomaticUniquenessAndUsagesResolution(
      UnitStore.inMemory({ addressType: U32 }),
    );
    links.createLink(0, 0);
    links.createLink(1, 1);
    links.createLink(1, 2);
    links.createLink(2, 3);
    links.createLink(0, 0);
    const { changes, handler } = recorder();
    links.deleteWith(1, handler);
    assert.deepEqual(rows(links), [[5, 0, 0]]);
    assert.ok(changes.length > 4);
    // 1 and 2 use each other: the cascade visits each of them once.
    assert.equal(links.createLink(1, 2), 1);
    assert.equal(links.createLink(2, 1), 2);
    links.delete(1);
    assert.deepEqual(rows(links), [[5, 0, 0]]);
    links.delete(5);
    assert.equal(links.count(), 0);
    // 3 uses both 2 and 1: deleting 2 first deletes it.
    links.createLink(0, 0);
    links.createLink(1, 0);
    links.createLink(2, 1);
    links.delete(1);
    assert.equal(links.count(), 0);
  });

  it('cascade through a cycle of usages each address once', () => {
    const store = storeOf([
      [2, 2],
      [1, 1],
    ]);
    const links = new CascadeUsagesResolver(store);
    links.delete(1);
    assert.equal(store.count(), 0);
  });
});
