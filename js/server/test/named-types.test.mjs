// The names sidecar layer over a data store. The traces and files are the
// ones the Rust clink writes for `() ((x: x x))` and `((x: x x)) ()`.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { makeNamesDatabaseFilename, NamedTypesDecorator } from '../src/named-types.js';
import { capture, scratchDirectory, traceLines } from './helpers/storage.mjs';

const rows = (values) => values.map((value) => [value.index, value.source, value.target]);
const lines = (path) => readFileSync(path, 'utf8').split('\n').slice(0, -1);
const decoratorLines = (stderr) =>
  traceLines(stderr).filter((line) => line.startsWith('[TRACE] NamedTypesDecorator'));

function opened(dataText = null, options = {}) {
  const directory = scratchDirectory();
  const path = join(directory, 'db.links');
  if (dataText !== null) writeFileSync(path, dataText);
  const stderr = capture();
  const store = NamedTypesDecorator.open(path, { stderr, ...options });
  return { directory, path, names: join(directory, 'db.names.links'), stderr, store };
}

describe('names database filename', () => {
  it('follows Rust Path::parent and Path::file_stem', () => {
    for (const [path, names] of [
      ['db.links', 'db.names.links'],
      ['dir/db.links', 'dir/db.names.links'],
      ['/db.links', '/db.names.links'],
      ['/', '.names.links'],
      ['', '.names.links'],
      ['.', '.names.links'],
      ['..', '.names.links'],
      ['a/..', 'a/.names.links'],
      ['a/b/', 'a/b.names.links'],
      ['a/b/.', 'a/b.names.links'],
      ['a//b.c.links', 'a/b.c.names.links'],
      ['./db.links', './db.names.links'],
      ['.hidden', '.hidden.names.links'],
      ['a.b.c', 'a.b.names.links'],
      ['/a/', '/a.names.links'],
      ['a/./', 'a.names.links'],
      ['dir//', 'dir.names.links'],
      ['x/y.', 'x/y.names.links'],
      ['../db', '../db.names.links'],
    ]) {
      assert.equal(makeNamesDatabaseFilename(path), names, path);
    }
  });
});

describe('named types decorator', () => {
  it('names a link and removes the name with it', () => {
    const { path, names, stderr, store } = opened(null, { trace: true });
    const id = store.create(0, 0);
    store.setName(id, 'x');
    store.update(id, id, id);
    store.save();
    assert.deepEqual(decoratorLines(stderr), [
      '[TRACE] NamedTypesDecorator set_name for link 1: x',
      '[TRACE] NamedTypesDecorator get_by_name for name x',
      '[TRACE] NamedTypesDecorator remove_name for link 1',
    ]);
    assert.equal(readFileSync(path, 'utf8'), '(1 1 1)\n');
    assert.equal(lines(names).length, 97);
    assert.deepEqual(lines(names).slice(-3), ['(95 4 94)', '(96 6 95)', '(97 4294967295 96)']);
    assert.equal(store.getName(1), 'x');
    assert.equal(store.getByName('x'), 1);
    assert.deepEqual(store.logicalNames(), new Map([[1, 'x']]));

    stderr.text = '';
    assert.deepEqual(store.delete(1), { index: 1, source: 1, target: 1 });
    assert.deepEqual(traceLines(stderr), [
      '[TRACE] Updating link 1 from (1 1) to (0 0)',
      '[TRACE] Deleted link: (1 0 0)',
      '[TRACE] NamedTypesDecorator remove_name for link 1',
      '[TRACE] Updating link 97 from (4294967295 96) to (0 0)',
      '[TRACE] Deleted link: (97 0 0)',
      '[TRACE] Updating link 96 from (6 95) to (0 0)',
      '[TRACE] Deleted link: (96 0 0)',
    ]);
    store.save();
    assert.equal(lines(names).length, 95);
    assert.equal(store.getByName('x'), null);
  });

  it('moves a name to the link it is set on', () => {
    const { store } = opened('(1 1 1)\n(2 2 2)\n');
    store.setName(1, 'a');
    store.setName(2, 'a');
    store.setName(2, 'a');
    assert.equal(store.getName(1), null);
    assert.equal(store.getByName('a'), 2);
  });

  it('removes the names of the links a merge deletes', () => {
    const { store } = opened('(1 1 1)\n(2 2 2)\n(3 1 2)\n(4 2 1)\n(5 1 4)\n');
    store.setName(4, 'four');
    store.setName(5, 'five');
    const changes = [];
    const before = store.updateObserved(4, 1, 2, (left, right) =>
      changes.push(rows([left, right])),
    );
    assert.deepEqual(rows([before]), [[4, 2, 1]]);
    assert.deepEqual(changes, [
      [
        [5, 1, 4],
        [5, 1, 3],
      ],
      [
        [4, 2, 1],
        [4, 0, 0],
      ],
      [
        [4, 0, 0],
        [0, 0, 0],
      ],
    ]);
    assert.equal(store.getByName('four'), null);
    assert.equal(store.getByName('five'), 5);
    store.updateObserved(5, 3, 3, () => {});
    assert.equal(store.getName(5), 'five');
  });

  it('removes the names of the links a delete cascades to', () => {
    const { store } = opened('(1 1 1)\n(2 1 2)\n(3 3 3)\n');
    store.setName(2, 'two');
    store.setName(3, 'three');
    const changes = [];
    store.deleteObserved(1, (left, right) => changes.push(rows([left, right])));
    assert.equal(changes.length, 4);
    assert.deepEqual(store.logicalNames(), new Map([[3, 'three']]));
  });

  it('forwards to the data store', () => {
    const { store } = opened('(1 1 1)\n(2 1 2)\n');
    assert.deepEqual(rows([store.get(2), store.getLink(2)]), [
      [2, 1, 2],
      [2, 1, 2],
    ]);
    assert.equal(store.get(9), null);
    assert.equal(store.exists(2), true);
    assert.equal(store.exists(9), false);
    assert.deepEqual(rows(store.all()), rows(store.allLinks()));
    assert.deepEqual(rows(store.query(null, 1)), [
      [1, 1, 1],
      [2, 1, 2],
    ]);
    assert.deepEqual(rows(store.queryLinks(2)), [[2, 1, 2]]);
    assert.equal(store.search(1, 2), 2);
    assert.equal(store.getOrCreate(2, 2), 3);
    assert.deepEqual(store.ensureCreated(5), 5);
    assert.deepEqual(store.pinnedTypes(2), [1, 2]);
    assert.throws(() => store.pinnedTypes(3), {
      message: 'Unexpected link found at address 3. Expected: (3: 1 3), Found: (3: 2 2).',
    });
  });

  it('keeps a names database at the path it is given', () => {
    const directory = scratchDirectory();
    const namesDatabasePath = join(directory, 'other.links');
    const store = NamedTypesDecorator.open(join(directory, 'db.links'), {
      namesDatabasePath,
      stderr: capture(),
    });
    store.setName(store.create(0, 0), 'n');
    store.save();
    const reopened = NamedTypesDecorator.open(join(directory, 'db.links'), { namesDatabasePath });
    assert.equal(reopened.getName(1), 'n');
    assert.equal(existsSync(join(directory, 'db.names.links')), false);
  });

  it('previews a query on an untraced copy', () => {
    const { stderr, store } = opened('(1 1 1)\n', { trace: true });
    store.setName(1, 'one');
    stderr.text = '';
    const copy = store.queryCopy();
    copy.setName(copy.create(1, 1), 'copy');
    assert.equal(stderr.text, '');
    assert.equal(copy.getName(1), 'one');
    assert.equal(store.exists(2), false);
    assert.equal(store.getByName('copy'), null);
    const processor = {
      preview(scratch, restriction, substitution) {
        assert.deepEqual([restriction, substitution], ['r', 's']);
        scratch.create(1, 1);
        scratch.create(1, 1);
      },
    };
    assert.deepEqual(store.reserveQueryAllocations(processor, 'r', 's'), new Set([1, 2, 3]));
    assert.equal(store.exists(2), false);
  });

  it('traces the lookup indexes of both stores', () => {
    const { stderr, store } = opened('(1 1 1)\n', { trace: true });
    stderr.text = '';
    assert.equal(store.withIndexedLookups(), store);
    assert.deepEqual(traceLines(stderr), [
      '[TRACE] IndexedLinksDecorator enabled for 1 links',
      '[TRACE] IndexedLinksDecorator enabled for 0 links',
    ]);
  });

  it('sees and reloads changes other processes save', () => {
    const first = opened('(1 1 1)\n');
    first.store.setName(1, 'one');
    first.store.save();
    const { path, names } = first;
    const store = NamedTypesDecorator.open(path);
    assert.equal(store.getName(1), 'one');
    assert.equal(store.hasExternalChanges(), false);
    writeFileSync(names, '(1 1 1)\n');
    assert.equal(store.hasExternalChanges(), true);
    store.reloadFromDisk();
    assert.equal(store.hasExternalChanges(), false);
    assert.equal(store.getName(1), null);
    writeFileSync(path, '(1 1 1)\n(2 2 2)\n');
    assert.equal(store.hasExternalChanges(), true);
    store.reloadFromDisk();
    assert.equal(store.exists(2), true);
  });
});
