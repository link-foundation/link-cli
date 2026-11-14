// Binary links notation packets, version 1: ports links-notation 0.31.6
// src/binary/packet.rs (`ArityRange`, `DecodeLimits`, `LinksPacket` with its
// `pack` section planner, `to_bytes`, `from_bytes` and `read_from`).
//
// The npm links-notation package has its own `LinksPacket`, but its errors are
// worded differently; this port keeps the Rust texts, which reach clients in
// `(error: …)` replies.
//
// Addresses and values are u64, so they are BigInts. A reference is one
// BigInt: an internal address `a` is `a` (≥ 0) and the external value `v` is
// `-(v + 1)`, so references compare and hash as plain values.
import { ProtocolError } from './protocol-errors.js';

const U64_MAX = 0xffffffffffffffffn;

/** The high nibble of the header byte; text never starts with `0x10..=0x1F`. */
export const BINARY_VERSION_1 = 0x10;
const FLAG_EXTERNAL_REFERENCES = 0b0001;
const FLAG_EXPLICIT_LAYOUT = 0b0010;
const WIDTH_SHIFT = 2;
const WIDTH_BITS = 0b1100;
const SHAPE_WIDTH_BITS = 0b0011n;
const SHAPE_HAS_GAP = 0b0100n;
const SHAPE_VARIABLE_ARITY = 0b1000n;
const SHAPE_MIN_ARITY_SHIFT = 4n;

/** The marker points of the LiNo mapping. */
export const NULL = 0n;
export const ONE = 1n;
export const NUMBER = 2n;
export const STRING = 3n;
export const LIST = 4n;
export const IDENTIFIED = 5n;
export const FIRST_LINK_ADDRESS = 6n;
const COMPACT_GAP = FIRST_LINK_ADDRESS - 1n;

/** The reference widths, in bytes. */
export const WIDTHS = [1, 2, 4, 8];

/** `Reference::External(value)`. */
export const external = (value) => -(BigInt(value) + 1n);
/** Whether `reference` is external. */
export const isExternal = (reference) => reference < 0n;
/** The value of an external reference. */
export const externalValue = (reference) => -reference - 1n;
/** `{reference:?}`: `Internal(5)` or `External(5)`. */
export const debugReference = (reference) =>
  isExternal(reference) ? `External(${externalValue(reference)})` : `Internal(${reference})`;

/** How many references the links of a section hold: `min..=max`, `max` null for no bound. */
export class ArityRange {
  constructor(min, max) {
    this.min = BigInt(min);
    this.max = max === null ? null : BigInt(max);
  }

  static exactly(arity) {
    return new ArityRange(arity, arity);
  }

  static atLeast(min) {
    return new ArityRange(min, null);
  }

  static between(min, max) {
    return new ArityRange(min, max);
  }

  contains(length) {
    const value = BigInt(length);
    return value >= this.min && (this.max === null || value <= this.max);
  }

  isFixed() {
    return this.max === this.min;
  }

  equals(other) {
    return this.min === other.min && this.max === other.max;
  }

  /** The reason the range is invalid, or null. */
  problem() {
    if (this.min === 0n) return 'arity must be at least 1';
    if (this.min > U64_MAX >> SHAPE_MIN_ARITY_SHIFT) return `arity ${this.min} is too large`;
    if (this.max !== null && this.max < this.min) return `arity range ${this} is empty`;
    return null;
  }

  extra() {
    return this.max === null ? 0n : this.max - this.min;
  }

  static fromShape(min, extra) {
    let range;
    if (extra === null) range = ArityRange.exactly(min);
    else if (extra === 0n) range = ArityRange.atLeast(min);
    else if (min + extra > U64_MAX) throw ProtocolError.malformed('arity range overflows 64 bits');
    else range = ArityRange.between(min, min + extra);
    const problem = range.problem();
    if (problem !== null) throw ProtocolError.malformed(problem);
    return range;
  }

  toString() {
    if (this.max === this.min) return `${this.max}`;
    return this.max === null ? `${this.min}..` : `${this.min}..${this.max}`;
  }

  /** `FromStr`: `n`, `min..max` (inclusive) or `min..`; throws the message as a string. */
  static parse(text) {
    const number = (part) => {
      if (!/^\+?[0-9]+$/.test(part) || BigInt(part) > U64_MAX) {
        throw `invalid arity '${text}': expected n, min..max or min..`;
      }
      return BigInt(part);
    };
    const split = text.indexOf('..');
    let range;
    if (split < 0) range = ArityRange.exactly(number(text));
    else if (split + 2 === text.length) range = ArityRange.atLeast(number(text.slice(0, split)));
    else range = ArityRange.between(number(text.slice(0, split)), number(text.slice(split + 2)));
    const problem = range.problem();
    if (problem !== null) throw problem;
    return range;
  }
}

ArityRange.DOUBLETS = ArityRange.exactly(2);

/**
 * Safety limits applied while decoding untrusted input. `maxLinks` and
 * `maxReferences` are u64 BigInts; the `usize` limits are numbers.
 */
export class DecodeLimits {
  constructor({
    maxLinks = 1n << 22n,
    maxReferences = 1n << 24n,
    maxNodes = 1 << 22,
    maxStringBytes = 64 << 20,
    maxDepth = 64,
  } = {}) {
    this.maxLinks = BigInt(maxLinks);
    this.maxReferences = BigInt(maxReferences);
    this.maxNodes = maxNodes;
    this.maxStringBytes = maxStringBytes;
    this.maxDepth = maxDepth;
  }

  /** Limits for trusted input: only the address space bounds a packet. */
  static unlimited() {
    return new DecodeLimits({
      maxLinks: U64_MAX,
      maxReferences: U64_MAX,
      maxNodes: Number.MAX_SAFE_INTEGER,
      maxStringBytes: Number.MAX_SAFE_INTEGER,
      maxDepth: Number.MAX_SAFE_INTEGER,
    });
  }
}

/** Largest internal address that fits in `width` bytes. */
export function internalCapacity(width, externalReferences) {
  const bits = BigInt(width * 8 - (externalReferences ? 1 : 0));
  return bits >= 64n ? U64_MAX : (1n << bits) - 1n;
}

/** Largest external value that fits in `width` bytes. */
export const externalCapacity = (width) => (1n << BigInt(width * 8 - 1)) - 1n;

/** The narrowest width able to hold the internal address `address`. */
export function addressTier(address, externalReferences) {
  const width = WIDTHS.find(
    (candidate) => internalCapacity(candidate, externalReferences) >= address,
  );
  if (width === undefined) {
    throw ProtocolError.unencodable(`address ${address} exceeds the internal range`);
  }
  return width;
}

/** The narrowest width able to hold `reference`. */
export function referenceWidth(reference, externalReferences) {
  if (!isExternal(reference)) return addressTier(reference, externalReferences);
  if (!externalReferences) {
    throw ProtocolError.unencodable('external reference in a packet without external references');
  }
  const value = externalValue(reference);
  const width = WIDTHS.find((candidate) => externalCapacity(candidate) >= value);
  if (width === undefined) {
    throw ProtocolError.unencodable(`external value ${value} exceeds 63 bits`);
  }
  return width;
}

const widthMask = (width) => internalCapacity(width, false);

/** Encodes an external value at `width` the way `Platform.Data.Hybrid<T>` does. */
export function encodeExternal(value, width) {
  if (value === 0n) return 1n << BigInt(width * 8 - 1);
  return BigInt.asUintN(64, -value) & widthMask(width);
}

/** Decodes a raw `width`-byte value: the external value, or null for an address. */
export function decodeExternal(raw, width) {
  const externalZero = 1n << BigInt(width * 8 - 1);
  if (raw === externalZero) return 0n;
  if (raw > externalZero) return BigInt.asUintN(64, -raw) & widthMask(width);
  return null;
}

function widthCode(width) {
  const code = WIDTHS.indexOf(width);
  if (code < 0) throw ProtocolError.unencodable(`invalid width ${width}`);
  return code;
}

const widthFromCode = (code) => WIDTHS[Number(code & 0b11n)];

/** Appends `value` as unsigned LEB128 to the byte array `out`. */
export function writeLeb128(out, value) {
  let rest = BigInt(value);
  for (;;) {
    const byte = Number(rest & 0x7fn);
    rest >>= 7n;
    if (rest === 0n) {
      out.push(byte);
      return;
    }
    out.push(byte | 0x80);
  }
}

function writeRaw(out, value, width) {
  let rest = value;
  for (let index = 0; index < width; index += 1) {
    out.push(Number(rest & 0xffn));
    rest >>= 8n;
  }
}

/**
 * The steps of a reader: each `yield n` asks the driver for `n` bytes and
 * gets fewer only at the end of the input; `yield 0` asks for one byte where
 * the input may cleanly end.
 */
function* readExact(count) {
  const bytes = yield count;
  if (bytes.length < count) throw ProtocolError.malformed('unexpected end of packet');
  return bytes;
}

/** Reads an unsigned LEB128 value of at most 64 bits. */
export function* readLeb128() {
  let value = 0n;
  for (let shift = 0n; shift < 64n; shift += 7n) {
    const [byte] = yield* readExact(1);
    const payload = BigInt(byte & 0x7f);
    if (shift === 63n && payload > 1n) {
      throw ProtocolError.malformed('LEB128 value overflows 64 bits');
    }
    value |= payload << shift;
    if ((byte & 0x80) === 0) return value;
  }
  throw ProtocolError.malformed('LEB128 value overflows 64 bits');
}

function* readRaw(width) {
  const bytes = yield* readExact(width);
  let value = 0n;
  for (let index = width - 1; index >= 0; index -= 1) value = (value << 8n) | BigInt(bytes[index]);
  return value;
}

/** A run of links at consecutive addresses sharing an arity range and a width. */
export class Section {
  constructor(gap, arity, width, links = []) {
    this.gap = BigInt(gap);
    this.arity = arity;
    this.width = width;
    this.links = links;
  }

  writeHeader(out) {
    let shape = BigInt(widthCode(this.width)) | (this.arity.min << SHAPE_MIN_ARITY_SHIFT);
    if (this.gap !== 0n) shape |= SHAPE_HAS_GAP;
    if (!this.arity.isFixed()) shape |= SHAPE_VARIABLE_ARITY;
    writeLeb128(out, shape);
    if (this.gap !== 0n) writeLeb128(out, this.gap);
    if (!this.arity.isFixed()) writeLeb128(out, this.arity.extra());
    writeLeb128(out, this.links.length);
  }

  static *readHeader() {
    const shape = yield* readLeb128();
    const width = widthFromCode(shape & SHAPE_WIDTH_BITS);
    const gap = shape & SHAPE_HAS_GAP ? yield* readLeb128() : 0n;
    const extra = shape & SHAPE_VARIABLE_ARITY ? yield* readLeb128() : null;
    const arity = ArityRange.fromShape(shape >> SHAPE_MIN_ARITY_SHIFT, extra);
    const count = yield* readLeb128();
    return [new Section(gap, arity, width), count];
  }

  validate() {
    widthCode(this.width);
    const problem = this.arity.problem();
    if (problem !== null) throw ProtocolError.unencodable(problem);
    for (const link of this.links) {
      if (!this.arity.contains(link.length)) {
        throw ProtocolError.unencodable(
          `a link of ${link.length} references in a section of arity ${this.arity}`,
        );
      }
    }
  }
}

const tooManyLinks = (limits) =>
  ProtocolError.limitExceeded(`packet declares more than ${limits.maxLinks} links`);

/** A binary links packet. */
export class LinksPacket {
  constructor(externalReferences, sections = []) {
    this.externalReferences = externalReferences;
    this.sections = sections;
  }

  /**
   * Lays out `links`, `[address, references]` pairs in ascending address
   * order, with the version 1 section planner.
   */
  static pack(externalReferences, links, packedWidths) {
    const planner = new SectionPlanner(externalReferences, links);
    const uniformLayout = planner.plan(false);
    const uniform = LinksPacket.layOut(externalReferences, links, uniformLayout);
    if (!packedWidths) return uniform;
    const packedLayout = planner.plan(true);
    const same =
      packedLayout.length === uniformLayout.length &&
      packedLayout.every(([count, width], index) => {
        const [otherCount, otherWidth] = uniformLayout[index];
        return count === otherCount && width === otherWidth;
      });
    if (same) return uniform;
    const packed = LinksPacket.layOut(externalReferences, links, packedLayout);
    return packed.toBytes().length < uniform.toBytes().length ? packed : uniform;
  }

  static layOut(externalReferences, links, layout) {
    const sections = [];
    let nextAddress = 1n;
    let offset = 0;
    for (const [count, width] of layout) {
      const members = links.slice(offset, offset + count);
      offset += count;
      const start = members[0][0];
      let shortest = Infinity;
      let longest = 0;
      for (const [, link] of members) {
        shortest = Math.min(shortest, link.length);
        longest = Math.max(longest, link.length);
      }
      sections.push(
        new Section(
          start - nextAddress,
          ArityRange.between(shortest, longest),
          width,
          members.map(([, link]) => link),
        ),
      );
      nextAddress = start + BigInt(count);
    }
    return new LinksPacket(externalReferences, sections);
  }

  /** Every link with its address, in address order: `[address, references]`. */
  *links() {
    let nextAddress = 1n;
    for (const section of this.sections) {
      const start = nextAddress + section.gap;
      nextAddress = start + BigInt(section.links.length);
      for (let index = 0; index < section.links.length; index += 1) {
        yield [start + BigInt(index), section.links[index]];
      }
    }
  }

  linkCount() {
    return this.sections.reduce((total, section) => total + BigInt(section.links.length), 0n);
  }

  validate() {
    let nextAddress = 1n;
    for (const section of this.sections) {
      section.validate();
      nextAddress += section.gap + BigInt(section.links.length);
      if (nextAddress > U64_MAX) throw ProtocolError.unencodable('addresses overflow 64 bits');
      for (const link of section.links) {
        for (const reference of link) this.rawReference(reference, section.width);
      }
    }
  }

  isCompact() {
    if (this.sections.length === 0) return true;
    if (this.sections.length > 1) return false;
    const [only] = this.sections;
    return (
      only.gap === COMPACT_GAP && only.arity.equals(ArityRange.DOUBLETS) && only.links.length > 0
    );
  }

  /** Serializes the packet into a Buffer. */
  toBytes() {
    let header = BINARY_VERSION_1;
    if (this.externalReferences) header |= FLAG_EXTERNAL_REFERENCES;
    const out = [];
    this.validate();
    if (this.isCompact()) {
      const width = this.sections.length > 0 ? this.sections[0].width : 1;
      out.push(header | (widthCode(width) << WIDTH_SHIFT));
      writeLeb128(out, this.linkCount());
    } else {
      out.push(header | FLAG_EXPLICIT_LAYOUT);
      writeLeb128(out, this.sections.length);
      for (const section of this.sections) section.writeHeader(out);
    }
    for (const section of this.sections) {
      for (const link of section.links) {
        if (!section.arity.isFixed()) writeLeb128(out, BigInt(link.length) - section.arity.min);
        for (const reference of link) {
          writeRaw(out, this.rawReference(reference, section.width), section.width);
        }
      }
    }
    return Buffer.from(out);
  }

  rawReference(reference, width) {
    if (referenceWidth(reference, this.externalReferences) > width) {
      throw ProtocolError.unencodable(`${debugReference(reference)} does not fit ${width} byte(s)`);
    }
    return isExternal(reference) ? encodeExternal(externalValue(reference), width) : reference;
  }

  /** Parses a complete packet; trailing bytes are an error. */
  static fromBytes(bytes, limits) {
    const reader = new BufferSource(bytes);
    const packet = reader.run(LinksPacket.reader(limits));
    if (packet === null) throw ProtocolError.malformed('empty input');
    if (reader.offset < bytes.length) {
      throw ProtocolError.malformed('trailing bytes after the packet');
    }
    return packet;
  }

  /**
   * The steps reading one packet, `read_from`; it returns null on a clean end
   * of input. Drive it with `BufferSource.run` or an async stream reader.
   */
  static *reader(limits) {
    const first = yield 0;
    if (first.length === 0) return null;
    const header = first[0];
    if ((header & 0xf0) !== BINARY_VERSION_1) {
      const hex = header.toString(16).toUpperCase().padStart(2, '0');
      throw ProtocolError.malformed(`unsupported binary header byte 0x${hex}`);
    }
    const packet = new LinksPacket((header & FLAG_EXTERNAL_REFERENCES) !== 0);
    const counts = [];
    if ((header & FLAG_EXPLICIT_LAYOUT) === 0) {
      const width = widthFromCode(BigInt((header & WIDTH_BITS) >> WIDTH_SHIFT));
      const count = yield* readLeb128();
      if (count > U64_MAX - FIRST_LINK_ADDRESS) {
        throw ProtocolError.malformed('addresses overflow 64 bits');
      }
      if (count > 0n) {
        packet.sections.push(new Section(COMPACT_GAP, ArityRange.DOUBLETS, width));
        counts.push(count);
      }
    } else {
      if ((header & WIDTH_BITS) !== 0) {
        throw ProtocolError.malformed('the explicit layout keeps the header width bits clear');
      }
      const sectionCount = yield* readLeb128();
      if (sectionCount > limits.maxLinks) throw tooManyLinks(limits);
      let nextAddress = 1n;
      for (let index = 0n; index < sectionCount; index += 1n) {
        const [section, count] = yield* Section.readHeader();
        nextAddress += section.gap + count;
        if (nextAddress > U64_MAX) throw ProtocolError.malformed('addresses overflow 64 bits');
        packet.sections.push(section);
        counts.push(count);
      }
    }
    const total = counts.reduce((sum, count) => sum + count, 0n);
    if (total > U64_MAX || total > limits.maxLinks) throw tooManyLinks(limits);
    let referencesLeft = limits.maxReferences;
    const externalReferences = packet.externalReferences;
    for (let index = 0; index < packet.sections.length; index += 1) {
      const section = packet.sections[index];
      for (let link = 0n; link < counts[index]; link += 1n) {
        let length = section.arity.min;
        if (!section.arity.isFixed()) {
          length = (yield* readLeb128()) + section.arity.min;
          if (length > U64_MAX || !section.arity.contains(length)) {
            throw ProtocolError.malformed(`link length outside the section arity ${section.arity}`);
          }
        }
        if (length > referencesLeft) {
          throw ProtocolError.limitExceeded(
            `references exceed the limit of ${limits.maxReferences}`,
          );
        }
        referencesLeft -= length;
        const references = [];
        for (let item = 0n; item < length; item += 1n) {
          const raw = yield* readRaw(section.width);
          const value = externalReferences ? decodeExternal(raw, section.width) : null;
          references.push(value === null ? raw : external(value));
        }
        section.links.push(references);
      }
    }
    return packet;
  }
}

/** Drives a reader over a whole Buffer. */
export class BufferSource {
  constructor(bytes) {
    this.bytes = bytes;
    this.offset = 0;
  }

  run(steps) {
    let step = steps.next();
    while (!step.done) {
      const count = Math.max(step.value, 1);
      const bytes = this.bytes.subarray(this.offset, this.offset + count);
      this.offset += bytes.length;
      step = steps.next(bytes);
    }
    return step.value;
  }
}

// Estimated bytes of a section header, and the extra bytes of a variable
// arity section (its `extra_arity` field and each link's length prefix).
const SECTION_HEADER_ESTIMATE = 2;
const VARIABLE_ARITY_ESTIMATE = 1;
const UNREACHABLE = Infinity;
const STATES = WIDTHS.length * 2;

function firstCheapest(costs) {
  let best = 0;
  for (let state = 1; state < STATES; state += 1) if (costs[state] < costs[best]) best = state;
  return best;
}

/** Splits links into sections with a linear dynamic program, `SectionPlanner`. */
class SectionPlanner {
  constructor(externalReferences, links) {
    this.links = links;
    this.needs = [];
    let previousAddress = 0n;
    for (const [address, link] of links) {
      if (address <= previousAddress) {
        throw ProtocolError.unencodable(
          `link addresses must ascend from 1, got ${address} after ${previousAddress}`,
        );
      }
      if (address === U64_MAX) throw ProtocolError.unencodable('addresses overflow 64 bits');
      if (link.length === 0) {
        throw ProtocolError.unencodable(`link ${address} has no references`);
      }
      previousAddress = address;
      let need = 1;
      for (const reference of link) {
        need = Math.max(need, referenceWidth(reference, externalReferences));
      }
      this.needs.push(need);
    }
  }

  /** The sections as `[link count, width]` pairs. */
  plan(packedWidths) {
    const { links, needs } = this;
    if (links.length === 0) return [];
    const widest = Math.max(...needs);
    const allowed = WIDTHS.map((width) => packedWidths || width === widest);
    const opensSection = new Array(links.length).fill(0);
    const previousBest = new Array(links.length).fill(0);
    let costs = new Array(STATES).fill(UNREACHABLE);
    for (let index = 0; index < links.length; index += 1) {
      const [address, link] = links[index];
      const best = firstCheapest(costs);
      previousBest[index] = best;
      const cheapestBefore = index === 0 ? 0 : costs[best];
      const continues = index > 0 && links[index - 1][0] + 1n === address;
      const sameLength = index > 0 && links[index - 1][1].length === link.length;
      const next = new Array(STATES).fill(UNREACHABLE);
      for (let state = 0; state < STATES; state += 1) {
        const widthIndex = state >> 1;
        const variable = state % 2 === 1;
        const width = WIDTHS[widthIndex];
        if (!allowed[widthIndex] || width < needs[index]) continue;
        const variableEstimate = variable ? VARIABLE_ARITY_ESTIMATE : 0;
        const body = link.length * width + variableEstimate;
        const openingCost = cheapestBefore + SECTION_HEADER_ESTIMATE + variableEstimate;
        const continuingCost = continues && (variable || sameLength) ? costs[state] : UNREACHABLE;
        if (continuingCost <= openingCost) {
          next[state] = continuingCost + body;
        } else {
          next[state] = openingCost + body;
          opensSection[index] |= 1 << state;
        }
      }
      costs = next;
    }
    const sections = [];
    let state = firstCheapest(costs);
    let end = links.length;
    for (let index = links.length - 1; index >= 0; index -= 1) {
      if (opensSection[index] & (1 << state)) {
        sections.push([end - index, WIDTHS[state >> 1]]);
        end = index;
        state = previousBest[index];
      }
    }
    return sections.reverse();
  }
}
