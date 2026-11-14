// Binary links notation packets, a port of links-notation 0.31.6
// src/binary/packet.rs. The golden vectors come from
// docs/protocol/binary-links-notation-vectors.txt (checked as
// rust/tests/protocol_packet_tests.rs does); every other expectation is the
// output of that crate's `LinksPacket`, `ArityRange` and width helpers.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { describe, it } from 'node:test';
import {
  ArityRange,
  BINARY_VERSION_1,
  BufferSource,
  DecodeLimits,
  FIRST_LINK_ADDRESS,
  IDENTIFIED,
  LIST,
  LinksPacket,
  NULL,
  NUMBER,
  ONE,
  STRING,
  Section,
  WIDTHS,
  addressTier,
  debugReference,
  decodeExternal,
  encodeExternal,
  external,
  externalCapacity,
  externalValue,
  internalCapacity,
  isExternal,
  readLeb128,
  referenceWidth,
  writeLeb128,
} from '../src/protocol-binary-packet.js';

const U64_MAX = 0xffffffffffffffffn;
const I64_MAX = 0x7fffffffffffffffn;

const hex = (bytes) => [...bytes].map((byte) => byte.toString(16).padStart(2, '0')).join(' ');
const unhex = (text) =>
  Buffer.from(
    text
      .split(/\s+/)
      .filter((byte) => byte !== '')
      .map((byte) => parseInt(byte, 16)),
  );

/** The tab-separated lines of docs/protocol/binary-links-notation-vectors.txt. */
function goldenVectors(kind) {
  const url = new URL('../../../docs/protocol/binary-links-notation-vectors.txt', import.meta.url);
  return readFileSync(url, 'utf8')
    .split('\n')
    .filter((line) => line !== '' && !line.startsWith('#'))
    .map((line) => line.split('\t'))
    .filter((fields) => fields[0] === kind);
}

/** `address:reference reference…` separated by `;`, `#v` being external. */
const parseLinks = (text) =>
  text.split(';').map((link) => {
    const [address, references] = link.split(':');
    return [
      BigInt(address),
      references
        .split(' ')
        .map((reference) =>
          reference.startsWith('#') ? external(reference.slice(1)) : BigInt(reference),
        ),
    ];
  });

const references = (field) => ({ plain: false, external: true })[field];
const widths = (field) => ({ uniform: false, packed: true })[field];

/** Matches a thrown `ProtocolError` of `kind` with `detail`. */
const failure = (kind, detail) => (error) => {
  assert.equal(error.kind, kind);
  assert.equal(error.detail, detail);
  return true;
};
const malformed = (detail) => failure('malformed', detail);
const unencodable = (detail) => failure('unencodable', detail);
const limitExceeded = (detail) => failure('limitExceeded', detail);

const read = (text, limits = new DecodeLimits()) => LinksPacket.fromBytes(unhex(text), limits);
const doublet = (source, target) => [BigInt(source), BigInt(target)];
const internals = (...values) => values.map(BigInt);
const section = (gap, arity, width, links) =>
  new LinksPacket(false, [new Section(gap, arity, width, links)]);
const linksOf = (packet) => [...packet.links()];
const shapeOf = (packet) =>
  packet.sections.map((part) => [part.gap, `${part.arity}`, part.width, part.links.length]);

/** The value `ArityRange.parse` throws, a string as Rust's `Err(String)`. */
function arityProblem(text) {
  try {
    ArityRange.parse(text);
  } catch (problem) {
    return problem;
  }
  return null;
}

describe('golden vectors', () => {
  it('packs, writes and reads every links vector', () => {
    // docs/protocol/binary-links-notation-vectors.txt, the `links` lines.
    const vectors = goldenVectors('links');
    assert.ok(vectors.length >= 10, `only ${vectors.length} links vectors`);
    for (const [, text, referencesField, widthsField, expected] of vectors) {
      const links = parseLinks(text);
      const packet = LinksPacket.pack(references(referencesField), links, widths(widthsField));
      assert.equal(hex(packet.toBytes()), expected, text);
      const readBack = read(expected);
      assert.deepEqual(readBack, packet);
      assert.deepEqual(linksOf(readBack), links);
      assert.equal(readBack.linkCount(), BigInt(links.length));
    }
  });
});

describe('references', () => {
  it('keeps internal addresses and negates external values', () => {
    assert.equal(external(0), -1n);
    assert.equal(external(5n), -6n);
    assert.equal(isExternal(-1n), true);
    assert.equal(isExternal(0n), false);
    assert.equal(externalValue(external(70000)), 70000n);
    assert.equal(debugReference(5n), 'Internal(5)');
    assert.equal(debugReference(external(5)), 'External(5)');
  });

  it('numbers the markers as the LiNo mapping does', () => {
    assert.equal(BINARY_VERSION_1, 0x10);
    assert.deepEqual(
      [NULL, ONE, NUMBER, STRING, LIST, IDENTIFIED, FIRST_LINK_ADDRESS],
      [0n, 1n, 2n, 3n, 4n, 5n, 6n],
    );
    assert.deepEqual(WIDTHS, [1, 2, 4, 8]);
  });
});

describe('ArityRange', () => {
  it('parses and displays ranges as FromStr and Display do', () => {
    for (const [text, range] of [
      ['2', ArityRange.DOUBLETS],
      ['2..3', ArityRange.between(2, 3)],
      ['1..', ArityRange.atLeast(1)],
      ['3..3', ArityRange.exactly(3)],
      ['+2', ArityRange.exactly(2)],
      ['2..+3', ArityRange.between(2, 3)],
      ['1152921504606846975', ArityRange.exactly(1152921504606846975n)],
    ]) {
      assert.ok(ArityRange.parse(text).equals(range), text);
    }
    assert.equal(`${ArityRange.between(3, 3)}`, '3');
    assert.equal(`${ArityRange.between(2, 3)}`, '2..3');
    assert.equal(`${ArityRange.atLeast(1)}`, '1..');
  });

  it('rejects the texts Rust rejects, with its messages', () => {
    const invalid = (text) => `invalid arity '${text}': expected n, min..max or min..`;
    for (const text of ['', 'a', '1..b', '..3', '-1', ' 2', '2..3..4', '18446744073709551616']) {
      assert.equal(arityProblem(text), invalid(text), text);
    }
    assert.equal(arityProblem('0'), 'arity must be at least 1');
    assert.equal(arityProblem('0..2'), 'arity must be at least 1');
    assert.equal(arityProblem('3..2'), 'arity range 3..2 is empty');
    assert.equal(arityProblem('18446744073709551615'), 'arity 18446744073709551615 is too large');
    assert.equal(arityProblem('1152921504606846976'), 'arity 1152921504606846976 is too large');
    assert.equal(arityProblem('2'), null);
  });

  it('answers contains, isFixed, equals and extra', () => {
    assert.ok(ArityRange.atLeast(2).contains(1000));
    assert.ok(!ArityRange.atLeast(2).contains(1));
    assert.ok(!ArityRange.between(2, 3).contains(4));
    assert.ok(ArityRange.between(2, 3).contains(3n));
    assert.ok(ArityRange.exactly(2).isFixed());
    assert.ok(!ArityRange.atLeast(2).isFixed());
    assert.ok(!ArityRange.between(2, 3).equals(ArityRange.between(2, 4)));
    assert.ok(!ArityRange.between(1, 3).equals(ArityRange.between(2, 3)));
    assert.equal(ArityRange.atLeast(2).extra(), 0n);
    assert.equal(ArityRange.between(2, 5).extra(), 3n);
    assert.equal(ArityRange.exactly(0).problem(), 'arity must be at least 1');
    assert.equal(ArityRange.exactly(1n << 62n).problem(), 'arity 4611686018427387904 is too large');
    assert.equal(ArityRange.between(3, 2).problem(), 'arity range 3..2 is empty');
    assert.equal(ArityRange.atLeast(1).problem(), null);
  });

  it('builds the range of a section shape, from_shape', () => {
    assert.ok(ArityRange.fromShape(2n, null).equals(ArityRange.exactly(2)));
    assert.ok(ArityRange.fromShape(1n, 0n).equals(ArityRange.atLeast(1)));
    assert.ok(ArityRange.fromShape(2n, 3n).equals(ArityRange.between(2, 5)));
    assert.throws(
      () => ArityRange.fromShape(2n, U64_MAX),
      malformed('arity range overflows 64 bits'),
    );
    assert.throws(() => ArityRange.fromShape(0n, null), malformed('arity must be at least 1'));
  });
});

describe('limits and widths', () => {
  it('defaults the decode limits as Rust does', () => {
    const limits = new DecodeLimits();
    assert.equal(limits.maxLinks, 1n << 22n);
    assert.equal(limits.maxReferences, 1n << 24n);
    assert.equal(limits.maxNodes, 1 << 22);
    assert.equal(limits.maxStringBytes, 64 << 20);
    assert.equal(limits.maxDepth, 64);
    const unlimited = DecodeLimits.unlimited();
    assert.equal(unlimited.maxLinks, U64_MAX);
    assert.equal(unlimited.maxReferences, U64_MAX);
    assert.equal(unlimited.maxDepth, Number.MAX_SAFE_INTEGER);
    assert.equal(new DecodeLimits({ maxLinks: 3 }).maxLinks, 3n);
  });

  it('chooses width tiers as width_tiers_follow_the_number_of_links', () => {
    for (const [address, externalReferences, width] of [
      [0n, false, 1],
      [255n, false, 1],
      [256n, false, 2],
      [65535n, false, 2],
      [65536n, false, 4],
      [0xffffffffn, false, 4],
      [0x100000000n, false, 8],
      [U64_MAX, false, 8],
      [127n, true, 1],
      [128n, true, 2],
      [32767n, true, 2],
      [32768n, true, 4],
    ]) {
      assert.equal(addressTier(address, externalReferences), width, `${address}`);
    }
    assert.throws(
      () => addressTier(1n << 63n, true),
      unencodable('address 9223372036854775808 exceeds the internal range'),
    );
    assert.equal(internalCapacity(1, false), 255n);
    assert.equal(internalCapacity(1, true), 127n);
    assert.equal(internalCapacity(8, false), U64_MAX);
    assert.equal(internalCapacity(8, true), I64_MAX);
    assert.equal(externalCapacity(1), 127n);
    assert.equal(externalCapacity(8), I64_MAX);
  });

  it('measures references, reference_width', () => {
    assert.equal(referenceWidth(300n, false), 2);
    assert.equal(referenceWidth(external(127), true), 1);
    assert.equal(referenceWidth(external(128), true), 2);
    assert.equal(referenceWidth(external(I64_MAX), true), 8);
    assert.throws(
      () => referenceWidth(external(1), false),
      unencodable('external reference in a packet without external references'),
    );
    assert.throws(
      () => referenceWidth(external(1n << 63n), true),
      unencodable('external value 9223372036854775808 exceeds 63 bits'),
    );
  });

  it('encodes external values as Platform.Data.Hybrid does', () => {
    // [value, width, raw, decoded]: encode_external and decode_external.
    for (const [value, width, raw, decoded] of [
      [0n, 1, 0x80n, 0n],
      [0n, 8, 0x8000000000000000n, 0n],
      [1n, 1, 0xffn, 1n],
      [1n, 8, U64_MAX, 1n],
      [127n, 1, 0x81n, 127n],
      [128n, 1, 0x80n, 0n],
      [128n, 2, 0xff80n, 128n],
      [1000n, 1, 0x18n, null],
      [1000n, 2, 0xfc18n, 1000n],
      [1000n, 4, 0xfffffc18n, 1000n],
    ]) {
      assert.equal(encodeExternal(value, width), raw, `${value} @ ${width}`);
      assert.equal(decodeExternal(raw, width), decoded, `${value} @ ${width}`);
    }
    for (const width of WIDTHS) {
      for (const value of [0n, 1n, 2n, 100n, externalCapacity(width)]) {
        assert.equal(decodeExternal(encodeExternal(value, width), width), value);
      }
      assert.equal(decodeExternal(internalCapacity(width, true), width), null);
    }
    assert.equal(decodeExternal(0x7fn, 1), null);
  });
});

describe('LEB128', () => {
  const leb = (value) => {
    const out = [];
    writeLeb128(out, value);
    return hex(out);
  };
  const readLeb = (text) => new BufferSource(unhex(text)).run(readLeb128());

  it('writes and reads unsigned values up to 64 bits', () => {
    assert.equal(leb(0), '00');
    assert.equal(leb(127), '7f');
    assert.equal(leb(300), 'ac 02');
    assert.equal(leb(U64_MAX), 'ff ff ff ff ff ff ff ff ff 01');
    for (const value of [0n, 1n, 127n, 128n, 300n, U64_MAX])
      assert.equal(readLeb(leb(value)), value);
  });

  it('rejects values past 64 bits and truncated input', () => {
    const overflow = malformed('LEB128 value overflows 64 bits');
    assert.throws(() => readLeb('ff ff ff ff ff ff ff ff ff 02'), overflow);
    assert.throws(() => readLeb('ff ff ff ff ff ff ff ff ff 81'), overflow);
    assert.throws(() => readLeb('80'), malformed('unexpected end of packet'));
  });
});

describe('sections', () => {
  it('writes and reads section headers', () => {
    const out = [];
    new Section(3n, ArityRange.between(1, 4), 2, [[1n], [1n]]).writeHeader(out);
    assert.equal(hex(out), '1d 03 03 02');
    const reader = new BufferSource(Buffer.from(out));
    const [header, count] = reader.run(Section.readHeader());
    assert.equal(count, 2n);
    assert.deepEqual([header.gap, `${header.arity}`, header.width], [3n, '1..4', 2]);
    assert.equal(reader.offset, 4);
  });

  it('rejects sections that do not hold their links, writing_rejects_sections_…', () => {
    const pair = internals(1, 1);
    for (const [packet, detail] of [
      [
        section(0, ArityRange.exactly(3), 1, [pair]),
        'a link of 2 references in a section of arity 3',
      ],
      [section(0, ArityRange.exactly(0), 1, []), 'arity must be at least 1'],
      [section(0, ArityRange.between(3, 2), 1, []), 'arity range 3..2 is empty'],
      [section(0, ArityRange.exactly(1n << 62n), 1, []), 'arity 4611686018427387904 is too large'],
      [section(0, ArityRange.DOUBLETS, 3, [pair]), 'invalid width 3'],
      [section(0, ArityRange.DOUBLETS, 1, [[256n, NULL]]), 'Internal(256) does not fit 1 byte(s)'],
      [section(U64_MAX, ArityRange.DOUBLETS, 1, [pair]), 'addresses overflow 64 bits'],
      [
        new LinksPacket(true, [new Section(0, ArityRange.DOUBLETS, 1, [[external(1000), NULL]])]),
        'External(1000) does not fit 1 byte(s)',
      ],
    ]) {
      assert.throws(() => packet.toBytes(), unencodable(detail));
    }
  });
});

describe('LinksPacket', () => {
  it('writes hand-built packets, hand_built_packets_decode', () => {
    const links = [doublet(1, 1), doublet(2, 6), doublet(7, 0), doublet(4, 8)];
    const compact = section(5, ArityRange.DOUBLETS, 1, links);
    assert.equal(hex(compact.toBytes()), '10 04 01 01 02 06 07 00 04 08');
    const explicit = section(5, ArityRange.atLeast(1), 2, links);
    assert.equal(
      hex(explicit.toBytes()),
      '12 01 1d 05 00 04 01 01 00 01 00 01 02 00 06 00 01 07 00 00 00 01 04 00 08 00',
    );
    assert.deepEqual(linksOf(read(hex(explicit.toBytes()))), linksOf(compact));
  });

  it('writes the layouts that are not compact', () => {
    // No sections: the compact header with width 1 and no links.
    assert.equal(hex(new LinksPacket(true).toBytes()), '11 00');
    assert.equal(hex(new LinksPacket(false, []).toBytes()), '10 00');
    // One empty section is explicit.
    assert.equal(hex(section(5, ArityRange.DOUBLETS, 1, []).toBytes()), '12 01 24 05 00');
    // Two sections are explicit.
    const two = new LinksPacket(false, [
      new Section(0, ArityRange.DOUBLETS, 1, [internals(1, 1)]),
      new Section(0, ArityRange.DOUBLETS, 2, []),
    ]);
    assert.equal(hex(two.toBytes()), '12 02 20 01 21 00 01 01');
    const wide = new LinksPacket(true, [
      new Section(0, ArityRange.between(1, 3), 8, [[external(1000)], [1n, 2n, external(0)]]),
    ]);
    assert.equal(
      hex(wide.toBytes()),
      '13 01 1b 02 02 00 18 fc ff ff ff ff ff ff 02 01 00 00 00 00 00 00 00 02 00 00 00 00 00 00 00 00 00 00 00 00 00 00 80',
    );
    assert.deepEqual(read(hex(wide.toBytes())), wide);
  });

  it('reads compact and explicit packets', () => {
    assert.deepEqual(linksOf(read('11 01 80 7f')), [[6n, [external(0), 127n]]]);
    assert.deepEqual(linksOf(read('10 01 80 7f')), [[6n, [128n, 127n]]]);
    assert.deepEqual(linksOf(read('1c 01 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00')), [
      [6n, [1n, 0n]],
    ]);
    assert.deepEqual(read('12 00'), new LinksPacket(false, []));
    assert.deepEqual(read('10 00'), new LinksPacket(false, []));
    const packet = read('13 01 1c 05 01 04 01 ff ff 00 06 01 00 07 00 08');
    assert.deepEqual(shapeOf(packet), [[5n, '1..2', 1, 4]]);
    assert.deepEqual(packet.sections[0].links, [[external(1), external(1)], [6n], [0n, 7n], [8n]]);
  });

  it('rejects malformed packets with the Rust messages', () => {
    for (const [bytes, detail] of [
      ['', 'empty input'],
      ['20', 'unsupported binary header byte 0x20'],
      ['0a', 'unsupported binary header byte 0x0A'],
      ['10', 'unexpected end of packet'],
      ['10 05 01', 'unexpected end of packet'],
      ['10 80', 'unexpected end of packet'],
      ['12 02', 'unexpected end of packet'],
      ['16 00', 'the explicit layout keeps the header width bits clear'],
      ['12 01 00 00', 'arity must be at least 1'],
      ['12 01 28 ff ff ff ff ff ff ff ff ff 01 01', 'arity range overflows 64 bits'],
      ['12 01 24 ff ff ff ff ff ff ff ff ff 01 01', 'addresses overflow 64 bits'],
      ['10 fa ff ff ff ff ff ff ff ff 01', 'addresses overflow 64 bits'],
      ['12 01 2c 01 02 01 05', 'link length outside the section arity 2..4'],
      ['12 01 18 03 01 05 01 01 01', 'link length outside the section arity 1..4'],
      ['12 01 28 00 01 ff ff ff ff ff ff ff ff ff 01', 'link length outside the section arity 2..'],
      ['10 ff ff ff ff ff ff ff ff ff 02', 'LEB128 value overflows 64 bits'],
      ['10 01 01 01 00', 'trailing bytes after the packet'],
    ]) {
      assert.throws(() => read(bytes), malformed(detail), bytes);
    }
  });

  it('enforces the decode limits', () => {
    const oneLink = new DecodeLimits({ maxLinks: 1 });
    const declares = limitExceeded('packet declares more than 1 links');
    assert.throws(() => read('12 02', oneLink), declares);
    assert.throws(() => read('10 02 01 01 01 01', oneLink), declares);
    assert.throws(() => read('12 01 20 02 01 01 01 01', oneLink), declares);
    assert.throws(
      () => read('10 f9 ff ff ff ff ff ff ff ff 01'),
      limitExceeded('packet declares more than 4194304 links'),
    );
    assert.throws(
      () => read('10 02 01 01 01 01', new DecodeLimits({ maxReferences: 3 })),
      limitExceeded('references exceed the limit of 3'),
    );
    assert.equal(read('10 02 01 01 01 01', new DecodeLimits({ maxReferences: 4 })).linkCount(), 2n);
  });

  it('rejects links it cannot lay out, packing_rejects_links_it_cannot_lay_out', () => {
    for (const [externalReferences, links, detail] of [
      [false, [[0n, [1n]]], 'link addresses must ascend from 1, got 0 after 0'],
      [
        false,
        [
          [2n, [1n]],
          [1n, [1n]],
        ],
        'link addresses must ascend from 1, got 1 after 2',
      ],
      [
        false,
        [
          [2n, [1n]],
          [2n, [1n]],
        ],
        'link addresses must ascend from 1, got 2 after 2',
      ],
      [false, [[1n, []]], 'link 1 has no references'],
      [false, [[1n, [external(1)]]], 'external reference in a packet without external references'],
      [true, [[1n, [external(1n << 63n)]]], 'external value 9223372036854775808 exceeds 63 bits'],
      [true, [[1n, [1n << 63n]]], 'address 9223372036854775808 exceeds the internal range'],
      [false, [[U64_MAX, [1n]]], 'addresses overflow 64 bits'],
    ]) {
      assert.throws(() => LinksPacket.pack(externalReferences, links, true), unencodable(detail));
    }
    assert.equal(
      hex(LinksPacket.pack(false, [[1n, [U64_MAX]]], true).toBytes()),
      '12 01 13 01 ff ff ff ff ff ff ff ff',
    );
    assert.equal(hex(LinksPacket.pack(false, [], true).toBytes()), '10 00');
  });

  it('keeps the uniform layout unless packed widths save bytes', () => {
    // The packed layout differs but takes as many bytes: the uniform one wins.
    const even = [
      [1n, internals(300, 300)],
      [2n, internals(200, 1)],
    ];
    const packed = LinksPacket.pack(false, even, true);
    assert.equal(hex(packed.toBytes()), '12 01 21 02 2c 01 2c 01 c8 00 01 00');
    assert.deepEqual(shapeOf(packed), [[0n, '2', 2, 2]]);
    // Mixed lengths and widths: packed splits into three sections.
    const mixed = [
      [1n, internals(1, 1)],
      [2n, internals(1, 1, 1)],
      [3n, internals(70000, 1)],
    ];
    assert.deepEqual(shapeOf(LinksPacket.pack(false, mixed, true)), [
      [0n, '2', 1, 1],
      [0n, '3', 1, 1],
      [0n, '2', 4, 1],
    ]);
    assert.equal(
      hex(LinksPacket.pack(false, mixed, true).toBytes()),
      '12 03 20 01 30 01 22 01 01 01 01 01 01 70 11 01 00 01 00 00 00',
    );
    const uniform = LinksPacket.pack(false, mixed, false);
    assert.deepEqual(shapeOf(uniform), [
      [0n, '2', 4, 1],
      [0n, '3', 4, 1],
      [0n, '2', 4, 1],
    ]);
    assert.equal(
      hex(uniform.toBytes()),
      '12 03 22 01 32 01 22 01 01 00 00 00 01 00 00 00 01 00 00 00 01 00 00 00 01 00 00 00 70 11 01 00 01 00 00 00',
    );
  });

  it('drives a reader that ends cleanly before a packet', () => {
    const source = new BufferSource(unhex('10 01 01 01 10 00'));
    const limits = new DecodeLimits();
    assert.equal(source.run(LinksPacket.reader(limits)).linkCount(), 1n);
    assert.equal(source.offset, 4);
    assert.equal(source.run(LinksPacket.reader(limits)).linkCount(), 0n);
    assert.equal(source.run(LinksPacket.reader(limits)), null);
    assert.equal(source.offset, 6);
  });
});
