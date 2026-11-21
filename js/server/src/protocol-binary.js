// The lossless mapping between LiNo documents and binary links packets: ports
// links-notation 0.31.6 src/binary/mapping.rs (`BinaryLinoOptions`,
// `encode_document_with_limits`, `decode_document`) and the `BinaryLinoCodec`
// of src/binary/mod.rs. The npm links-notation package has an encoder of its
// own; this port keeps Rust's link order and error texts.
import { byteLength, isRef } from './protocol-lino.js';
import {
  ArityRange,
  DecodeLimits,
  FIRST_LINK_ADDRESS,
  IDENTIFIED,
  LIST,
  LinksPacket,
  NULL,
  NUMBER,
  ONE,
  STRING,
  debugReference,
  external,
  externalCapacity,
  externalValue,
  isExternal,
} from './protocol-binary-packet.js';
import { ProtocolError } from './protocol-errors.js';

const MAX_EXTERNAL = externalCapacity(8);
const U64_MAX = 0xffffffffffffffffn;

/** Optional features of the binary LiNo protocol; all off by default. */
export class BinaryLinoOptions {
  constructor({
    externalReferences = false,
    arity = ArityRange.DOUBLETS,
    packedWidths = false,
  } = {}) {
    this.externalReferences = externalReferences;
    this.arity = arity;
    this.packedWidths = packedWidths;
  }

  equals(other) {
    return (
      this.externalReferences === other.externalReferences &&
      this.arity.equals(other.arity) &&
      this.packedWidths === other.packedWidths
    );
  }

  /** The options a peer most likely used to write `packet`, `of_packet`. */
  static ofPacket(packet) {
    let shortest = 2;
    let longest = 2;
    for (const [, link] of packet.links()) {
      shortest = Math.min(shortest, link.length);
      longest = Math.max(longest, link.length);
    }
    const [first, ...rest] = packet.sections.map((section) => section.width);
    return new BinaryLinoOptions({
      externalReferences: packet.externalReferences,
      arity: ArityRange.between(shortest, longest),
      packedWidths: rest.some((width) => width !== first),
    });
  }
}

/** `canonical_number`: an unsigned decimal without sign or leading zeros, as a BigInt. */
export function canonicalNumber(text) {
  if (!/^(0|[1-9][0-9]*)$/.test(text)) return null;
  const value = BigInt(text);
  return value > U64_MAX ? null : value;
}

// Encoder nodes are strings so equal links share one map key: `i<address>`,
// `e<value>`, `d<doublet index>` and `t<tuple index>`.
const internalNode = (address) => `i${address}`;

class Encoder {
  constructor(options) {
    this.options = options;
    this.doublets = [];
    this.tuples = [];
    this.created = new Map();
    this.powers = [internalNode(ONE)];
  }

  link(items) {
    const key = items.join(',');
    const known = this.created.get(key);
    if (known !== undefined) return known;
    let node;
    if (items.length === 2 && !items.some((item) => item[0] === 't')) {
      this.doublets.push(items);
      node = `d${this.doublets.length - 1}`;
    } else {
      this.tuples.push(items);
      node = `t${this.tuples.length - 1}`;
    }
    this.created.set(key, node);
    return node;
  }

  pair(first, second) {
    return this.link([first, second]);
  }

  chain(items) {
    let tail = internalNode(NULL);
    for (let index = items.length - 1; index >= 0; index -= 1) tail = this.pair(items[index], tail);
    return tail;
  }

  fitsOneLink(items) {
    return items !== 2 && this.options.arity.contains(items);
  }

  typed(marker, elements) {
    if (this.fitsOneLink(elements.length + 1))
      return this.link([internalNode(marker), ...elements]);
    return this.pair(internalNode(marker), this.chain(elements));
  }

  list(elements) {
    if (elements.length === 0) return internalNode(NULL);
    if (elements.length === 2) return this.pair(elements[0], elements[1]);
    if (this.fitsOneLink(elements.length)) return this.link(elements);
    return this.typed(LIST, elements);
  }

  power(exponent) {
    while (this.powers.length <= exponent) {
      const previous = this.powers[this.powers.length - 1];
      this.powers.push(this.pair(previous, previous));
    }
    return this.powers[exponent];
  }

  unary(value) {
    const powers = [];
    for (let bit = 63; bit >= 0; bit -= 1) {
      if (value & (1n << BigInt(bit))) powers.push(this.power(bit));
    }
    if (powers.length === 0) return internalNode(NULL);
    let sum = powers[powers.length - 1];
    for (let index = powers.length - 2; index >= 0; index -= 1) sum = this.pair(powers[index], sum);
    return sum;
  }

  scalar(value) {
    if (this.options.externalReferences && value <= MAX_EXTERNAL) return `e${value}`;
    return this.unary(value);
  }

  reference(text) {
    const value = canonicalNumber(text);
    if (value !== null) {
      if (this.options.externalReferences && value <= MAX_EXTERNAL) return `e${value}`;
      return this.pair(internalNode(NUMBER), this.unary(value));
    }
    const codePoints = [];
    for (const character of text) codePoints.push(this.scalar(BigInt(character.codePointAt(0))));
    return this.typed(STRING, codePoints);
  }

  encode(node) {
    if (isRef(node)) return this.reference(node);
    if (node.id === null) return this.list(node.values.map((value) => this.encode(value)));
    const elements = [this.reference(node.id)];
    for (const value of node.values) elements.push(this.encode(value));
    return this.typed(IDENTIFIED, elements);
  }

  finish() {
    const doubletCount = BigInt(this.doublets.length);
    const resolve = (node) => {
      const number = BigInt(node.slice(1));
      switch (node[0]) {
        case 'i':
          return number;
        case 'e':
          return external(number);
        case 'd':
          return FIRST_LINK_ADDRESS + number;
        default:
          return FIRST_LINK_ADDRESS + doubletCount + number;
      }
    };
    const links = [...this.doublets, ...this.tuples].map((items, index) => [
      FIRST_LINK_ADDRESS + BigInt(index),
      items.map(resolve),
    ]);
    return LinksPacket.pack(this.options.externalReferences, links, this.options.packedWidths);
  }
}

/** `encode_document_with_limits`: a document as a packet, checked against `limits`. */
export function encodeDocumentWithLimits(document, options, limits = new DecodeLimits()) {
  const problem = options.arity.problem();
  if (problem !== null) throw ProtocolError.unencodable(problem);
  if (!options.arity.contains(2)) {
    throw ProtocolError.unencodable(`arity ${options.arity} does not include doublets (2)`);
  }
  const pending = document.map((node) => [node, 0]);
  let budget = limits.maxNodes;
  let stringsLeft = limits.maxStringBytes;
  const tooManyNodes = () => ProtocolError.unencodable('too many LiNo nodes');
  while (pending.length > 0) {
    const [node, depth] = pending.pop();
    if (depth >= limits.maxDepth) {
      throw ProtocolError.unencodable(`nesting deeper than ${limits.maxDepth}`);
    }
    if (budget === 0) throw tooManyNodes();
    budget -= 1;
    let text;
    if (isRef(node)) {
      text = node;
    } else {
      for (const value of node.values) pending.push([value, depth + 1]);
      if (node.id !== null) {
        if (budget === 0) throw tooManyNodes();
        budget -= 1;
      }
      text = node.id;
    }
    if (text !== null) {
      const length = byteLength(text);
      if (length > stringsLeft) throw ProtocolError.unencodable('too many string bytes');
      stringsLeft -= length;
    }
  }
  const encoder = new Encoder(options);
  if (document.length > 0) encoder.list(document.map((node) => encoder.encode(node)));
  const packet = encoder.finish();
  if (BigInt(packet.sections.length) > limits.maxLinks || packet.linkCount() > limits.maxLinks) {
    throw ProtocolError.unencodable('too many packet links');
  }
  let referencesLeft = limits.maxReferences;
  for (const [, link] of packet.links()) {
    if (BigInt(link.length) > referencesLeft) {
      throw ProtocolError.unencodable('too many packet references');
    }
    referencesLeft -= BigInt(link.length);
  }
  return packet;
}

const isMarker = (reference) => reference >= ONE && reference < FIRST_LINK_ADDRESS;
const startsWithMarker = (items) => items.length > 0 && isMarker(items[0]);
const expectedUnary = () => ProtocolError.malformed('expected a unary number');

class Decoder {
  constructor(packet, limits) {
    if (BigInt(packet.sections.length) > limits.maxLinks || packet.linkCount() > limits.maxLinks) {
      throw ProtocolError.limitExceeded(`packet declares more than ${limits.maxLinks} links`);
    }
    try {
      packet.validate();
    } catch (error) {
      throw ProtocolError.malformed(error.message);
    }
    this.limits = limits;
    this.stringBytesLeft = limits.maxStringBytes;
    this.links = [];
    this.unary = [];
    let referencesLeft = limits.maxReferences;
    for (const [address, link] of packet.links()) {
      if (BigInt(link.length) > referencesLeft) {
        throw ProtocolError.limitExceeded(`references exceed the limit of ${limits.maxReferences}`);
      }
      referencesLeft -= BigInt(link.length);
      const expected = FIRST_LINK_ADDRESS + BigInt(this.links.length);
      if (address !== expected) {
        throw ProtocolError.malformed(
          `a LiNo packet stores its links contiguously from address ${FIRST_LINK_ADDRESS}, ` +
            `found link ${address} where ${expected} belongs`,
        );
      }
      const target = link.find((reference) => !isExternal(reference) && reference >= address);
      if (target !== undefined) {
        throw ProtocolError.malformed(
          `link ${address} refers to ${debugReference(target)}, which is not an earlier link`,
        );
      }
      // Links only refer backwards, so one forward pass evaluates every unary number.
      const valueOf = (reference) => {
        if (reference === NULL) return 0n;
        if (reference === ONE) return 1n;
        if (reference >= FIRST_LINK_ADDRESS)
          return this.unary[Number(reference - FIRST_LINK_ADDRESS)];
        return null;
      };
      let value = null;
      if (link.length === 2) {
        const source = valueOf(link[0]);
        const target2 = valueOf(link[1]);
        if (source !== null && target2 !== null && source + target2 <= U64_MAX) {
          value = source + target2;
        }
      }
      this.unary.push(value);
      this.links.push(link);
    }
  }

  /** `view`: `{ kind: 'null' | 'marker' | 'external' | 'link', … }`. */
  view(reference) {
    if (isExternal(reference)) return { kind: 'external', value: externalValue(reference) };
    if (reference === NULL) return { kind: 'null' };
    if (reference < FIRST_LINK_ADDRESS) return { kind: 'marker', marker: reference };
    return { kind: 'link', items: this.links[Number(reference - FIRST_LINK_ADDRESS)] };
  }

  number(reference) {
    if (isExternal(reference)) return externalValue(reference);
    if (reference === NULL) return 0n;
    if (reference === ONE) return 1n;
    if (reference >= FIRST_LINK_ADDRESS) {
      const value = this.unary[Number(reference - FIRST_LINK_ADDRESS)];
      if (value === null) throw expectedUnary();
      return value;
    }
    throw expectedUnary();
  }

  /** The elements of the cons list `(e1 (e2 (… (en 0))))`. */
  chain(reference) {
    const elements = [];
    let tail = reference;
    for (;;) {
      const view = this.view(tail);
      if (view.kind === 'null') return elements;
      if (view.kind !== 'link' || view.items.length !== 2) {
        throw ProtocolError.malformed('broken element chain');
      }
      if (elements.length >= this.limits.maxNodes) {
        throw ProtocolError.limitExceeded('chain too long');
      }
      elements.push(view.items[0]);
      tail = view.items[1];
    }
  }

  typed(marker, elements, depth, budget) {
    switch (marker) {
      case NUMBER:
        if (elements.length !== 1) {
          throw ProtocolError.malformed('a number needs exactly one value');
        }
        return this.referenceText(String(this.number(elements[0])));
      case STRING: {
        let text = '';
        let bytes = 0;
        for (const element of elements) {
          const codePoint = this.number(element);
          if (codePoint > 0x10ffffn || (codePoint >= 0xd800n && codePoint <= 0xdfffn)) {
            throw ProtocolError.malformed(`invalid code point ${codePoint}`);
          }
          const character = String.fromCodePoint(Number(codePoint));
          const length = byteLength(character);
          if (length > Math.max(this.stringBytesLeft - bytes, 0)) {
            throw ProtocolError.limitExceeded('too many expanded string bytes');
          }
          text += character;
          bytes += length;
        }
        return this.referenceText(text);
      }
      case LIST:
        return this.list(elements, depth, budget);
      case IDENTIFIED: {
        if (elements.length === 0) {
          throw ProtocolError.malformed('an identified link needs an id');
        }
        const id = this.decode(elements[0], depth, budget);
        if (!isRef(id)) throw ProtocolError.malformed('a link id must be a reference');
        return { id, values: this.values(elements.slice(1), depth, budget) };
      }
      default:
        throw ProtocolError.malformed(`marker ${marker} cannot start a typed value`);
    }
  }

  list(elements, depth, budget) {
    return { id: null, values: this.values(elements, depth, budget) };
  }

  values(elements, depth, budget) {
    return elements.map((element) => this.decode(element, depth + 1, budget));
  }

  referenceText(text) {
    const length = byteLength(text);
    if (length > this.stringBytesLeft) {
      throw ProtocolError.limitExceeded('too many expanded string bytes');
    }
    this.stringBytesLeft -= length;
    return text;
  }

  decode(reference, depth, budget) {
    if (depth >= this.limits.maxDepth) {
      throw ProtocolError.limitExceeded(`nesting deeper than ${this.limits.maxDepth}`);
    }
    if (budget.left === 0) throw ProtocolError.limitExceeded('too many LiNo nodes');
    budget.left -= 1;
    const view = this.view(reference);
    switch (view.kind) {
      case 'null':
        return { id: null, values: [] };
      case 'external':
        return this.referenceText(String(view.value));
      case 'marker':
        throw ProtocolError.malformed(`marker ${view.marker} used as a value`);
      default: {
        const { items } = view;
        if (items.length === 2 && items[0] === NUMBER) {
          return this.typed(NUMBER, [items[1]], depth, budget);
        }
        if (items.length === 2 && isMarker(items[0])) {
          return this.typed(items[0], this.chain(items[1]), depth, budget);
        }
        if (startsWithMarker(items)) return this.typed(items[0], items.slice(1), depth, budget);
        return this.list(items, depth, budget);
      }
    }
  }
}

/** `decode_document`: a packet back into a document. */
export function decodeDocument(packet, limits) {
  const decoder = new Decoder(packet, limits);
  if (decoder.links.length === 0) return [];
  const items = decoder.links[decoder.links.length - 1];
  let elements;
  if (items.length === 2 && items[0] === LIST) elements = decoder.chain(items[1]);
  else if (!startsWithMarker(items)) elements = items;
  else throw ProtocolError.malformed('the root link is not a list');
  const budget = { left: limits.maxNodes };
  return elements.map((item) => decoder.decode(item, 0, budget));
}

/** `BinaryLinoCodec`: complete packets in one call. */
export class BinaryLinoCodec {
  constructor(options = new BinaryLinoOptions(), limits = new DecodeLimits()) {
    this.options = options;
    this.limits = limits;
  }

  encode(document) {
    return encodeDocumentWithLimits(document, this.options, this.limits).toBytes();
  }

  decode(bytes) {
    return decodeDocument(LinksPacket.fromBytes(bytes, this.limits), this.limits);
  }
}
