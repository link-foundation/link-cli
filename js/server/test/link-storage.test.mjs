// LinkStorage, the text links database: the cases of
// rust/tests/link_storage_tests.rs plus the file format, the trace lines and
// the error reports the Rust clink prints for them.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import {
  chmodSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  writeFileSync,
} from 'node:fs';
import { join } from 'node:path';
import { formatErrorReport } from '../src/error-report.js';
import { link } from '../src/link.js';
import { LinkStorage, NO_REVISION, sameRevision, storageRevision } from '../src/link-storage.js';
import { FileLock, lockBackend, LockMode } from '../src/storage-lock.js';
import { capture, scratchDirectory, traceLines } from './helpers/storage.mjs';

const parts = (value) => [value.index, value.source, value.target];
const rows = (storage) => storage.all().map(parts);

function database(text) {
  const path = join(scratchDirectory(), 'db.links');
  writeFileSync(path, text);
  return path;
}

describe('LinkStorage basics', () => {
  it('creates, updates, deletes, searches and gets or creates links', () => {
    const storage = LinkStorage.inMemory();
    const id = storage.create(2, 3);
    assert.deepEqual(parts(storage.get(id)), [1, 2, 3]);
    assert.equal(storage.getLink(id), storage.get(id));
    storage.update(id, 4, 5);
    assert.deepEqual(parts(storage.get(id)), [1, 4, 5]);
    assert.equal(storage.search(4, 5), id);
    assert.equal(storage.search(1, 1), null);
    assert.equal(storage.getOrCreate(4, 5), id);
    assert.equal(storage.getOrCreate(5, 4), 2);
    storage.delete(id);
    assert.equal(storage.get(id), null);
    assert.equal(storage.exists(id), false);
    assert.equal(storage.exists(2), true);
  });

  it('matches service constants literally', () => {
    const storage = LinkStorage.inMemory();
    const existing = storage.create(0, 0);
    storage.update(existing, 7, 42);
    for (const reserved of [0xffffffff, 0xfffffffe, 0xfffffffd, 0xfffffffc, 0xfffffffb]) {
      const created = storage.getOrCreate(reserved, 42);
      assert.notEqual(created, existing);
      assert.deepEqual(parts(storage.get(created)), [created, reserved, 42]);
      assert.equal(storage.getOrCreate(reserved, 42), created);
    }
  });

  it('queries by pattern, by address', () => {
    const storage = LinkStorage.inMemory();
    storage.create(1, 2);
    storage.create(1, 3);
    storage.create(2, 3);
    assert.deepEqual(storage.query().map(parts), [
      [1, 1, 2],
      [2, 1, 3],
      [3, 2, 3],
    ]);
    assert.deepEqual(storage.queryLinks(null, 1).map(parts), [
      [1, 1, 2],
      [2, 1, 3],
    ]);
    assert.deepEqual(storage.query(null, null, 3).map(parts), [
      [2, 1, 3],
      [3, 2, 3],
    ]);
    assert.deepEqual(storage.allLinks().map(parts), rows(storage));
  });

  it('formats links with their names', () => {
    const storage = LinkStorage.inMemory();
    storage.create(1, 2);
    storage.create(1, 1);
    storage.setName(1, 'one');
    assert.equal(storage.format(storage.get(1)), '(one one 2)');
    const out = capture();
    storage.printAllLinks(out);
    storage.printChange(null, storage.get(2), out);
    storage.printChange(storage.get(2), undefined, out);
    assert.equal(out.text, '(one one 2)\n(2 one one)\n() ((2 one one))\n((2 one one)) ()\n');
  });

  it('reports the in-memory state and the trace switch', () => {
    const storage = LinkStorage.inMemory({ trace: true, stderr: capture() });
    assert.equal(storage.isInMemory(), true);
    assert.equal(storage.databasePath(), '');
    assert.equal(storage.isTraceEnabled(), true);
    assert.equal(LinkStorage.inMemory().isTraceEnabled(), false);
    assert.equal(new LinkStorage('').isTraceEnabled(), false);
  });
});

describe('LinkStorage names', () => {
  it('names links and finds them by name', () => {
    const storage = LinkStorage.inMemory();
    const id = storage.getOrCreateNamed('test');
    assert.deepEqual(parts(storage.get(id)), [1, 1, 1]);
    assert.equal(storage.getName(id), 'test');
    assert.equal(storage.getByName('test'), id);
    assert.equal(storage.getOrCreateNamed('test'), id);
    assert.equal(storage.getName(9), null);
    assert.equal(storage.getByName('missing'), null);
  });

  it('moving a name unnames its previous link', () => {
    const storage = LinkStorage.inMemory();
    const first = storage.create(5, 5);
    const second = storage.create(6, 6);
    storage.setName(first, 'moving');
    storage.setName(second, 'moving');
    assert.equal(storage.getByName('moving'), second);
    assert.equal(storage.getName(first), null);
    storage.removeName(first);
    assert.equal(storage.getByName('moving'), second);
    storage.setName(second, 'moving');
    assert.equal(storage.getName(second), 'moving');
    storage.setName(second, 'renamed');
    assert.equal(storage.getByName('moving'), null);
    assert.equal(storage.getByName('renamed'), second);
    storage.removeName(second);
    assert.equal(storage.getName(second), null);
  });

  it('keeps the name when the point cannot be made, as Rust drops the error', () => {
    // The new link 3 becomes (3 3), a duplicate of 2, so it merges into 2,
    // which itself merges into 4 on the way: the survivor 2 is gone and the
    // update fails with "Link not found: 2".
    const storage = new LinkStorage(database('(1 1 1)\n(2 3 3)\n(4 2 2)\n'));
    const id = storage.getOrCreateNamed('named');
    assert.equal(id, 3);
    assert.equal(storage.getByName('named'), 3);
    assert.deepEqual(rows(storage), [
      [1, 1, 1],
      [4, 4, 4],
    ]);
  });

  it('deleting a link removes its name', () => {
    const storage = LinkStorage.inMemory();
    const id = storage.getOrCreateNamed('gone');
    storage.delete(id);
    assert.equal(storage.getByName('gone'), null);
  });
});

describe('LinkStorage allocation', () => {
  it('reuses a freed address before the store grows', () => {
    const storage = LinkStorage.inMemory();
    assert.deepEqual([storage.create(1, 1), storage.create(2, 2), storage.create(3, 3)], [1, 2, 3]);
    storage.deleteRaw(2);
    assert.equal(storage.create(1, 3), 2);
    assert.equal(storage.create(3, 1), 4);
  });

  it('reuses freed addresses most recently freed first', () => {
    const storage = LinkStorage.inMemory();
    for (let part = 1; part <= 5; part++) storage.create(part, part);
    storage.deleteRaw(2);
    storage.deleteRaw(4);
    assert.equal(storage.create(1, 3), 4);
    assert.equal(storage.create(3, 1), 2);
    assert.equal(storage.create(1, 5), 6);
  });

  it('shrinks when the last link is freed', () => {
    const storage = LinkStorage.inMemory();
    for (let part = 1; part <= 3; part++) storage.create(part, part);
    storage.deleteRaw(2);
    storage.deleteRaw(3);
    assert.equal(storage.create(1, 2), 2);
    assert.equal(storage.create(2, 1), 3);
  });

  it('ignores the release of the null and of unallocated addresses', () => {
    const storage = LinkStorage.inMemory();
    storage.create(1, 1);
    storage.release(0);
    storage.release(7);
    assert.deepEqual([storage.allocated, storage.unused], [1, []]);
  });

  it('ensures a link by freeing the addresses it passed over', () => {
    const storage = LinkStorage.inMemory();
    storage.create(1, 1);
    assert.equal(storage.ensureCreated(4), 4);
    assert.equal(storage.exists(2), false);
    assert.equal(storage.exists(3), false);
    assert.deepEqual(parts(storage.get(4)), [4, 0, 0]);
    assert.equal(storage.ensureCreated(4), 4);
    assert.equal(storage.ensureCreated(0), 0);
    assert.equal(storage.create(1, 4), 3);
    assert.equal(storage.create(4, 1), 2);
  });

  it('reports the next created addresses without keeping them', () => {
    const storage = LinkStorage.inMemory();
    storage.create(1, 1);
    storage.create(2, 2);
    storage.deleteRaw(1);
    assert.deepEqual(storage.nextCreatedAddresses(3), [1, 3, 4]);
    assert.deepEqual(rows(storage), [[2, 2, 2]]);
    assert.equal(storage.create(5, 5), 1);
  });

  it('copies the recycling state for a query preview', () => {
    const storage = LinkStorage.inMemory();
    storage.create(1, 1);
    storage.create(2, 2);
    storage.setName(2, 'two');
    storage.deleteRaw(1);
    storage.enableIndexedLookups();
    const copy = storage.queryCopy();
    assert.equal(copy.create(3, 3), 1);
    assert.equal(copy.getName(2), 'two');
    assert.equal(copy.indexedLookups, true);
    assert.deepEqual(rows(storage), [[2, 2, 2]]);
    const preview = {
      preview(scratch, restriction, substitution) {
        assert.deepEqual([restriction, substitution], ['r', 's']);
        scratch.create(4, 4);
        scratch.create(5, 5);
      },
    };
    assert.deepEqual([...storage.reserveQueryAllocations(preview, 'r', 's')], [1, 2, 3]);
    assert.deepEqual(rows(storage), [[2, 2, 2]]);
  });
});

describe('LinkStorage resolution stack', () => {
  const merged = () =>
    new LinkStorage(database('(1 1 1)\n(2 2 2)\n(3 1 2)\n(4 3 3)\n(5 2 1)\n(6 5 4)\n'), {
      trace: true,
      stderr: capture(),
    });

  it('merges an update into the existing doublet, as the Rust clink traces it', () => {
    const storage = merged();
    storage.stderr.text = '';
    const changes = [];
    const before = storage.updateObserved(5, 1, 2, (b, a) => changes.push([parts(b), parts(a)]));
    assert.deepEqual(parts(before), [5, 2, 1]);
    assert.deepEqual(traceLines(storage.stderr), [
      '[TRACE] Updating link 6 from (5 4) to (3 4)',
      '[TRACE] Updating link 5 from (2 1) to (0 0)',
      '[TRACE] Deleted link: (5 0 0)',
    ]);
    assert.deepEqual(changes, [
      [
        [6, 5, 4],
        [6, 3, 4],
      ],
      [
        [5, 2, 1],
        [5, 0, 0],
      ],
      [
        [5, 0, 0],
        [0, 0, 0],
      ],
    ]);
    assert.deepEqual(rows(storage), [
      [1, 1, 1],
      [2, 2, 2],
      [3, 1, 2],
      [4, 3, 3],
      [6, 3, 4],
    ]);
  });

  it('reports the survivor when a link is updated to its own doublet', () => {
    const storage = merged();
    const changes = [];
    storage.updateObserved(3, 1, 2, (b, a) => changes.push([parts(b), parts(a)]));
    assert.deepEqual(changes, [
      [
        [3, 1, 2],
        [3, 1, 2],
      ],
    ]);
  });

  it('cascades a deletion to every link still using it, as the Rust clink traces it', () => {
    const stderr = capture();
    const storage = new LinkStorage(
      database('# unused: 5\n(1 1 1)\n(2 2 2)\n(3 1 2)\n(4 3 3)\n(6 3 4)\n'),
      { trace: true, stderr },
    );
    stderr.text = '';
    const changes = [];
    storage.deleteObserved(1, (b, a) => changes.push([parts(b), parts(a)]));
    assert.deepEqual(traceLines(stderr), [
      '[TRACE] Updating link 1 from (1 1) to (0 0)',
      '[TRACE] Updating link 3 from (1 2) to (0 0)',
      '[TRACE] Updating link 4 from (3 3) to (0 0)',
      '[TRACE] Updating link 6 from (3 4) to (0 0)',
      '[TRACE] Deleted link: (6 0 0)',
      '[TRACE] Deleted link: (4 0 0)',
      '[TRACE] Deleted link: (3 0 0)',
      '[TRACE] Deleted link: (1 0 0)',
    ]);
    assert.deepEqual(changes.slice(0, 2), [
      [
        [1, 1, 1],
        [1, 0, 0],
      ],
      [
        [3, 1, 2],
        [3, 0, 0],
      ],
    ]);
    assert.deepEqual(changes[7], [
      [1, 0, 0],
      [0, 0, 0],
    ]);
    assert.deepEqual(rows(storage), [[2, 2, 2]]);
    storage.save();
    assert.equal(readFileSync(storage.databasePath(), 'utf8'), '# unused: 1\n(2 2 2)\n');
  });

  it('fails for a link that does not exist', () => {
    const storage = LinkStorage.inMemory();
    assert.throws(() => storage.updateObserved(4, 1, 1, () => {}), {
      message: 'Link not found: 4',
    });
    assert.throws(() => storage.deleteObserved(4, () => {}), { message: 'Link not found: 4' });
    assert.throws(() => storage.updateRaw(4, 1, 1), { message: 'Link not found: 4' });
    assert.throws(() => storage.deleteRaw(4), { message: 'Link not found: 4' });
  });
});

describe('LinkStorage files', () => {
  it('loads every row form, the free list and the name metadata, and writes them back', () => {
    const path = database(
      '# unused: 3 0 9 2 2 6 x\n(1 1 1 "one")\n  (2 2 2)  \r\njunk line\n\n(5 1 2 "a b")\n' +
        '# comment\n(7 7 7 bare)\n# link-cli:name:1 9 ~1{67686f7374}\n' +
        '# link-cli:name:1 2 ~1{610962}\n(8 1 7)',
    );
    const storage = new LinkStorage(path);
    assert.equal(storage.getName(2), 'a\tb');
    assert.equal(storage.getName(9), 'ghost');
    assert.equal(storage.getName(7), 'bare');
    storage.save();
    // The text the Rust clink writes for the same file.
    assert.equal(
      readFileSync(path, 'utf8'),
      '# unused: 3 6\n(1 1 1 "one")\n(2 2 2)\n(5 1 2 "a b")\n(7 7 7 "bare")\n(8 1 7)\n' +
        '# link-cli:name:1 2 ~1{610962}\n# link-cli:name:1 9 ~1{67686f7374}\n',
    );
    assert.deepEqual(readdirSync(join(path, '..')), ['db.links']);
    const again = new LinkStorage(path);
    assert.equal(again.create(1, 3), 3);
    assert.equal(again.create(3, 1), 6);
    assert.equal(again.create(1, 5), 9);
  });

  it('keeps the free list across a save and a load', () => {
    const path = join(scratchDirectory(), 'db.links');
    const storage = new LinkStorage(path);
    for (let part = 1; part <= 5; part++) storage.create(part, part);
    storage.deleteRaw(2);
    storage.deleteRaw(4);
    storage.save();
    const reloaded = new LinkStorage(path);
    assert.equal(reloaded.create(1, 3), 4);
    assert.equal(reloaded.create(3, 1), 2);
    assert.equal(reloaded.create(1, 5), 6);
  });

  it('recovers the missing addresses of a file without a free list', () => {
    const storage = new LinkStorage(database('(1 1 1)\n(3 3 3)\n(5 5 5)\n'));
    assert.equal(storage.create(1, 3), 2);
    assert.equal(storage.create(3, 1), 4);
    assert.equal(storage.create(1, 5), 6);
  });

  it('traces loads, saves and every write', () => {
    const path = join(scratchDirectory(), 'db.links');
    const stderr = capture();
    const storage = new LinkStorage(path, { trace: true, stderr }).withIndexedLookups();
    const id = storage.getOrCreateNamed('x');
    storage.ensureCreated(3);
    storage.removeName(id);
    storage.save();
    storage.reloadFromDisk();
    assert.deepEqual(traceLines(stderr), [
      '[TRACE] IndexedLinksDecorator enabled for 0 links',
      '[TRACE] Created link: (1 0 0)',
      '[TRACE] Updating link 1 from (0 0) to (1 1)',
      '[TRACE] Created named link: x => 1',
      '[TRACE] Created link: (2 0 0)',
      '[TRACE] Created link: (3 0 0)',
      '[TRACE] Deleted link: (2 0 0)',
      '[TRACE] Ensured link: (3 0 0)',
      '[TRACE] Removed name: 1 => x',
      `[TRACE] Saved 2 links to ${path}`,
      `[TRACE] Loaded 2 links from ${path}`,
      '[TRACE] IndexedLinksDecorator enabled for 2 links',
    ]);
    assert.equal(storage.indexedLookups, true);
  });

  it('reloads only from the file, keeping lookups unindexed', () => {
    const path = database('(1 1 1)\n');
    const storage = new LinkStorage(path);
    storage.create(2, 2);
    storage.setName(1, 'stored');
    storage.reloadFromDisk();
    assert.deepEqual(rows(storage), [[1, 1, 1]]);
    assert.equal(storage.getByName('stored'), null);
    assert.equal(storage.indexedLookups, false);
  });

  it('an in-memory store never touches a file and reloads empty', () => {
    for (const indexed of [false, true]) {
      const storage = indexed
        ? LinkStorage.inMemory().withIndexedLookups()
        : LinkStorage.inMemory();
      storage.create(10, 11);
      storage.setName(1, 'stored');
      storage.setName(0, 'external');
      storage.save();
      storage.refreshObservedRevision();
      assert.equal(storage.hasExternalChanges(), false);
      assert.equal(storage.observedRevision(), NO_REVISION);
      storage.reloadFromDisk();
      assert.deepEqual(rows(storage), []);
      assert.equal(storage.getByName('external'), null);
      assert.equal(storage.create(30, 31), 1);
    }
  });

  it('notices a database another process replaced', () => {
    const path = join(scratchDirectory(), 'db.links');
    const storage = new LinkStorage(path);
    assert.equal(storage.isInMemory(), false);
    assert.equal(storage.hasExternalChanges(), false);
    const other = new LinkStorage(path);
    other.create(1, 1);
    other.save();
    assert.equal(storage.hasExternalChanges(), true);
    storage.refreshObservedRevision();
    assert.equal(storage.hasExternalChanges(), false);
    assert.equal(sameRevision(storage.observedRevision(), storageRevision(path)), true);
    assert.equal(storageRevision(join(path, '..', 'missing')), NO_REVISION);
    assert.equal(sameRevision(NO_REVISION, { len: 0, modified: 1n }), false);
  });
});

describe('LinkStorage errors', () => {
  const report = (action) => {
    try {
      action();
    } catch (error) {
      return formatErrorReport(error);
    }
    assert.fail('expected an error');
  };

  it('reports a database below a file as the Rust I/O error', () => {
    const directory = scratchDirectory();
    writeFileSync(join(directory, 'f'), 'x');
    assert.equal(
      report(() => new LinkStorage(join(directory, 'f', 'db.links'))),
      'I/O error: Not a directory (os error 20)\n\nCaused by:\n    Not a directory (os error 20)',
    );
  });

  it('reports a database that is a directory', () => {
    const path = join(scratchDirectory(), 'd.links');
    mkdirSync(path);
    assert.equal(
      report(() => new LinkStorage(path)),
      `Failed to read ${path} at line 1\n\nCaused by:\n    Is a directory (os error 21)`,
    );
  });

  it('reports a database it may not open', () => {
    const path = database('(1 1 1)\n');
    chmodSync(path, 0o000);
    try {
      assert.equal(
        report(() => new LinkStorage(path)),
        `Failed to open database: ${path}\n\nCaused by:\n    Permission denied (os error 13)`,
      );
    } finally {
      chmodSync(path, 0o644);
    }
  });

  it('reports the line that is not UTF-8', () => {
    const path = join(scratchDirectory(), 'bad.links');
    writeFileSync(path, Buffer.from('(1 1 1)\n(2 2 \xff)\n', 'latin1'));
    assert.equal(
      report(() => new LinkStorage(path)),
      `Failed to read ${path} at line 2\n\nCaused by:\n    stream did not contain valid UTF-8`,
    );
  });

  it('reports invalid name metadata', () => {
    const path = database('(1 1 1)\n# link-cli:name:1 x ~1{zz}\n');
    assert.equal(
      report(() => new LinkStorage(path)),
      `Invalid name metadata in ${path} at line 2\n\nCaused by:\n    invalid name metadata address`,
    );
  });

  it('reports a database it cannot create or replace', () => {
    const directory = scratchDirectory();
    const missing = join(directory, 'missing', 'db.links');
    const storage = new LinkStorage(missing);
    storage.create(1, 1);
    assert.equal(
      report(() => storage.save()),
      `Failed to create database: ${missing}.saving\n\nCaused by:\n    No such file or directory (os error 2)`,
    );
    const path = join(directory, 'db.links');
    const replaced = new LinkStorage(path);
    mkdirSync(path);
    writeFileSync(join(path, 'kept'), '');
    assert.match(
      report(() => replaced.save()),
      new RegExp(`^Failed to replace database: ${path}\n\nCaused by:\n    `),
    );
    assert.equal(existsSync(join(path, 'kept')), true);
  });
});

describe('LinkStorage links', () => {
  it('stores frozen links', () => {
    const storage = LinkStorage.inMemory();
    storage.create(1, 2);
    assert.deepEqual(storage.get(1), link(1, 1, 2));
  });
});

describe('LinkStorage advisory locks', () => {
  const lockable = { skip: lockBackend().unavailable };
  const NOT_BACKED = {
    name: 'LockError',
    message: 'Lock error: this doublets storage is not backed by a known file',
  };

  it('has no lock file in memory, as require_lock_path refuses', () => {
    const storage = LinkStorage.inMemory();
    assert.throws(() => storage.lockShared(), NOT_BACKED);
    assert.throws(() => storage.lockExclusive(), NOT_BACKED);
    assert.equal(storage.heldLock(), null);
    storage.close();
    for (const open of ['openShared', 'openExclusive', 'tryOpenExclusive']) {
      assert.throws(() => LinkStorage[open](''), NOT_BACKED);
    }
  });

  it('holds an exclusive lock until closed, across reloads', lockable, () => {
    const path = database('1 1 1\n');
    const stderr = capture();
    const storage = LinkStorage.openExclusive(path, { trace: true, stderr });
    assert.deepEqual(traceLines(stderr), [`[TRACE] Loaded 1 links from ${path}`]);
    assert.equal(storage.heldLock().path(), `${path}.lock`);
    assert.equal(storage.heldLock().mode(), LockMode.Exclusive);
    assert.equal(LinkStorage.tryOpenExclusive(path), null);
    assert.equal(FileLock.tryAcquire(`${path}.lock`, LockMode.Shared), null);
    const held = storage.heldLock();
    storage.reloadFromDisk();
    assert.equal(storage.heldLock(), held);
    storage.close();
    assert.equal(storage.heldLock(), null);
    assert.equal(held.isReleased(), true);
    const again = LinkStorage.tryOpenExclusive(path);
    assert.deepEqual(rows(again), [[1, 1, 1]]);
    again.close();
  });

  it('lets shared holders read together and keeps writers out', lockable, () => {
    const path = database('');
    const first = LinkStorage.openShared(path);
    const second = LinkStorage.openShared(path);
    assert.equal(first.heldLock().mode(), LockMode.Shared);
    assert.equal(LinkStorage.tryOpenExclusive(path), null);
    first.close();
    second.close();
    const plain = new LinkStorage(path);
    assert.equal(plain.heldLock(), null);
    const exclusive = plain.lockExclusive();
    assert.equal(exclusive.path(), `${path}.lock`);
    assert.equal(LinkStorage.tryOpenExclusive(path), null);
    exclusive.release();
    const shared = plain.lockShared();
    assert.equal(shared.mode(), LockMode.Shared);
    shared.release();
  });

  it('releases the lock when the database cannot load', lockable, () => {
    const path = join(scratchDirectory(), 'db.links');
    mkdirSync(path);
    assert.throws(() => LinkStorage.openExclusive(path), {
      message: `Failed to read ${path} at line 1`,
    });
    const lock = FileLock.tryAcquire(`${path}.lock`, LockMode.Exclusive);
    assert.notEqual(lock, null);
    lock.release();
  });
});
