// The string and name layer of the names sidecar store. The sidecar rows are
// the ones the Rust clink writes for `() ((hi: hi hi))`.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { externalReference } from '../src/hybrid-reference.js';
import { LinkStorage } from '../src/link-storage.js';
import { UnicodeStringStorage } from '../src/unicode-string-storage.js';
import { scratchDirectory } from './helpers/storage.mjs';

const ORACLE_HI_SIDECAR = [
  '(1 1 1) (2 1 2) (3 1 3) (4 1 4) (5 1 5) (6 1 6) (7 4294967212 2) (8 4294967175 2)',
  '(9 4294967184 2) (10 4294967195 2) (11 7 8) (12 9 10) (13 11 12) (14 13 3) (15 4 14)',
  '(16 6 15) (17 1 16) (18 4294967211 2) (19 4294967186 2) (20 4294967191 2)',
  '(21 4294967197 2) (22 4294967185 2) (23 4294967196 2) (24 4294967213 2) (25 4294967187 2)',
  '(26 4294967198 2) (27 4294967188 2) (28 18 19) (29 20 21) (30 22 23) (31 10 24) (32 8 25)',
  '(33 26 22) (34 28 29) (35 30 31) (36 32 33) (37 34 35) (38 36 27) (39 37 38) (40 39 3)',
  '(41 4 40) (42 6 41) (43 2 42) (44 4294967183 2) (45 4294967179 2) (46 10 44) (47 45 10)',
  '(48 19 21) (49 46 47) (50 48 10) (51 49 50) (52 37 51) (53 52 3) (54 4 53) (55 6 54)',
  '(56 3 55) (57 4294967180 2) (58 4294967182 2) (59 4294967193 2) (60 24 57) (61 58 20)',
  '(62 19 59) (63 60 61) (64 63 62) (65 64 3) (66 4 65) (67 6 66) (68 4 67) (69 4294967227 2)',
  '(70 69 25) (71 9 57) (72 8 24) (73 57 58) (74 20 19) (75 70 71) (76 72 73) (77 74 59)',
  '(78 75 76) (79 78 77) (80 79 3) (81 4 80) (82 6 81) (83 5 82) (84 4294967218 2)',
  '(85 4294967199 2) (86 84 85) (87 25 10) (88 86 87) (89 88 3) (90 4 89) (91 6 90)',
  '(92 6 91) (93 4294967192 2) (94 93 20) (95 94 3) (96 4 95) (97 6 96) (98 4294967295 97)',
].join(' ');

const text = (storage) =>
  storage
    .all()
    .map((value) => `(${value.index} ${value.source} ${value.target})`)
    .join(' ');

function opened() {
  const links = LinkStorage.inMemory();
  return [links, new UnicodeStringStorage(links)];
}

function stored(rows) {
  const path = join(scratchDirectory(), 'db.names.links');
  writeFileSync(path, rows);
  const links = new LinkStorage(path, { stderr: { write() {} } });
  return [links, new UnicodeStringStorage(links)];
}

describe('unicode string storage', () => {
  it('names a data link as the Rust clink does', () => {
    const [links, strings] = opened();
    assert.equal(strings.setNameForExternalReference(1, 'hi'), 98);
    assert.equal(text(links), ORACLE_HI_SIDECAR);
    assert.equal(strings.getNameByExternalReference(1), 'hi');
    assert.equal(strings.getName(6), 'Name');
    assert.equal(strings.getNameByExternalReference(2), null);
  });

  it('reads strings from string, name and name pair links', () => {
    const [links, strings] = opened();
    const empty = strings.createString('');
    assert.deepEqual(links.get(empty), { index: empty, source: 4, target: 5 });
    assert.equal(strings.getString(empty), '');
    const named = strings.setName(9, 'nine');
    assert.equal(strings.getString(named), 'nine');
    assert.equal(strings.getString(links.get(named).target), 'nine');
    for (const id of [1, 0x7fff]) {
      assert.throws(() => strings.getString(id), {
        message: 'The passed link does not contain a string.',
      });
    }
  });

  it('finds data links by name', () => {
    const [, strings] = opened();
    strings.setNameForExternalReference(0, 'zero');
    strings.setNameForExternalReference(5, 'five');
    strings.setName(9, 'five');
    assert.equal(strings.getExternalReferenceByName('zero'), 0);
    assert.equal(strings.getExternalReferenceByName('five'), 5);
    assert.equal(strings.getExternalReferenceByName('Type'), null);
    assert.equal(strings.getExternalReferenceByName('missing'), null);
  });

  it('lists the first name of each data link', () => {
    const [links, strings] = opened();
    links.create(externalReference(7), 1);
    strings.setNameForExternalReference(7, 'seven');
    strings.setNameForExternalReference(7, 'again');
    strings.setNameForExternalReference(3, 'three');
    assert.equal(strings.getNameByExternalReference(7), 'seven');
    assert.deepEqual(
      strings.externalNames(),
      new Map([
        [3, 'three'],
        [7, 'seven'],
      ]),
    );
  });

  it('removes names and the name links no one else holds', () => {
    const [links, strings] = opened();
    links.create(externalReference(1), 1);
    links.create(externalReference(1), 0x7fff);
    strings.setNameForExternalReference(1, 'shared');
    const nameLink = links.get(strings.setNameForExternalReference(2, 'shared')).target;
    strings.setNameForExternalReference(2, 'own');
    strings.removeNameByExternalReference(1);
    assert.equal(strings.getNameByExternalReference(1), null);
    assert.equal(links.exists(nameLink), true);
    assert.notEqual(links.search(externalReference(1), 1), null);
    const before = links.all().length;
    strings.removeNameByExternalReference(2);
    assert.equal(strings.getNameByExternalReference(2), null);
    assert.equal(links.exists(nameLink), false);
    assert.equal(links.all().length, before - 4);
  });

  it('removes a name pair that is its own name link', () => {
    const [links, strings] = stored(
      '(1 1 1)\n(2 1 2)\n(3 1 3)\n(4 1 4)\n(5 1 5)\n(6 1 6)\n(7 6 7)\n',
    );
    assert.equal(links.exists(7), true);
    strings.removeName(6);
    assert.equal(links.exists(7), false);
    assert.equal(strings.getName(6), null);
  });
});
