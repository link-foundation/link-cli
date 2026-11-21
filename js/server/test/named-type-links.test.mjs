// Tests of the `NamedTypeLinks` base class (src/named-type-links.js), the
// storage contract of rust/src/named_type_links.rs: the methods a layer must
// implement fail loudly when it does not, the default methods go through
// them, and the shared helpers (`escapeLinoReference`, `queryLinks`,
// `writeLinoOutput`) behave as their Rust defaults.
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, describe, it } from 'node:test';
import { InvalidFormatError } from '../src/errors.js';
import { link } from '../src/link.js';
import { escapeLinoReference, NamedTypeLinks } from '../src/named-type-links.js';
import { decodeReferenceLiteral } from '../src/notation.js';

const dir = mkdtempSync(join(tmpdir(), 'clink-named-type-links-'));
after(() => rmSync(dir, { recursive: true, force: true }));

/** A layer that implements nothing. */
class Bare extends NamedTypeLinks {}

/** A read-only layer over a fixed list of links, without names. */
class Listed extends NamedTypeLinks {
  constructor(links) {
    super();
    this.links = links;
  }

  allLinks() {
    return [...this.links];
  }

  getName() {
    return null;
  }
}

const missing = (method) => ({ name: 'Error', message: `Bare does not implement ${method}` });

describe('the required methods', () => {
  it('fail with the name of the layer and of the method when not implemented', () => {
    const bare = new Bare();
    const calls = [
      ['create', () => bare.create(1, 1)],
      ['ensureCreated', () => bare.ensureCreated(5)],
      ['getLink', () => bare.getLink(1)],
      ['exists', () => bare.exists(1)],
      ['updateObserved', () => bare.updateObserved(1, 1, 1, () => {})],
      ['deleteObserved', () => bare.deleteObserved(1, () => {})],
      ['allLinks', () => bare.allLinks()],
      ['search', () => bare.search(1, 1)],
      ['getOrCreate', () => bare.getOrCreate(1, 1)],
      ['getName', () => bare.getName(1)],
      ['setName', () => bare.setName(1, 'a')],
      ['getByName', () => bare.getByName('a')],
      ['removeName', () => bare.removeName(1)],
    ];
    for (const [method, call] of calls) assert.throws(call, missing(method), method);
  });

  it('are what the default methods are written in terms of', () => {
    const bare = new Bare();
    const defaults = [
      ['create', () => bare.tryCreate(1, 1)],
      ['ensureCreated', () => bare.tryEnsureCreated(5)],
      ['exists', () => bare.referenceExists(1)],
      ['updateObserved', () => bare.update(1, 2, 2)],
      ['deleteObserved', () => bare.delete(1)],
      ['create', () => bare.nextCreatedAddresses(1)],
      ['search', () => bare.tryGetOrCreate(1, 1)],
      ['getByName', () => bare.getOrCreateNamed('a')],
      ['getName', () => bare.formatReference(1)],
      ['allLinks', () => bare.linoLines()],
      ['getLink', () => bare.formatStructure(1)],
    ];
    for (const [method, call] of defaults) assert.throws(call, missing(method), method);
    // The address check of `try_ensure_created` comes before the store is asked.
    assert.throws(() => bare.tryEnsureCreated(0), InvalidFormatError);
  });

  it('save is a no-op by default and the optional hooks answer null', () => {
    const bare = new Bare();
    assert.equal(bare.save(), undefined);
    assert.equal(bare.queryLimits(), null);
    assert.equal(bare.claims(1), null);
    assert.equal(bare.reserveQueryAllocations(null, null, null), null);
  });
});

describe('queryLinks', () => {
  const store = new Listed([link(3, 1, 2), link(1, 1, 1), link(2, 1, 2), link(4, 2, 1)]);
  const indexes = (links) => links.map((value) => value.index);

  it('matches every link without a pattern, ordered by address', () => {
    assert.deepEqual(store.queryLinks(), [
      link(1, 1, 1),
      link(2, 1, 2),
      link(3, 1, 2),
      link(4, 2, 1),
    ]);
  });

  it('treats null as any value and matches each given part exactly', () => {
    assert.deepEqual(indexes(store.queryLinks(2)), [2]);
    assert.deepEqual(indexes(store.queryLinks(null, 1)), [1, 2, 3]);
    assert.deepEqual(indexes(store.queryLinks(null, null, 1)), [1, 4]);
    assert.deepEqual(indexes(store.queryLinks(null, 1, 2)), [2, 3]);
    assert.deepEqual(indexes(store.queryLinks(3, 1, 2)), [3]);
    assert.deepEqual(store.queryLinks(3, 2, 1), []);
    assert.deepEqual(store.queryLinks(9), []);
  });
});

describe('escapeLinoReference', () => {
  it('quotes digits that would read as an address', () => {
    assert.equal(escapeLinoReference(''), '');
    assert.equal(escapeLinoReference('12'), "'12'");
    assert.equal(escapeLinoReference('+7'), "'+7'");
    assert.equal(escapeLinoReference('plain'), 'plain');
    assert.equal(escapeLinoReference('a b'), "'a b'");
  });

  it('writes a name with both quotes as a reference literal', () => {
    // The Rust clink prints the name of `() ((""a'b"c"": 1 1))` the same way.
    const literal = escapeLinoReference(`a'b"c`);
    assert.equal(literal, '~1{6127622263}');
    assert.equal(decodeReferenceLiteral(literal), `a'b"c`);
  });
});

describe('writeLinoOutput', () => {
  const store = new Listed([link(2, 1, 2), link(1, 1, 1)]);

  it('writes every link, one per line, ordered by address', () => {
    const path = join(dir, 'out.lino');
    store.writeLinoOutput(path);
    assert.equal(readFileSync(path, 'utf8'), '(1: 1 1)\n(2: 1 2)\n');
  });

  it('reports a file it cannot create with the path and the cause', () => {
    // The Rust clink: `Error: Failed to create LiNo output: PATH`, then
    // `Caused by: No such file or directory`.
    const path = join(dir, 'missing', 'out.lino');
    assert.throws(
      () => store.writeLinoOutput(path),
      (error) => {
        assert.equal(error.message, `Failed to create LiNo output: ${path}`);
        assert.equal(error.cause.code, 'ENOENT');
        return true;
      },
    );
  });
});
