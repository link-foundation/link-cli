// The codecs of the transactions layer: the inline tests of
// rust/src/transactions/mod.rs, the parsing half of
// rust/src/transactions/contract_tests.rs, and the shared vectors
// docs/protocol/transaction-name-vectors.json and
// docs/protocol/transaction-undo-vectors.json (as
// rust/tests/transaction_name_codec_tests.rs reads them).
import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { AddressOverflowError, InvalidFormatError } from '../src/errors.js';
import {
  CommitMode,
  compareBig,
  compareStrings,
  decodeName,
  decodeUtf16Hex,
  doublet,
  EMPTY_DOUBLET,
  encodeName,
  hex32,
  I64_MAX,
  INFINITE_RETENTION,
  journalRecord,
  KIND_NAMES,
  makeLegacyTransitionsFilename,
  makeTransitionsDatabaseFilename,
  makeTransitionsLogFilename,
  parseI64,
  parseImage,
  parseRecord,
  parseRetentionPolicy,
  parseTransition,
  parseU128,
  parseU128Hex,
  parseU64,
  parseU64Hex,
  RowUndoPolicy,
  sameDoublet,
  sameRecord,
  serializeImage,
  serializeRecord,
  serializeTransition,
  sidecarPath,
  transitionOf,
  TransitionKind,
  validateRecord,
} from '../src/transactions-types.js';
import { protocolVectors } from './helpers/transactions.mjs';

const transition = (kind, before, after, overrides = {}) => ({
  transactionId: 1n,
  sequence: 1n,
  timestampMs: 0n,
  kind,
  before,
  after,
  ...overrides,
});

describe('numbers', () => {
  it('parses Rust integers and refuses what str::parse refuses', () => {
    assert.equal(parseI64('9223372036854775807'), I64_MAX);
    assert.equal(parseI64('-9223372036854775808'), -(1n << 63n));
    assert.equal(parseI64('+7'), 7n);
    for (const text of ['9223372036854775808', '', '-', ' 1', '1 ', '0x1', '1.0']) {
      assert.equal(parseI64(text), null, text);
    }
    assert.equal(parseU64('18446744073709551615'), (1n << 64n) - 1n);
    assert.equal(parseU64('-0'), null);
    assert.equal(parseU64('18446744073709551616'), null);
    assert.equal(parseU128('340282366920938463463374607431768211455'), (1n << 128n) - 1n);
    assert.equal(parseU128Hex('ffffffffffffffffffffffffffffffff'), (1n << 128n) - 1n);
    assert.equal(parseU128Hex('1ffffffffffffffffffffffffffffffff'), null);
    assert.equal(parseU128Hex('+Ab'), 0xabn);
    assert.equal(parseU64Hex('g'), null);
    assert.equal(hex32(0xabcdefn), '00000000000000000000000000abcdef');
  });

  it('orders BigInts and strings as Rust does', () => {
    assert.deepEqual([3n, 1n, 2n, 1n].sort(compareBig), [1n, 1n, 2n, 3n]);
    // UTF-8 order puts U+FF5E after U+1F600's high surrogate in UTF-16 order.
    assert.ok(compareStrings('\uff5e', '\u{1f600}') < 0);
    assert.ok('\uff5e' > '\u{1f600}');
    assert.ok(compareStrings('a', 'ab') < 0);
    assert.ok(compareStrings('b', 'a') > 0);
    assert.equal(compareStrings('same', 'same'), 0);
  });
});

describe('retention policies', () => {
  it('parses infinite, sized and chunked specs (mod.rs retention_policy_parsing)', () => {
    assert.equal(parseRetentionPolicy('infinite'), INFINITE_RETENTION);
    assert.equal(parseRetentionPolicy(' INFINITE '), INFINITE_RETENTION);
    assert.equal(parseRetentionPolicy(''), INFINITE_RETENTION);
    assert.deepEqual(parseRetentionPolicy('sized:1000'), { kind: 'sized', maxTransitions: 1000n });
    assert.deepEqual(parseRetentionPolicy('Sized:0'), { kind: 'sized', maxTransitions: 0n });
    assert.deepEqual(parseRetentionPolicy('chunked:500:/tmp/x'), {
      kind: 'chunked',
      chunkSize: 500n,
      archiveDirectory: '/tmp/x',
    });
    assert.deepEqual(parseRetentionPolicy('CHUNKED:2:Dir:With:Colons'), {
      kind: 'chunked',
      chunkSize: 2n,
      archiveDirectory: 'Dir:With:Colons',
    });
  });

  it('refuses malformed specs with the Rust messages', () => {
    const cases = [
      ['garbage', "unknown retention spec 'garbage'"],
      ['sized:', "invalid sized retention spec 'sized:'"],
      ['sized:-1', "invalid sized retention spec 'sized:-1'"],
      ['chunked:0:/tmp/x', "invalid chunked size in 'chunked:0:/tmp/x'"],
      ['chunked:x:/tmp', "invalid chunked size in 'chunked:x:/tmp'"],
      ['chunked:5:', "invalid chunked retention spec 'chunked:5:'"],
      ['chunked:5', "invalid chunked retention spec 'chunked:5'"],
    ];
    for (const [spec, message] of cases) {
      assert.throws(() => parseRetentionPolicy(spec), { message }, spec);
    }
  });

  it('names the commit modes, row undo policies and kinds', () => {
    assert.deepEqual(Object.keys(CommitMode), ['Sync', 'Async']);
    assert.deepEqual(Object.keys(RowUndoPolicy), ['Observed', 'WriteAheadImage']);
    assert.deepEqual(KIND_NAMES, ['Create', 'Update', 'Delete']);
  });
});

describe('transitions', () => {
  it('round-trips a transition (mod.rs transition_round_trip)', () => {
    const value = transition(TransitionKind.Update, doublet(1, 2, 3), doublet(1, 4, 5), {
      transactionId: 0xabcdef1234567890n,
      sequence: 42n,
      timestampMs: 1234567890n,
    });
    const text = serializeTransition(value);
    assert.equal(text, 'v1|0000000000000000abcdef1234567890|42|1234567890|1|1,2,3|1,4,5');
    assert.deepEqual(parseTransition(text), value);
  });

  it('reports a wide address as an overflow (mod.rs wide_address_errors)', () => {
    assert.throws(
      () => parseTransition('v1|00000000000000000000000000000001|1|0|0|0,0,0|4294967296,0,0'),
      AddressOverflowError,
    );
  });

  it('refuses malformed transition text (contract_tests.rs)', () => {
    for (const text of [
      '',
      'v1',
      'v2|1|2|3|0|1,2|1,2,3',
      'v2|1|2|3|9|1,2,3|1,2,3',
      'v2|1|2|3|0|1,2,3|1,x,3',
      'v1|1|2|3|0|1,2|1,2,3',
      'v1|1|2|3|3|1,2,3|1,2,3',
      'v1|1|2|3|256|1,2,3|1,2,3',
      'v1|x|2|3|0|1,2,3|1,2,3',
      'v1|1|2|3|0|1,2,3|1,x,3',
    ]) {
      assert.throws(() => parseTransition(text), InvalidFormatError, text);
    }
    assert.throws(() => parseTransition('v1|1|2'), {
      message: "Invalid link format: malformed transition entry 'v1|1|2'",
    });
  });

  it('keeps the doublet helpers of the Rust types', () => {
    assert.ok(sameDoublet(doublet(1, 2, 3), { index: 1, source: 2, target: 3 }));
    assert.ok(!sameDoublet(doublet(1, 2, 3), doublet(1, 2, 4)));
    assert.ok(!sameDoublet(doublet(1, 2, 3), doublet(1, 3, 3)));
    assert.ok(!sameDoublet(doublet(1, 2, 3), EMPTY_DOUBLET));
    const record = journalRecord(
      transition(TransitionKind.Create, EMPTY_DOUBLET, doublet(1, 2, 3)),
    );
    assert.equal(record.nameOnly, false);
    assert.deepEqual(record.names, []);
    assert.deepEqual(
      transitionOf(record),
      transition(TransitionKind.Create, EMPTY_DOUBLET, doublet(1, 2, 3)),
    );
  });
});

describe('names', () => {
  it('decodes UTF-16 hexadecimal names (version_control/tests.rs decode_name)', () => {
    assert.equal(decodeUtf16Hex('006a006b'), 'jk');
    assert.equal(decodeUtf16Hex(''), '');
    assert.equal(decodeUtf16Hex('D83DDE00'), '\u{1f600}');
    for (const text of ['D800', 'DFFF', 'D8000041', 'éé', '０００１', '006', 'zzzz']) {
      assert.equal(decodeUtf16Hex(text), undefined, text);
    }
    assert.equal(decodeName('-'), null);
    assert.equal(decodeName('0041'), 'A');
    assert.equal(encodeName(null), '-');
    assert.equal(encodeName('\u96ea|\0'), '96EA007C0000');
  });
});

describe('docs/protocol/transaction-name-vectors.json', () => {
  const vectors = protocolVectors('transaction-name-vectors.json');

  it('round-trips every complete record with its names', () => {
    assert.equal(vectors.records.length, 7);
    for (const row of vectors.records) {
      const actual = parseRecord(row.wire);
      assert.equal(serializeRecord(actual), row.wire, row.name);
      assert.equal(actual.nameOnly, row.nameOnly, row.name);
      assert.equal(actual.names.length, row.names.length, row.name);
      actual.names.forEach((name, i) => {
        assert.equal(String(name.index), row.names[i].index);
        assert.equal(name.before, row.names[i].before);
        assert.equal(name.after, row.names[i].after);
      });
      assert.ok(sameRecord(actual, parseRecord(row.wire)));
    }
  });

  it('refuses invalid and non-Unicode records', () => {
    for (const text of [...vectors.invalid, ...vectors.nonUnicode]) {
      assert.throws(() => parseRecord(text), Error, text);
    }
  });

  it('reports wide addresses as overflows and reads legacy rows as transitions', () => {
    for (const text of vectors.wide) assert.throws(() => parseRecord(text), AddressOverflowError);
    for (const text of vectors.legacy) {
      const record = parseRecord(text);
      assert.equal(serializeRecord(record), serializeTransition(record));
      assert.equal(record.nameOnly, false);
      assert.deepEqual(record.names, []);
    }
  });
});

describe('complete records', () => {
  const row = (kind, before, after, names, nameOnly = false) =>
    journalRecord(transition(kind, before, after), nameOnly, names);
  const change = (index, before, after) => ({ index, before, after });

  it('validates the name state each record shape may hold', () => {
    const valid = [
      row(TransitionKind.Create, EMPTY_DOUBLET, doublet(1, 1, 1), [change(1, null, 'a')]),
      row(TransitionKind.Update, doublet(1, 1, 1), doublet(1, 2, 2), [change(1, 'a', 'b')]),
      row(TransitionKind.Delete, doublet(1, 1, 1), EMPTY_DOUBLET, [change(1, 'a', null)]),
      row(TransitionKind.Update, EMPTY_DOUBLET, EMPTY_DOUBLET, [change(0, 'x', null)], true),
      row(TransitionKind.Create, EMPTY_DOUBLET, doublet(1, 1, 1), []),
    ];
    for (const record of valid) validateRecord(record);
    const invalid = [
      row(TransitionKind.Update, EMPTY_DOUBLET, EMPTY_DOUBLET, [], true),
      row(TransitionKind.Create, EMPTY_DOUBLET, EMPTY_DOUBLET, [change(1, 'a', null)], true),
      row(TransitionKind.Update, doublet(1, 0, 0), EMPTY_DOUBLET, [change(1, 'a', null)], true),
      row(TransitionKind.Update, EMPTY_DOUBLET, doublet(1, 0, 0), [change(1, 'a', null)], true),
      row(
        TransitionKind.Update,
        EMPTY_DOUBLET,
        EMPTY_DOUBLET,
        [change(2, 'a', null), change(1, 'b', null)],
        true,
      ),
      row(TransitionKind.Create, doublet(1, 1, 1), doublet(1, 1, 1), [change(1, null, 'a')]),
      row(TransitionKind.Update, doublet(1, 1, 1), doublet(2, 1, 1), [change(2, 'a', 'b')]),
      row(TransitionKind.Delete, doublet(1, 1, 1), doublet(1, 1, 1), [change(1, 'a', null)]),
      row(TransitionKind.Create, EMPTY_DOUBLET, doublet(1, 1, 1), [change(2, null, 'a')]),
      row(TransitionKind.Create, EMPTY_DOUBLET, doublet(1, 1, 1), [
        change(1, null, 'a'),
        change(2, null, 'b'),
      ]),
    ];
    for (const record of invalid) {
      assert.throws(() => validateRecord(record), {
        message: 'Invalid link format: invalid name state in transaction record',
      });
    }
  });

  it('refuses non-canonical and out-of-range v2 records', () => {
    const base = 'v2|00000000000000000000000000000001|1|0|1|0,0,0|0,0,0';
    assert.equal(serializeRecord(parseRecord(`${base}|n|1,-,0041`)), `${base}|n|1,-,0041`);
    for (const text of [
      `${base}|n`,
      `${base}|x|1,-,0041`,
      `${base}|n|1,-`,
      `${base}|n|x,-,0041`,
      `${base}|n|1,D800,0041`,
      `${base}|n|1,-,0041|extra`,
      `${base}|n|1,-,0041`.replace('|1|0|1|', '|+1|0|1|'),
      `${base}|n|1,-,0041`.replace('|1|0|1|', '|1|253402300800000|1|'),
      `${base}|n|1,-,0041`.replace('|1|0|1|', '|1|-62135596800001|1|'),
    ]) {
      assert.throws(() => parseRecord(text), InvalidFormatError, text);
    }
  });
});

describe('docs/protocol/transaction-undo-vectors.json', () => {
  const vectors = protocolVectors('transaction-undo-vectors.json');

  it('round-trips every canonical undo image, which no record parser accepts', () => {
    assert.equal(vectors.records.length, 4);
    for (const { name, wire } of vectors.records) {
      const image = parseImage(wire);
      assert.equal(serializeImage(image), wire, name);
      assert.throws(() => parseRecord(wire), InvalidFormatError, name);
    }
    const holes = parseImage(vectors.records[2].wire);
    assert.deepEqual(holes.rows, [doublet(1, 11, 12), doublet(3, 1, 3)]);
    assert.deepEqual(
      holes.names,
      new Map([
        [0, 'zero'],
        [1, '\u96ea|\0'],
        [77, ''],
      ]),
    );
    assert.equal(parseImage(vectors.records[0].wire).names, null);
    assert.equal(parseImage(vectors.records[3].wire).base, I64_MAX);
  });

  it('refuses every invalid, wide and unpaired image', () => {
    assert.equal(vectors.invalid.length, 33);
    for (const wire of vectors.invalid) assert.throws(() => parseImage(wire), Error, wire);
    assert.throws(() => parseImage(vectors.wide), AddressOverflowError);
    assert.throws(() => parseImage(vectors.unpairedUtf16), {
      message: 'Invalid link format: invalid write-ahead undo image',
    });
  });

  it('refuses other shapes (contract_tests.rs undo_image_parsing_refuses_other_shapes)', () => {
    for (const text of ['', 'v3|undo', 'v2|undo|1|0|a|b', 'v3|redo|1|0|a|b']) {
      assert.throws(() => parseImage(text), InvalidFormatError, text);
    }
    const owner = '00000000000000000000000000000001';
    for (const text of [
      `v3|undo|${owner}|0|x,0,0|-`,
      `v3|undo|${owner}|0||1`,
      `v3|undo|${owner}|0||x,0041`,
      `v3|undo|${owner}|0||2,0041;1,0042`,
      `v3|undo|${owner}|0||1,-`,
    ]) {
      assert.throws(() => parseImage(text), InvalidFormatError, text);
    }
  });
});

describe('sidecar file names', () => {
  it('places the transitions log beside the database as Rust does', () => {
    const cases = [
      ['/var/data/db.links', '/var/data/db.transitions.links'],
      ['db.links', 'db.transitions.links'],
      ['./db.links', './db.transitions.links'],
      ['a/./b.links', 'a/b.transitions.links'],
      ['/db.links', '/db.transitions.links'],
      ['db', 'db.transitions.links'],
      ['.hidden', '.hidden.transitions.links'],
      ['db.tar.links', 'db.tar.transitions.links'],
      ['dir/', 'dir.transitions.links'],
      ['dir//x.links', 'dir/x.transitions.links'],
      ['..', '.transitions.links'],
      ['a/..', 'a/.transitions.links'],
      ['/', '.transitions.links'],
      ['', '.transitions.links'],
    ];
    for (const [path, expected] of cases) {
      assert.equal(makeTransitionsDatabaseFilename(path), expected, path);
      assert.equal(makeLegacyTransitionsFilename(path), expected, path);
      const journal = expected.replace(/\.transitions\.links$/, '.transitions.log');
      assert.equal(makeTransitionsLogFilename(path), journal, path);
    }
    assert.equal(sidecarPath('/x/y.links', '.z'), '/x/y.z');
  });
});
