// The doublets decorator stack a LinkStorage writes through. The traces are
// the ones the Rust clink prints for the same databases.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { LinkStorage } from '../src/link-storage.js';
import {
  byPattern,
  DOUBLETS_ANY,
  doubletsSearch,
  exist,
  resolvedDelete,
  resolvedUpdate,
  usagesOf,
} from '../src/link-storage-doublets.js';
import { capture, scratchDirectory, traceLines } from './helpers/storage.mjs';

const parts = (value) => [value.index, value.source, value.target];

function traced(text) {
  const path = join(scratchDirectory(), 'db.links');
  writeFileSync(path, text);
  const stderr = capture();
  const storage = new LinkStorage(path, { trace: true, stderr });
  stderr.text = '';
  return storage;
}

function store() {
  const storage = LinkStorage.inMemory();
  storage.create(1, 1);
  storage.create(1, 2);
  storage.create(2, 1);
  storage.create(1, 2);
  return storage;
}

describe('doublets lookups', () => {
  it('treats external references as existing', () => {
    const storage = store();
    assert.equal(exist(storage, 1), true);
    assert.equal(exist(storage, 9), false);
    assert.equal(exist(storage, 0), false);
    assert.equal(exist(storage, 0x7ffffff9), false);
    assert.equal(exist(storage, 0x7ffffffa), false);
    assert.equal(exist(storage, 0x7fffffff), true);
    assert.equal(exist(storage, 0xffffffff), true);
  });

  it('matches patterns as the unit store indexes do', () => {
    const storage = store();
    assert.deepEqual(byPattern(storage, DOUBLETS_ANY, DOUBLETS_ANY).map(parts), [
      [1, 1, 1],
      [2, 1, 2],
      [3, 2, 1],
      [4, 1, 2],
    ]);
    assert.deepEqual(byPattern(storage, DOUBLETS_ANY, 1).map(parts), [
      [1, 1, 1],
      [3, 2, 1],
    ]);
    assert.deepEqual(byPattern(storage, 2, DOUBLETS_ANY).map(parts), [[3, 2, 1]]);
    assert.deepEqual(byPattern(storage, 1, 2).map(parts), [[2, 1, 2]]);
    assert.deepEqual(byPattern(storage, 2, 2), []);
    storage.create(0, 0);
    storage.create(0, 1);
    assert.deepEqual(byPattern(storage, DOUBLETS_ANY, 0), []);
    assert.deepEqual(byPattern(storage, 0, DOUBLETS_ANY), []);
    assert.deepEqual(byPattern(storage, 0, 1), []);
    assert.deepEqual(byPattern(storage, 1, 0), []);
  });

  it('searches the lowest exact pair', () => {
    const storage = store();
    assert.equal(doubletsSearch(storage, 1, 2), 2);
    assert.equal(doubletsSearch(storage, 2, 2), null);
  });

  it('lists the usages of a link without the link itself', () => {
    const storage = store();
    assert.deepEqual(usagesOf(storage, 1).map(parts), [
      [2, 1, 2],
      [4, 1, 2],
      [3, 2, 1],
    ]);
    assert.deepEqual(usagesOf(storage, 2).map(parts), [
      [3, 2, 1],
      [4, 1, 2],
    ]);
  });
});

describe('doublets resolution', () => {
  it('merges usages through nested duplicates', () => {
    const storage = traced('(1 1 1)\n(2 3 1)\n(3 2 2)\n(4 1 6)\n(5 4 4)\n(6 6 6)\n(7 5 1)\n');
    const changes = [];
    resolvedUpdate(storage, 2, 1, 6, (before, after) =>
      changes.push([parts(before), parts(after)]),
    );
    assert.deepEqual(traceLines(storage.stderr), [
      '[TRACE] Updating link 3 from (2 2) to (7 7)',
      '[TRACE] Updating link 2 from (3 1) to (0 0)',
      '[TRACE] Deleted link: (2 0 0)',
      '[TRACE] Updating link 3 from (7 7) to (0 0)',
      '[TRACE] Deleted link: (3 0 0)',
    ]);
    assert.deepEqual(changes.at(-1), [
      [4, 1, 6],
      [4, 1, 6],
    ]);
    assert.deepEqual(
      storage.all().map((value) => value.index),
      [1, 4, 5, 6, 7],
    );
  });

  it('points a usage by target at the survivor', () => {
    const storage = traced('(1 1 1)\n(2 2 2)\n(3 1 2)\n(4 2 1)\n(5 1 4)\n');
    resolvedUpdate(storage, 4, 1, 2, () => {});
    assert.deepEqual(traceLines(storage.stderr), [
      '[TRACE] Updating link 5 from (1 4) to (1 3)',
      '[TRACE] Updating link 4 from (2 1) to (0 0)',
      '[TRACE] Deleted link: (4 0 0)',
    ]);
    assert.deepEqual(storage.get(5), { index: 5, source: 1, target: 3 });
  });

  it('skips a usage an earlier usage already deleted', () => {
    const storage = traced('(1 1 1)\n(2 1 3)\n(3 2 1)\n');
    resolvedDelete(storage, 1, () => {});
    assert.deepEqual(traceLines(storage.stderr), [
      '[TRACE] Updating link 1 from (1 1) to (0 0)',
      '[TRACE] Updating link 2 from (1 3) to (0 0)',
      '[TRACE] Updating link 3 from (2 1) to (0 0)',
      '[TRACE] Deleted link: (3 0 0)',
      '[TRACE] Deleted link: (2 0 0)',
      '[TRACE] Deleted link: (1 0 0)',
    ]);
    assert.deepEqual(storage.all(), []);
  });

  it('deletes a reset link without updating it again', () => {
    const storage = traced('(1 0 0)\n');
    resolvedDelete(storage, 1, () => {});
    assert.deepEqual(traceLines(storage.stderr), ['[TRACE] Deleted link: (1 0 0)']);
  });

  it('fails for a missing link and for a survivor the merge deleted', () => {
    assert.throws(() => resolvedDelete(LinkStorage.inMemory(), 3, () => {}), {
      message: 'Link not found: 3',
    });
    const storage = LinkStorage.inMemory();
    storage.create(1, 1);
    storage.create(1, 3);
    storage.create(3, 3);
    storage.create(2, 3);
    assert.throws(() => resolvedUpdate(storage, 1, 1, 3, () => {}), {
      message: 'Link not found: 2',
    });
  });
});
