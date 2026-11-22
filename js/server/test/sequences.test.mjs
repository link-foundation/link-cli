// Unicode sequences: strings as balanced trees of UTF-16 code unit symbols.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { externalReference } from '../src/hybrid-reference.js';
import { LinkStorage } from '../src/link-storage.js';
import { pinnedTypes } from '../src/pinned-types.js';
import {
  balancedVariant,
  stringToUnicodeSequence,
  unicodeSequenceToString,
  walkSequence,
} from '../src/sequences.js';

const SYMBOL = 2;
const SEQUENCE = 3;

const rows = (storage) => storage.all().map((value) => [value.index, value.source, value.target]);

// A store of crafted links, `{ index: [source, target] }`.
const crafted = (links) => ({
  get: (id) => (id in links ? { index: id, source: links[id][0], target: links[id][1] } : null),
});

// A store with the pinned types Type, UnicodeSymbol and UnicodeSequence.
function typed() {
  const storage = LinkStorage.inMemory();
  pinnedTypes(storage, 3);
  return storage;
}

const roundTrip = (content) => {
  const storage = typed();
  const sequence = stringToUnicodeSequence(storage, content, SYMBOL, SEQUENCE);
  return unicodeSequenceToString(storage, sequence, SYMBOL, SEQUENCE);
};

describe('balanced variant', () => {
  it('pairs neighbours layer by layer', () => {
    const storage = LinkStorage.inMemory();
    assert.equal(balancedVariant(storage, []), 0);
    assert.equal(balancedVariant(storage, [7]), 7);
    assert.equal(balancedVariant(storage, [7, 8]), 1);
    assert.equal(balancedVariant(storage, [7, 8, 9]), 2);
    assert.equal(balancedVariant(storage, [1, 2, 3, 4, 5]), 5);
    assert.deepEqual(rows(storage), [
      [1, 7, 8],
      [2, 1, 9],
      [3, 1, 2],
      [4, 3, 4],
      [5, 4, 5],
    ]);
  });
});

describe('unicode sequences', () => {
  it('stores a string as symbols under a sequence link', () => {
    const storage = typed();
    const sequence = stringToUnicodeSequence(storage, 'hi', SYMBOL, SEQUENCE);
    assert.equal(sequence, 7);
    assert.deepEqual(rows(storage).slice(3), [
      [4, externalReference(0x68), SYMBOL],
      [5, externalReference(0x69), SYMBOL],
      [6, 4, 5],
      [7, 6, SEQUENCE],
    ]);
    assert.deepEqual(walkSequence(storage, 6, SYMBOL), [4, 5]);
  });

  it('reads back what it stores', () => {
    for (const content of ['x', 'hello, world', 'café', '\u{1f600} \u{10ffff}', '"a\tb"']) {
      assert.equal(roundTrip(content), content);
    }
    assert.equal(unicodeSequenceToString(crafted({}), SEQUENCE, SYMBOL, SEQUENCE), '');
    assert.equal(
      unicodeSequenceToString(crafted({ 4: [5, SEQUENCE], 5: [65, SYMBOL] }), 4, SYMBOL, SEQUENCE),
      'A',
    );
  });

  it('rejects links that are not sequences', () => {
    for (const links of [{}, { 4: [5, 6] }]) {
      assert.throws(() => unicodeSequenceToString(crafted(links), 4, SYMBOL, SEQUENCE), {
        message: 'Specified link 4 is not a Unicode sequence.',
      });
    }
  });

  it('rejects broken trees', () => {
    const convert = (links) => () => unicodeSequenceToString(crafted(links), 4, SYMBOL, SEQUENCE);
    assert.throws(convert({ 4: [5, SEQUENCE] }), { message: 'Sequence link 5 does not exist.' });
    assert.throws(convert({ 4: [5, SEQUENCE], 5: [5, 5] }), {
      message: 'Unicode sequence cycle at link 5.',
    });
    assert.throws(convert({ 4: [5, SEQUENCE], 5: [externalReference(0x10000), SYMBOL] }), {
      message: 'out of range integral type conversion attempted',
    });
  });

  it('accepts a subtree shared by both halves', () => {
    const links = { 4: [5, SEQUENCE], 5: [6, 6], 6: [externalReference(0x61), SYMBOL] };
    assert.equal(unicodeSequenceToString(crafted(links), 4, SYMBOL, SEQUENCE), 'aa');
  });

  it('rejects unpaired surrogates as Rust String::from_utf16 does', () => {
    for (const content of ['\ud800', 'a\udc00', '\ud83d\ud83d', '\ude00\ud83d']) {
      assert.throws(() => roundTrip(content), { message: 'invalid utf-16: lone surrogate found' });
    }
  });
});
