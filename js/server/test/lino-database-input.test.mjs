// `--in`: importing a LiNo file into a store. The saved databases, names and
// error texts are the ones the Rust clink writes and reports for the same
// input.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { formatErrorReport } from '../src/error-report.js';
import { LinkStorage } from '../src/link-storage.js';
import { importLinoFile, importLinoText, updateLink } from '../src/lino-database-input.js';
import { NamedTypesDecorator } from '../src/named-types.js';
import { scratchDirectory } from './helpers/storage.mjs';

function imported(text) {
  const directory = scratchDirectory();
  const input = join(directory, 'input.lino');
  writeFileSync(input, text);
  const databasePath = join(directory, 'db.links');
  const storage = NamedTypesDecorator.open(databasePath);
  importLinoFile(storage, input);
  return {
    storage,
    database: readFileSync(databasePath, 'utf8'),
    names: [...storage.logicalNames()],
  };
}

const rows = (storage) =>
  storage.allLinks().map((value) => [value.index, value.source, value.target]);

describe('importLinoFile', () => {
  it('writes what the Rust clink writes', () => {
    for (const [text, database, names] of [
      ['(3: 1 2)', '# unused: 2 1\n(3 1 2)\n', []],
      [
        `(child: 'its' "say hi")`,
        '(1 1 1)\n(2 2 2)\n(3 1 2)\n',
        [
          [1, 'its'],
          [2, 'say hi'],
          [3, 'child'],
        ],
      ],
      ['(0: (5 6) (7 (8 9)))', '(1 5 6)\n(2 8 9)\n(3 7 2)\n(4 1 3)\n', [[4, '0']]],
      ['(5: child 3)', '# unused: 4 3 2\n(1 1 1)\n(5 1 3)\n', [[1, 'child']]],
      [
        `('a b': "c" 'd')`,
        '(1 1 1)\n(2 2 2)\n(3 1 2)\n',
        [
          [1, 'c'],
          [2, 'd'],
          [3, 'a b'],
        ],
      ],
      ['(7: 1 1)', '# unused: 6 5 4 3 2 1\n(7 1 1)\n', []],
    ]) {
      const result = imported(text);
      assert.equal(result.database, database, text);
      assert.deepEqual(result.names, names, text);
    }
  });

  it('reports a file it cannot read as the Rust clink does', () => {
    const directory = scratchDirectory();
    const missing = join(directory, 'missing.lino');
    const storage = NamedTypesDecorator.open(join(directory, 'db.links'));
    assert.throws(
      () => importLinoFile(storage, missing),
      (error) =>
        formatErrorReport(error) ===
        `Failed to read LiNo input: ${missing}\n\nCaused by:\n    No such file or directory (os error 2)`,
    );
    const invalid = join(directory, 'invalid.lino');
    writeFileSync(invalid, Buffer.from([0x28, 0xff, 0x29]));
    assert.throws(
      () => importLinoFile(storage, invalid),
      (error) =>
        formatErrorReport(error) ===
        `Failed to read LiNo input: ${invalid}\n\nCaused by:\n    stream did not contain valid UTF-8`,
    );
    assert.equal(existsSync(join(directory, 'db.links')), false);
  });
});

describe('importLinoText', () => {
  it('requires an integer depth limit in range before writing', () => {
    const storage = LinkStorage.inMemory();
    for (const maxDepth of [0, 129, 1.5, '64']) {
      assert.throws(() => importLinoText(storage, '(new: a b)', { maxDepth }), RangeError);
      assert.deepEqual(rows(storage), []);
    }
    importLinoText(storage, '(new: a b)', { maxDepth: 1 });
    assert.notEqual(storage.getByName('new'), null);
  });

  it('rejects what the Rust clink rejects', () => {
    for (const [text, message] of [
      ['(1 2 3)', 'LiNo import supports links with exactly two values'],
      [`(child: 'it\\'s' 2)`, 'LiNo import supports links with exactly two values'],
      [
        '(1: (2 3 4) 5)',
        'LiNo import references must be scalar values or nested links with exactly two values',
      ],
      ['(1: 2 ())', 'LiNo import references must have a value'],
      [`(1: '' 2)`, 'LiNo import references must have a value'],
      ['(1 2)', 'Top-level LiNo import links must have an index or name'],
    ]) {
      assert.throws(() => importLinoText(LinkStorage.inMemory(), text), { message }, text);
    }
  });

  it('leaves the links and names unchanged when the input is invalid', () => {
    for (const populated of [false, true]) {
      for (const invalid of [
        '(c: c',
        '(c: c)',
        '(c: c c c)',
        '(x y)',
        '(c: (nested: a b) (bad: a))',
        '(c: a ())',
      ]) {
        const storage = LinkStorage.inMemory();
        if (populated) importLinoText(storage, '(a: a a)\n(b: b b)\n(existing: a b)');
        const before = storage.databaseText();
        assert.throws(() => importLinoText(storage, `(a: b a)\n(new: fresh fresh)\n${invalid}`));
        assert.equal(storage.databaseText(), before, invalid);
        assert.equal(storage.getByName('new'), null);
        assert.equal(storage.getByName('fresh'), null);
      }
    }
  });

  it('imports named and anonymous nested links', () => {
    const storage = LinkStorage.inMemory();
    importLinoText(storage, '(a: a a)\n(b: b b)\n(child: (nested: a b) (b a))');
    const child = storage.getLink(storage.getByName('child'));
    assert.equal(child.source, storage.getByName('nested'));
    const target = storage.getLink(child.target);
    assert.equal(target.source, storage.getByName('b'));
    assert.equal(target.target, storage.getByName('a'));
  });

  it('writes numbered links at their indexes', () => {
    const storage = LinkStorage.inMemory();
    importLinoText(storage, '\n        (1: 1 1)\n        (2: 1 2)\n        (3: 2 1)\n        ');
    assert.deepEqual(rows(storage), [
      [1, 1, 1],
      [2, 1, 2],
      [3, 2, 1],
    ]);
  });

  it('creates named references as points', () => {
    const storage = LinkStorage.inMemory();
    importLinoText(storage, '(child: father mother)');
    const [father, mother, child] = ['father', 'mother', 'child'].map((name) =>
      storage.getByName(name),
    );
    assert.deepEqual(storage.getLink(father), { index: father, source: father, target: father });
    assert.deepEqual(storage.getLink(mother), { index: mother, source: mother, target: mother });
    assert.deepEqual(storage.getLink(child), { index: child, source: father, target: mother });
  });

  it('treats out-of-range numbers as names', () => {
    const storage = LinkStorage.inMemory();
    importLinoText(storage, '(child: 4294967296 2)');
    const number = storage.getByName('4294967296');
    const child = storage.getByName('child');
    assert.deepEqual(storage.getLink(number), { index: number, source: number, target: number });
    assert.deepEqual(storage.getLink(child), { index: child, source: number, target: 2 });
  });

  it('strips the quotes and the trailing colons of identifiers', () => {
    const storage = LinkStorage.inMemory();
    importLinoText(storage, `(q: 'x:' y)\n("z": x 'y')`);
    assert.deepEqual(rows(storage), [
      [1, 1, 1],
      [2, 2, 2],
      [3, 1, 2],
    ]);
    // `z` merges into `q`, the link with its pair, and loses its name.
    assert.deepEqual(
      ['q', 'x', 'x:', 'y', 'z'].map((name) => storage.getByName(name)),
      [3, 1, null, 2, null],
    );
  });

  it('unquotes a quoted identifier the parser leaves quoted', () => {
    const storage = LinkStorage.inMemory();
    const point = (name) => `(${name}: ${name} ${name})`;
    importLinoText(
      storage,
      [`"'a'"`, `'"b"'`, `"'it\\'s'"`, `'"say \\"hi\\""'`].map(point).join('\n'),
    );
    assert.deepEqual(
      ['a', 'b', "it's", 'say "hi"'].map((name) => storage.getByName(name)),
      [1, 2, 3, 4],
    );
    assert.deepEqual(rows(storage), [
      [1, 1, 1],
      [2, 2, 2],
      [3, 3, 3],
      [4, 4, 4],
    ]);
  });

  it('keeps a link that already holds the pair', () => {
    const storage = LinkStorage.inMemory();
    importLinoText(storage, '(1: 1 1)\n(1: 1 1)');
    assert.deepEqual(rows(storage), [[1, 1, 1]]);
  });
});

describe('updateLink', () => {
  it('creates a missing link and updates a link the store cannot read back', () => {
    const calls = [];
    const storage = {
      exists: () => false,
      tryEnsureCreated: (index) => calls.push(['ensure', index]),
      getLink: () => null,
      update: (...args) => calls.push(['update', ...args]),
    };
    updateLink(storage, 4, 1, 2);
    assert.deepEqual(calls, [
      ['ensure', 4],
      ['update', 4, 1, 2],
    ]);
  });
});
