// The LiNo mapping of binary links notation, a port of links-notation 0.31.5
// src/binary/mapping.rs. The golden vectors come from
// docs/protocol/binary-links-notation-vectors.txt; the corpus and the other
// expectations come from rust/tests/protocol_packet_tests.rs and
// rust/tests/protocol_packet_malformed_tests.rs, and every crafted packet
// below was decoded by that crate's `decode_document` with the same result.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { describe, it } from 'node:test';
import {
  ArityRange,
  DecodeLimits,
  LinksPacket,
  Section,
  external,
} from '../src/protocol-binary-packet.js';
import {
  BinaryLinoCodec,
  BinaryLinoOptions,
  canonicalNumber,
  decodeDocument,
  encodeDocumentWithLimits,
} from '../src/protocol-binary.js';
import { formatDocument, parseDocument } from '../src/protocol-lino.js';

const hex = (bytes) => [...bytes].map((byte) => byte.toString(16).padStart(2, '0')).join(' ');
const unhex = (text) =>
  Buffer.from(
    text
      .split(/\s+/)
      .filter((byte) => byte !== '')
      .map((byte) => parseInt(byte, 16)),
  );

const failure = (kind, detail) => (error) => {
  assert.equal(error.kind, kind);
  assert.equal(error.detail, detail);
  return true;
};
const malformed = (detail) => failure('malformed', detail);
const unencodable = (detail) => failure('unencodable', detail);
const limitExceeded = (detail) => failure('limitExceeded', detail);

const limited = (overrides) => new DecodeLimits(overrides);
const codec = (options = {}, limits = {}) =>
  new BinaryLinoCodec(new BinaryLinoOptions(options), limited(limits));
const encode = (text, options = {}, limits = {}) =>
  encodeDocumentWithLimits(parseDocument(text), new BinaryLinoOptions(options), limited(limits));
const lengths = (packet) => [...packet.links()].map(([, link]) => link.length);
const widths = (packet) => packet.sections.map((section) => section.width);

/** A packet of one `1..` section of width 4 whose first link is at `gap + 1`. */
const tuples = (externalReferences, gap, links) =>
  new LinksPacket(externalReferences, [
    new Section(
      BigInt(gap),
      ArityRange.atLeast(1),
      4,
      links.map((link) => link.map(BigInt)),
    ),
  ]);
const decodeTuples = (externalReferences, gap, links, limits = {}) =>
  codec({}, limits).decode(tuples(externalReferences, gap, links).toBytes());

const CORPUS = [
  '() ((1 1))',
  '((1: 1 1)) ((1: 1 2))',
  '((1 1)) ()',
  '((1: 1 1)) ()',
  '(($i: $s $t)) (($i: $s $t))',
  '((($index: $source $target)) (($index: $target $source)))',
  '(a b c d)',
  `(name: 'with space' "it's")`,
  '((a))',
  '(((a)))',
  '(a (b c) ((d)))',
  '1\n2\n3',
  'hello',
  '😀 привет 世界',
  '0',
  '007',
  '9223372036854775807',
  '9223372036854775808',
  '18446744073709551615',
  '18446744073709551616',
  "'multi\nline'",
  '.dot',
  `'a''b"c\`d'`,
  '(1: (2: 3 4) 5)',
  '() ()',
  '(* *)',
  '(type: type type)',
];

/** external false/true × arity 2, 2..3, 1.. × packed false/true, as binary_options. */
const OPTION_SETS = [false, true].flatMap((externalReferences) =>
  [ArityRange.DOUBLETS, ArityRange.between(2, 3), ArityRange.atLeast(1)].flatMap((arity) =>
    [false, true].map((packedWidths) => ({ externalReferences, arity, packedWidths })),
  ),
);

const manyLinks = (count) =>
  Array.from({ length: count }, (_, index) => `(n${index} m${index})`).join('\n');

describe('golden vectors', () => {
  it('encodes and decodes every document vector', () => {
    // docs/protocol/binary-links-notation-vectors.txt, the `document` lines.
    const url = new URL(
      '../../../docs/protocol/binary-links-notation-vectors.txt',
      import.meta.url,
    );
    const vectors = readFileSync(url, 'utf8')
      .split('\n')
      .filter((line) => line !== '' && !line.startsWith('#'))
      .map((line) => line.split('\t'))
      .filter((fields) => fields[0] === 'document');
    assert.ok(vectors.length >= 80, `only ${vectors.length} document vectors`);
    for (const [, escaped, references, arity, packing, expected] of vectors) {
      const document = parseDocument(escaped.replaceAll('\\n', '\n'));
      const binary = codec({
        externalReferences: { plain: false, external: true }[references],
        arity: ArityRange.parse(arity),
        packedWidths: { uniform: false, packed: true }[packing],
      });
      assert.equal(hex(binary.encode(document)), expected, `${escaped} ${references} ${arity}`);
      assert.deepEqual(binary.decode(unhex(expected)), document);
    }
  });
});

describe('BinaryLinoOptions', () => {
  it('defaults every option off and compares them', () => {
    const plain = new BinaryLinoOptions();
    assert.equal(plain.externalReferences, false);
    assert.ok(plain.arity.equals(ArityRange.DOUBLETS));
    assert.equal(plain.packedWidths, false);
    assert.ok(plain.equals(new BinaryLinoOptions({})));
    assert.ok(!plain.equals(new BinaryLinoOptions({ externalReferences: true })));
    assert.ok(!plain.equals(new BinaryLinoOptions({ arity: ArityRange.atLeast(1) })));
    assert.ok(!plain.equals(new BinaryLinoOptions({ packedWidths: true })));
  });

  it('reads the options back from a packet, reply_options_follow_the_received_packet', () => {
    const options = new BinaryLinoOptions({
      externalReferences: true,
      arity: ArityRange.between(2, 3),
      packedWidths: true,
    });
    const document = parseDocument('(a b c) 70000');
    assert.ok(
      BinaryLinoOptions.ofPacket(encodeDocumentWithLimits(document, options)).equals(options),
    );
    const plain = encodeDocumentWithLimits(document, new BinaryLinoOptions());
    assert.ok(BinaryLinoOptions.ofPacket(plain).equals(new BinaryLinoOptions()));
    // Links of one reference widen the range downwards.
    const single = encode('a', { externalReferences: true, arity: ArityRange.atLeast(1) });
    assert.equal(`${BinaryLinoOptions.ofPacket(single).arity}`, '1..2');
    assert.ok(BinaryLinoOptions.ofPacket(new LinksPacket(false)).equals(new BinaryLinoOptions()));
  });
});

describe('canonicalNumber', () => {
  it('accepts unsigned decimals without leading zeros up to 64 bits', () => {
    assert.equal(canonicalNumber('0'), 0n);
    assert.equal(canonicalNumber('18446744073709551615'), 0xffffffffffffffffn);
    for (const text of ['', '007', '-1', '+1', '1a', '18446744073709551616']) {
      assert.equal(canonicalNumber(text), null, text);
    }
  });
});

describe('encoding', () => {
  it('round-trips the corpus with every option set', () => {
    for (const text of CORPUS) {
      const document = parseDocument(text);
      for (const options of OPTION_SETS) {
        const binary = codec(options);
        const bytes = binary.encode(document);
        assert.ok(bytes[0] >= 0x10 && bytes[0] <= 0x1f);
        assert.deepEqual(binary.decode(bytes), document, `${text} ${options.arity}`);
      }
    }
  });

  it('keeps packed widths small, uniform_widths_grow_past_256_addresses…', () => {
    assert.deepEqual(widths(encode(manyLinks(3))), [1]);
    const uniform = encode(manyLinks(400));
    assert.deepEqual(widths(uniform), [2]);
    const uniformBytes = uniform.toBytes();
    assert.equal(BigInt(uniformBytes.length), 3n + uniform.linkCount() * 2n * 2n);
    const packed = encode(manyLinks(400), { packedWidths: true });
    assert.equal(widths(packed)[0], 1);
    assert.ok(widths(packed).includes(2));
    const packedBytes = packed.toBytes();
    assert.ok(packedBytes.length < uniformBytes.length);
    const document = parseDocument(manyLinks(400));
    for (const bytes of [uniformBytes, packedBytes])
      assert.deepEqual(codec().decode(bytes), document);
  });

  it('never makes packed widths larger than uniform ones', () => {
    for (const text of [...CORPUS, manyLinks(200), '(1000 1)']) {
      for (const options of OPTION_SETS.filter((set) => set.packedWidths)) {
        const packed = encode(text, options).toBytes();
        const uniform = encode(text, { ...options, packedWidths: false }).toBytes();
        assert.ok(packed.length <= uniform.length, text);
      }
    }
  });

  it('widens only the sections of large external values', () => {
    const options = { externalReferences: true, packedWidths: true };
    assert.deepEqual(widths(encode('(100 1)', options)), [1]);
    assert.deepEqual(widths(encode('(70000 1)', options)), [4, 1]);
    // Beyond 63 bits the number falls back to in-band unary links.
    assert.ok(encode('18446744073709551615', options).linkCount() > 60n);
  });

  it('chooses doublets, triplets or any length by arity', () => {
    const document = parseDocument('(a b c)\n(d e)');
    const arities = (arity) => {
      const packet = encodeDocumentWithLimits(
        document,
        new BinaryLinoOptions({ externalReferences: true, arity }),
      );
      assert.deepEqual(codec().decode(packet.toBytes()), document);
      return lengths(packet);
    };
    assert.ok(arities(ArityRange.DOUBLETS).every((length) => length === 2));
    const triplets = arities(ArityRange.between(2, 3));
    assert.equal(triplets.filter((length) => length === 3).length, 1);
    assert.ok(triplets.every((length) => length === 2 || length === 3));
    assert.deepEqual(arities(ArityRange.atLeast(1)), triplets);
    assert.deepEqual(lengths(encode('a', { externalReferences: true })), [2, 2, 2, 2]);
    const anyLength = { externalReferences: true, arity: ArityRange.atLeast(1) };
    assert.deepEqual(lengths(encode('a', anyLength)), [2, 2, 1]);
    for (const [arity, singleLink] of [
      [ArityRange.between(2, 3), false],
      [ArityRange.between(2, 8), true],
      [ArityRange.atLeast(2), true],
    ]) {
      const packet = encode('(a b c d e f g h)', { externalReferences: true, arity });
      assert.equal(Math.max(...lengths(packet)) === 8, singleLink, `${arity}`);
    }
  });

  it('rejects arities without doublets', () => {
    for (const arity of [ArityRange.exactly(3), ArityRange.exactly(1), ArityRange.atLeast(3)]) {
      assert.throws(
        () => encode('(a b)', { arity }),
        unencodable(`arity ${arity} does not include doublets (2)`),
      );
    }
    assert.throws(
      () => encode('a', { arity: ArityRange.exactly(0) }),
      unencodable('arity must be at least 1'),
    );
  });

  it('enforces the limits, as encode_document_with_limits', () => {
    assert.throws(() => encode('((a))', {}, { maxDepth: 1 }), unencodable('nesting deeper than 1'));
    assert.equal(
      hex(encode('((a))', {}, { maxDepth: 2 }).toBytes()),
      '10 0e 01 01 06 06 07 07 08 08 09 09 0a 0a 0a 01 0b 0c 0d 00 03 0e 0f 00 04 10 11 00 04 12',
    );
    const tooManyNodes = unencodable('too many LiNo nodes');
    assert.throws(() => encode('(a: b c)', {}, { maxNodes: 3 }), tooManyNodes);
    assert.throws(() => encode('(a: b)', {}, { maxNodes: 1 }), tooManyNodes);
    assert.equal(encode('(a: b c)', {}, { maxNodes: 4 }).linkCount(), 25n);
    assert.throws(
      () => encode('abc', {}, { maxStringBytes: 2 }),
      unencodable('too many string bytes'),
    );
    assert.equal(
      hex(encode('abc', {}, { maxStringBytes: 3 }).toBytes()),
      '10 13 01 01 06 06 07 07 08 08 09 09 0a 0a 0a 01 0b 0c 0a 06 0b 0e 06 01 0a 10 0b 11 12 00 0f 13 0d 14 03 15 16 00 04 17',
    );
    assert.throws(() => encode('a b c', {}, { maxLinks: 1 }), unencodable('too many packet links'));
    assert.throws(
      () => encode('a', {}, { maxReferences: 1 }),
      unencodable('too many packet references'),
    );
    assert.equal(hex(encode('', {}, { maxLinks: 0 }).toBytes()), '10 00');
  });
});

describe('decoding', () => {
  it('decodes hand-built packets, hand_built_packets_decode', () => {
    for (const bytes of [
      '10 04 01 01 02 06 07 00 04 08',
      '12 01 1d 05 00 04 01 01 00 01 00 01 02 00 06 00 01 07 00 00 00 01 04 00 08 00',
    ]) {
      assert.equal(formatDocument(codec().decode(unhex(bytes))), '2');
    }
    // Number values: an external one, One and null.
    const numbers = [
      [2, external(5)],
      [2, 1],
      [2, 0],
      [6, 7, 8],
    ];
    assert.equal(formatDocument(decodeTuples(true, 5, numbers)), '5\n1\n0');
    const text = [
      [3, external(0x41), external(0xe000)],
      [6, 0],
    ];
    assert.deepEqual(decodeTuples(true, 5, text), ['A\u{e000}', { id: null, values: [] }]);
    assert.equal(formatDocument(decodeTuples(true, 5, [[external(12345), 0]])), '12345\n()');
    assert.deepEqual(decodeDocument(new LinksPacket(false), new DecodeLimits()), []);
  });

  it('rejects malformed packets, malformed_packets_are_rejected', () => {
    // rust/tests/protocol_packet_malformed_tests.rs.
    for (const [bytes, detail] of [
      ['', 'empty input'],
      ['20 00', 'unsupported binary header byte 0x20'],
      ['10 01 06 00', 'link 6 refers to Internal(6), which is not an earlier link'],
      ['10 01 07 00', 'link 6 refers to Internal(7), which is not an earlier link'],
      ['10 02 00', 'unexpected end of packet'],
      ['10 00 00', 'trailing bytes after the packet'],
      ['10 ff ff ff ff ff ff ff ff ff 7f', 'LEB128 value overflows 64 bits'],
      ['10 80 80 80 80 80 80 80 80 80 81', 'LEB128 value overflows 64 bits'],
      ['10 01 01 01', 'the root link is not a list'],
      ['10 02 00 02 04 06', 'broken element chain'],
      ['10 01 04 02', 'broken element chain'],
      ['10 02 02 00 04 06', 'marker 2 used as a value'],
      ['10 03 01 00 06 00 04 07', 'marker 1 cannot start a typed value'],
      ['12 02 14 05 01 20 02 02 06 00 04 07', 'a number needs exactly one value'],
      ['10 03 05 00 06 00 04 07', 'an identified link needs an id'],
      ['10 03 02 03 06 00 04 07', 'expected a unary number'],
      ['10 05 05 05 06 00 02 07 08 00 04 09', 'expected a unary number'],
      ['10 04 00 00 05 06 07 00 04 08', 'a link id must be a reference'],
      ['16 00', 'the explicit layout keeps the header width bits clear'],
      ['12 01 04 05 00', 'arity must be at least 1'],
      ['12 01 18 01 01 05 00', 'link length outside the section arity 1..2'],
      [
        '12 01 f8 ff ff ff ff ff ff ff ff 01 ff ff ff ff ff ff ff ff ff 01 00',
        'arity range overflows 64 bits',
      ],
      ['12 01 24 ff ff ff ff ff ff ff ff ff 01 01', 'addresses overflow 64 bits'],
      ['12 01 20 ff ff ff ff ff ff ff ff ff 01', 'addresses overflow 64 bits'],
      [
        '12 01 24 06 01 01 01',
        'a LiNo packet stores its links contiguously from address 6, found link 7 where 6 belongs',
      ],
      [
        '12 02 24 05 01 24 01 01 01 01 04 06',
        'a LiNo packet stores its links contiguously from address 6, found link 8 where 7 belongs',
      ],
    ]) {
      assert.throws(() => codec().decode(unhex(bytes)), malformed(detail), bytes);
    }
  });

  it('rejects crafted links with the messages of decode_document', () => {
    // Overflowing unary numbers: link 6 + k is 2^(k + 1), so link 69 is 2^64.
    const powers = [[1, 1], ...Array.from({ length: 64 }, (_, index) => [6 + index, 6 + index])];
    for (const [externalReferences, gap, links, detail] of [
      [false, 5, [[6, 0]], 'link 6 refers to Internal(6), which is not an earlier link'],
      [false, 5, [[1, 1, 1]], 'the root link is not a list'],
      [false, 5, [[5, 0, 0]], 'the root link is not a list'],
      [false, 5, [[0, 2]], 'marker 2 used as a value'],
      [
        false,
        5,
        [
          [1, 0],
          [6, 6],
        ],
        'marker 1 cannot start a typed value',
      ],
      [
        false,
        5,
        [
          [2, 0],
          [2, 6],
          [7, 0],
        ],
        'expected a unary number',
      ],
      [
        false,
        5,
        [
          [2, 3],
          [6, 0],
        ],
        'expected a unary number',
      ],
      [false, 5, [...powers, [2, 70], [71, 0]], 'expected a unary number'],
      [
        false,
        5,
        [
          [2, 1, 1],
          [6, 0],
        ],
        'a number needs exactly one value',
      ],
      [
        true,
        5,
        [
          [3, external(0x41), external(0xdfff)],
          [6, 0],
        ],
        'invalid code point 57343',
      ],
      [
        true,
        5,
        [
          [3, external(0x41), external(0x110000)],
          [6, 0],
        ],
        'invalid code point 1114112',
      ],
      [
        true,
        5,
        [
          [3, external(0x110000)],
          [6, 0],
        ],
        'broken element chain',
      ],
      [false, 5, [[5], [6, 0]], 'an identified link needs an id'],
      [
        false,
        5,
        [
          [5, 0],
          [6, 0],
        ],
        'an identified link needs an id',
      ],
      [
        false,
        5,
        [
          [5, 0, 0],
          [6, 0],
        ],
        'a link id must be a reference',
      ],
      [
        false,
        5,
        [
          [1, 1, 1],
          [4, 6],
          [7, 0],
        ],
        'broken element chain',
      ],
      [
        false,
        5,
        [
          [4, 3],
          [6, 0],
        ],
        'broken element chain',
      ],
      [
        false,
        5,
        [
          [1, 1, 1],
          [4, 6],
        ],
        'broken element chain',
      ],
    ]) {
      assert.throws(
        () => decodeTuples(externalReferences, gap, links),
        malformed(detail),
        JSON.stringify(links.slice(-2), (_, value) => `${value}`),
      );
    }
  });

  it('reports packets that fail validation as malformed', () => {
    const packet = new LinksPacket(false, [new Section(5n, ArityRange.DOUBLETS, 3, [[1n, 1n]])]);
    assert.throws(
      () => decodeDocument(packet, new DecodeLimits()),
      malformed('cannot encode: invalid width 3'),
    );
  });

  it('bounds hostile packets, hostile_packets_hit_limits', () => {
    // rust/tests/protocol_packet_malformed_tests.rs.
    const fewLinks = { maxLinks: 8 };
    for (const bytes of ['10 09', '12 09', '12 02 20 05 20 05']) {
      assert.throws(
        () => codec({}, fewLinks).decode(unhex(bytes)),
        (error) => {
          assert.equal(error.kind, 'limitExceeded');
          return true;
        },
      );
    }
    for (const bytes of ['10 03 01 01 01 01 01 01', '12 01 18 00 01 05']) {
      assert.throws(
        () => codec({}, { maxReferences: 5 }).decode(unhex(bytes)),
        limitExceeded('references exceed the limit of 5'),
      );
    }
    // A doubling chain d(k) = (d(k-1) d(k-1)) expands to 2^k nodes.
    const bomb = [[0n, 0n]];
    for (let address = 6n; address < 60n; address += 1n) bomb.push([address, address]);
    const last = 5n + BigInt(bomb.length);
    bomb.push([last, 0n], [4n, last + 1n]);
    const packet = LinksPacket.pack(
      false,
      bomb.map((link, index) => [6n + BigInt(index), link]),
      false,
    );
    assert.throws(
      () => codec({}, { maxNodes: 1 << 12 }).decode(packet.toBytes()),
      limitExceeded('too many LiNo nodes'),
    );
    const deep = `${'(y '.repeat(40)}x${')'.repeat(40)}`;
    assert.throws(
      () => codec({}, { maxDepth: 10 }).decode(codec().encode(parseDocument(deep))),
      limitExceeded('nesting deeper than 10'),
    );
    const threeLinks = codec().encode(parseDocument('a b c'));
    assert.throws(
      () => codec({}, { maxNodes: 2 }).decode(threeLinks),
      limitExceeded('chain too long'),
    );
  });

  it('enforces the decode limits of decode_document', () => {
    const abc = codec().encode(parseDocument('abc'));
    const tooManyBytes = limitExceeded('too many expanded string bytes');
    assert.throws(() => codec({}, { maxStringBytes: 2 }).decode(abc), tooManyBytes);
    const number = codec().encode(parseDocument('123'));
    assert.throws(() => codec({}, { maxStringBytes: 2 }).decode(number), tooManyBytes);
    const externalNumber = codec({ externalReferences: true }).encode(parseDocument('12345'));
    assert.equal(hex(externalNumber), '15 02 c7 cf 00 00 04 00 06 00');
    assert.throws(() => codec({}, { maxStringBytes: 3 }).decode(externalNumber), tooManyBytes);
    const twoRefs = codec().encode(parseDocument('a b'));
    assert.throws(
      () => codec({}, { maxNodes: 1 }).decode(twoRefs),
      limitExceeded('too many LiNo nodes'),
    );
    assert.throws(
      () => codec({}, { maxReferences: 3 }).decode(twoRefs),
      limitExceeded('references exceed the limit of 3'),
    );
    assert.throws(
      () => codec({}, { maxDepth: 1 }).decode(codec().encode(parseDocument('((a))'))),
      limitExceeded('nesting deeper than 1'),
    );
    const packet = LinksPacket.fromBytes(
      codec().encode(parseDocument('a b c')),
      new DecodeLimits(),
    );
    assert.throws(
      () => decodeDocument(packet, limited({ maxLinks: 2 })),
      limitExceeded('packet declares more than 2 links'),
    );
    // Reading the packet checks references already; a packet built in memory is checked here.
    assert.throws(
      () => decodeDocument(packet, limited({ maxReferences: 3 })),
      limitExceeded('references exceed the limit of 3'),
    );
  });

  it('builds a codec with the default options and limits', () => {
    const binary = new BinaryLinoCodec();
    assert.ok(binary.options.equals(new BinaryLinoOptions()));
    assert.equal(binary.limits.maxDepth, 64);
    assert.equal(hex(binary.encode([])), '10 00');
    assert.deepEqual(binary.decode(unhex('10 00')), []);
  });
});
