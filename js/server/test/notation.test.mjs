// Tests of src/notation.js: `readReference` and `quotedReferenceEnd`, the
// delimited-reference reading of the links-notation package (src/quotes.js)
// the query parser uses, as `links_notation::parser::quoted_reference_end` in
// the Rust crate.
import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { LinkStorage } from '../src/link-storage.js';
import { NamedTypesDecorator } from '../src/named-types.js';
import { formatReference, quotedReferenceEnd, readReference } from '../src/notation.js';
import { QueryProcessor } from '../src/query-processor.js';

const read = (text, start = 0) => readReference(text, start);

describe('readReference', () => {
  it('reads nothing where no delimiter opens a reference', () => {
    assert.equal(read('abc'), null);
    assert.equal(read('(a)'), null);
    assert.equal(read('x "a"', 0), null);
    assert.equal(quotedReferenceEnd('abc', 0), null);
  });

  it('reads a reference in single delimiters of each kind', () => {
    assert.deepEqual(read('"a b" c'), { value: 'a b', length: 5, closed: true });
    assert.deepEqual(read("'it' x"), { value: 'it', length: 4, closed: true });
    assert.deepEqual(read('x `y`', 2), { value: 'y', length: 3, closed: true });
    assert.equal(read('"open'), null);
    assert.equal(quotedReferenceEnd('x "a b" c', 2), 7);
  });

  it('reads a substantive body between doubled delimiters', () => {
    assert.deepEqual(read('""a b"" x'), { value: 'a b', length: 7, closed: true });
    assert.deepEqual(read('""(a)""'), { value: '(a)', length: 7, closed: true });
    // A doubled run inside is an escaped delimiter.
    assert.deepEqual(read('""x""""y""'), { value: 'x""y', length: 10, closed: true });
    assert.deepEqual(read(`""a'b"c""`), { value: `a'b"c`, length: 9, closed: true });
  });

  it('reads doubled delimiters around a blank or unbalanced body as an empty reference', () => {
    for (const text of ['""  ""', '""\t\n""', '"")(""', '""(""', '""a)(""']) {
      assert.deepEqual(read(text), { value: '', length: 2, closed: true }, JSON.stringify(text));
      assert.equal(quotedReferenceEnd(text, 0), 2, JSON.stringify(text));
    }
  });

  it('reads an even run that nothing closes as an open empty reference', () => {
    assert.deepEqual(read('""a'), { value: '', length: 2, closed: false });
    assert.deepEqual(read('""""""'), { value: '', length: 6, closed: false });
  });
});

describe('doubled delimiters in a query', () => {
  it('name a link as in the Rust clink', () => {
    // `clink '() ((""a b"": 1 1))' --after` prints `('a b': 'a b' 'a b')`.
    const storage = new NamedTypesDecorator(LinkStorage.inMemory(), LinkStorage.inMemory());
    new QueryProcessor().processQuery(storage, '() ((""a b"": 1 1))');
    assert.deepEqual(storage.linoLines(), ["('a b': 'a b' 'a b')"]);
    assert.equal(formatReference('a b'), "'a b'");
  });
});
