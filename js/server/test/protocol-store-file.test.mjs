// The store archive (src/protocol-store-file.js): ports the `archive.rs` cases
// of rust/tests/store_archive_tests.rs, store_archive_portable_tests.rs and
// store_archive_stream_tests.rs, with the vectors of
// rust/tests/common/store_archive.rs.
//
// The hex vectors are the bytes the Rust `clink` writes (built with
// `cargo build`, run from the repository root in an empty directory):
//
//   C=rust/target/debug/clink
//   # ARCHIVE: links 1, 3 and 4 named a, é and ab, a hole at 2.
//   $C --db g.links "() ((a: a a))"
//   $C --db g.links "() ((2: 2 2))"
//   $C --db g.links "() ((é: é é))"
//   $C --db g.links --auto-create-missing-references "() ((ab: a é))"
//   $C --db g.links "((2: 2 2)) ()" --after --binary-output g.bin
//   # FAMILY: names of several code points, one of them outside ASCII.
//   $C --db s.links --auto-create-missing-references \
//     "() ((child: father mother) (2: 2 1) (雪: 雪 child))" --binary-output s.bin
//   xxd -p g.bin s.bin
//
// `node bin/clink.js` writes the same bytes for both (`cmp` finds no
// difference). Rust reads and writes archives through `io::Read` and
// `io::Write`, so its stream tests also inject read and write faults; the
// JavaScript archive is a Buffer, so those faults have no counterpart and the
// truncated prefixes stand for them. Rust's round trip through a
// `RemoteLinks` has none either: `RemoteLinks` is asynchronous here and the
// archive functions are synchronous.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { link } from '../src/link.js';
import { main } from '../src/main.js';
import { NamedTypesDecorator } from '../src/named-types.js';
import { LinksPacket, external } from '../src/protocol-binary-packet.js';
import { ProtocolError } from '../src/protocol-errors.js';
import {
  exportStore,
  exportStoreFile,
  importStore,
  importStoreFile,
} from '../src/protocol-store-file.js';
import { capture, scratchDirectory } from './helpers/storage.mjs';

const ARCHIVE =
  '12 02 20 01 24 01 02 01 01 03 03 01 03 13 02 21 02 30 01 ff ff 9f ff fd ff 17 ff fc 9f 9e';
const FAMILY =
  '12 01 20 04 01 01 02 01 01 02 04 03 13 03 70 02 60 01 22 01 ff 9a 9f 8c 98 9b 8e fe 93 91 ' +
  '8c 98 9b 8e fd 9d 98 97 94 9c fc ff ff ff 16 69 ff ff';
const EMPTY = '10 00 11 00';

const bytes = (hex) => Buffer.from(hex.replaceAll(' ', ''), 'hex');
const hex = (buffer) => [...buffer].map((byte) => byte.toString(16).padStart(2, '0')).join(' ');

/** A fresh named store in a scratch directory. */
function store() {
  return NamedTypesDecorator.open(join(scratchDirectory(), 'store.links'));
}

/** Links `1: 1 1` named `a`, `3: 3 3` named `é` and `4: 1 3` named `ab`, with a hole at 2. */
function fill(storage) {
  for (let i = 0; i < 3; i += 1) {
    const id = storage.create(0, 0);
    storage.update(id, id, id);
  }
  assert.equal(storage.create(1, 3), 4);
  storage.delete(2);
  storage.setName(1, 'a');
  storage.setName(3, 'é');
  storage.setName(4, 'ab');
}

const links = (storage) => [...storage.allLinks()].sort((a, b) => a.index - b.index);
const names = (storage) => [1, 2, 3, 4].map((id) => [id, storage.getName(id) ?? null]);
const snapshot = (storage) => ({ links: links(storage), names: names(storage) });

/** `packet`: one links packet with packed widths; `links` holds `[address, references]`. */
const packet = (externalReferences, entries) =>
  LinksPacket.pack(
    externalReferences,
    entries.map(([address, references]) => [BigInt(address), references]),
    true,
  ).toBytes();
const doublet = (address, source, target) => [address, [BigInt(source), BigInt(target)]];
const noNames = () => packet(true, []);
const archiveOf = (...packets) => Buffer.concat(packets);

/** The message of the failed import of `archive` into a fresh store. */
function importError(archive) {
  const target = store();
  let failure = null;
  try {
    importStore(target, archive);
  } catch (error) {
    failure = error;
  }
  assert.ok(failure instanceof ProtocolError, `${hex(archive)} should not import`);
  assert.equal(failure.kind, 'malformed');
  return failure.message;
}

describe('store archive', () => {
  it('a store exports to the golden archive', () => {
    const source = store();
    fill(source);
    assert.equal(hex(exportStore(source)), ARCHIVE);
  });

  it('an imported archive restores links, holes and names', () => {
    const source = store();
    fill(source);
    const target = store();
    importStore(target, bytes(ARCHIVE));
    assert.deepEqual(links(target), links(source));
    assert.equal(target.exists(2), false, 'the hole at 2 stays a hole');
    assert.deepEqual(names(target), names(source));
    assert.equal(hex(exportStore(target)), ARCHIVE);
  });

  it('reads the archive the Rust clink writes for several names', () => {
    const target = store();
    importStore(target, bytes(FAMILY));
    assert.deepEqual(links(target), [link(1, 1, 1), link(2, 2, 1), link(3, 1, 2), link(4, 4, 3)]);
    assert.deepEqual(
      links(target).map((value) => target.getName(value.index)),
      ['father', 'mother', 'child', '雪'],
    );
    assert.equal(hex(exportStore(target)), FAMILY);
  });

  it('an empty store is two empty packets', () => {
    assert.equal(hex(exportStore(store())), EMPTY);
    const imported = store();
    importStore(imported, bytes(EMPTY));
    assert.deepEqual(imported.allLinks(), []);
  });

  it('an archive imports into a store that already has the links', () => {
    const target = store();
    fill(target);
    importStore(target, bytes(ARCHIVE));
    assert.equal(hex(exportStore(target)), ARCHIVE);
  });

  it('a truncated archive is rejected', () => {
    assert.match(importError(Buffer.alloc(0)), /ends before its links/);
    assert.match(importError(bytes('10 00')), /ends before its names/);
    assert.match(importError(bytes('10 00 11')), /malformed/);
  });

  it('trailing bytes are rejected', () => {
    assert.match(importError(bytes('10 00 11 00 10')), /trailing bytes/);
  });

  it('a link must be a doublet of link addresses', () => {
    const tripletLink = [1, [1n, 1n, 1n]];
    const externalLink = [1, [external(1), 1n]];
    for (const entry of [tripletLink, externalLink]) {
      const archive = archiveOf(packet(true, [entry]), noNames());
      assert.match(importError(archive), /is not a doublet of link addresses/);
    }
  });

  it('addresses must fit a 32-bit store', () => {
    const tooLarge = 0xffffffffn + 1n;
    for (const entry of [doublet(tooLarge, 1, 1), doublet(1, tooLarge, 1)]) {
      const archive = archiveOf(packet(false, [entry]), noNames());
      assert.match(importError(archive), /does not fit a 32-bit store/);
    }
  });

  it('names must be an address and code points', () => {
    const cases = [
      [[1n], 'only external values'],
      [[external(1), external(0xd800)], 'invalid code point 55296'],
      [[external(1), external(0xdfff)], 'invalid code point 57343'],
      [[external(1), external(1n << 40n)], 'invalid code point'],
      [[external(1n << 40n)], 'does not fit a 32-bit store'],
    ];
    for (const [name, message] of cases) {
      const archive = archiveOf(packet(false, [doublet(1, 1, 1)]), packet(true, [[1, name]]));
      assert.ok(importError(archive).includes(message), message);
    }
  });

  it('an empty name is kept', () => {
    const archive = archiveOf(
      packet(false, [doublet(1, 1, 1)]),
      packet(true, [[1, [external(1)]]]),
    );
    const imported = store();
    importStore(imported, archive);
    assert.equal(imported.getName(1), '');
    assert.equal(hex(exportStore(imported)), hex(archive));
  });

  it('archive file failures name the file', () => {
    const directory = scratchDirectory();
    const source = store();
    fill(source);
    const before = snapshot(source);
    const missing = join(directory, 'missing', 'store.bin');
    for (const [operation, action] of [
      ['write', () => exportStoreFile(source, missing)],
      ['read', () => importStoreFile(source, missing)],
    ]) {
      assert.throws(action, (error) => {
        assert.equal(error.message, `Failed to ${operation} the store archive: ${missing}`);
        assert.equal(error.cause.code, 'ENOENT');
        return true;
      });
      assert.deepEqual(snapshot(source), before);
    }
  });

  it('the file helpers write and read the archive', () => {
    const path = join(scratchDirectory(), 'store.bin');
    const source = store();
    fill(source);
    exportStoreFile(source, path);
    assert.equal(hex(readFileSync(path)), ARCHIVE);
    const target = store();
    importStoreFile(target, path);
    assert.deepEqual(snapshot(target), snapshot(source));
  });
});

/** `following_import`: a complete import still works, and so does a later write. */
function followingImport(storage, archive) {
  importStore(storage, archive);
  assert.deepEqual(exportStore(storage), archive);
  assert.equal(storage.exists(2), false);
  const following = storage.tryCreate(0, 0);
  storage.setName(following, 'following 雪=\0');
  storage.save();
  assert.equal(storage.getName(following), 'following 雪=\0');
}

describe('store archive input that ends early or late', () => {
  it('every truncated prefix and trailing byte preserves existing rows and names', () => {
    const target = store();
    const seed = target.tryCreate(1, 1);
    target.setName(seed, 'keep 雪');
    const before = snapshot(target);
    const archive = bytes(ARCHIVE);
    assert.equal(archive.length, 30);
    for (let end = 0; end < archive.length; end += 1) {
      assert.throws(() => importStore(target, archive.subarray(0, end)), ProtocolError, `${end}`);
      assert.deepEqual(snapshot(target), before, `${end}`);
    }
    for (const byte of [0, 0x10, 0xff]) {
      const extra = Buffer.concat([archive, Buffer.from([byte])]);
      assert.throws(() => importStore(target, extra), ProtocolError, `${byte}`);
      assert.deepEqual(snapshot(target), before, `${byte}`);
    }
    followingImport(target, archive);
  });

  it('repeated imports keep the archive bytes', () => {
    const target = store();
    const archive = bytes(ARCHIVE);
    for (let i = 0; i < 2; i += 1) {
      importStore(target, archive);
      assert.deepEqual(exportStore(target), archive);
      assert.equal(target.exists(2), false);
      assert.equal(target.getName(1), 'a');
      assert.equal(target.getName(3), 'é');
      assert.equal(target.getName(4), 'ab');
    }
  });
});

/** Runs `clink` in-process on `database`: its status, standard output and standard error. */
async function clink(database, ...args) {
  const [stdout, stderr] = [capture(), capture()];
  const status = await main(['--db', database, ...args], { stdout, stderr });
  return { status, stdout: stdout.text, stderr: stderr.text };
}

describe('store archive options of clink', () => {
  it('the cli copies a store through an archive', async () => {
    const directory = scratchDirectory();
    const archive = join(directory, 'store.bin');
    const sourceLino = join(directory, 'source.lino');
    const targetLino = join(directory, 'target.lino');
    const source = await clink(
      join(directory, 'source.links'),
      '--auto-create-missing-references',
      '() ((child: father mother) (2: 2 1))',
      '--export-binary',
      archive,
      '--out',
      sourceLino,
    );
    assert.equal(source.status, 0, source.stderr);
    const target = await clink(
      join(directory, 'target.links'),
      '--import-binary',
      archive,
      '--out',
      targetLino,
    );
    assert.equal(target.status, 0, target.stderr);
    const lino = readFileSync(targetLino, 'utf8');
    assert.equal(lino, readFileSync(sourceLino, 'utf8'));
    assert.ok(lino.includes('(child: father mother)'), lino);
  });

  it('the cli imports the archive before the lino file and the query', async () => {
    const directory = scratchDirectory();
    const archive = join(directory, 'store.bin');
    const lino = join(directory, 'more.lino');
    writeFileSync(archive, bytes(ARCHIVE));
    writeFileSync(lino, '(2: 3 4)\n');
    const output = await clink(
      join(directory, 'target.links'),
      '--import-binary',
      archive,
      '--in',
      lino,
      '((4: 1 3)) ((4: 4 1))',
      '--after',
    );
    assert.equal(output.status, 0, output.stderr);
    assert.equal(output.stdout, '(a: a a)\n(2: é ab)\n(é: é é)\n(ab: ab a)\n');
  });

  it('the cli reports a missing archive', async () => {
    const directory = scratchDirectory();
    const missing = join(directory, 'missing.bin');
    const output = await clink(join(directory, 'target.links'), '--import-binary', missing);
    assert.notEqual(output.status, 0);
    assert.ok(output.stderr.includes('Failed to read the store archive'), output.stderr);
    assert.equal(existsSync(missing), false);
  });
});
