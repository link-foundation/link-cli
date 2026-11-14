// Pinned types `(a: 1 a)` at fixed addresses. The errors are the ones the
// Rust clink reports for the same names databases.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { LinkStorage } from '../src/link-storage.js';
import { PinnedTypes, pinnedTypes } from '../src/pinned-types.js';
import { scratchDirectory } from './helpers/storage.mjs';

function stored(text) {
  const path = join(scratchDirectory(), 'db.names.links');
  writeFileSync(path, text);
  return new LinkStorage(path, { stderr: { write() {} } });
}

const rows = (storage) => storage.all().map((value) => [value.index, value.source, value.target]);

describe('pinned types', () => {
  it('creates the missing types and keeps the existing ones', () => {
    const storage = stored('(1 1 1)\n');
    assert.deepEqual(pinnedTypes(storage, 3), [1, 2, 3]);
    assert.deepEqual(rows(storage), [
      [1, 1, 1],
      [2, 1, 2],
      [3, 1, 3],
    ]);
    const pinned = new PinnedTypes(storage);
    assert.equal(pinned.nextType(), 1);
    assert.deepEqual(pinned.takeTypes(2), [2, 3]);
    assert.deepEqual(pinnedTypes(storage, 0), []);
  });

  it('rejects a different link at a type address', () => {
    assert.throws(() => pinnedTypes(stored('(1 1 1)\n(2 2 2)\n'), 2), {
      message: 'Unexpected link found at address 2. Expected: (2: 1 2), Found: (2: 2 2).',
    });
    assert.throws(() => pinnedTypes(stored('(1 2 1)\n'), 1), {
      message: 'Unexpected link found at address 1. Expected: (1: 1 1), Found: (1: 2 1).',
    });
  });

  it('rejects a type the store creates elsewhere', () => {
    assert.throws(() => pinnedTypes(stored('(2 1 1)\n'), 1), {
      message: 'Pinned type address 1 could not be created deterministically; got 2.',
    });
  });
});
