// Interchangeable message protocols for LiNo documents: ports
// rust/src/protocol/protocols.rs (`TextLinoProtocol`, `BinaryLinoProtocol`,
// `ProtocolLimits`, `MessageFormat`, `read_any_document`,
// `read_any_document_with_query_limits` and `LinoConnection`).
//
// Rust reads through a `BufReader`; `StreamReader` is its asynchronous
// counterpart over an `IpcStream` (`async read(max)`, an empty Buffer at the
// end of the stream).
import { BINARY_VERSION_1, DecodeLimits, LinksPacket } from './protocol-binary-packet.js';
import { BinaryLinoOptions, decodeDocument, encodeDocumentWithLimits } from './protocol-binary.js';
import { ProtocolError, ioErrorKind, protocolError } from './protocol-errors.js';
import { formatDocument, parseDocument, parseDocumentWithLimits } from './protocol-lino.js';

/** The default limit of a text message: 64 MiB. */
export const DEFAULT_MAX_TEXT_BYTES = 64 << 20;

const BUFFER_CAPACITY = 8192;

/** Limits applied to incoming messages of either protocol. */
export class ProtocolLimits {
  constructor({ binary = new DecodeLimits(), maxTextBytes = DEFAULT_MAX_TEXT_BYTES } = {}) {
    this.binary = binary;
    this.maxTextBytes = maxTextBytes;
  }

  /** Limits for trusted input: only the address space bounds a message. */
  static unlimited() {
    return new ProtocolLimits({
      binary: DecodeLimits.unlimited(),
      maxTextBytes: Number.MAX_SAFE_INTEGER,
    });
  }
}

/** A buffered reader over a stream, `BufReader`. Read errors are thrown as they come. */
export class StreamReader {
  constructor(stream) {
    this.stream = stream;
    this.buffer = Buffer.alloc(0);
  }

  /** `fill_buf`: the buffered bytes, reading more when there are none; empty at the end. */
  async fill() {
    if (this.buffer.length === 0) this.buffer = await this.stream.read(BUFFER_CAPACITY);
    return this.buffer;
  }

  consume(count) {
    this.buffer = this.buffer.subarray(count);
  }

  /** `take(budget).read_until(byte)`: the bytes up to and including `byte`. */
  async readUntil(byte, budget) {
    const parts = [];
    let left = budget;
    while (left > 0) {
      const available = await this.fill();
      if (available.length === 0) break;
      const window = available.subarray(0, Math.min(available.length, left));
      const end = window.indexOf(byte);
      const taken = end < 0 ? window : window.subarray(0, end + 1);
      parts.push(taken);
      this.consume(taken.length);
      left -= taken.length;
      if (end >= 0) break;
    }
    return Buffer.concat(parts);
  }

  /** Up to `count` bytes, fewer only at the end of the stream. */
  async readExact(count) {
    const parts = [];
    let left = count;
    while (left > 0) {
      const available = await this.fill();
      if (available.length === 0) break;
      const taken = available.subarray(0, Math.min(available.length, left));
      parts.push(taken);
      this.consume(taken.length);
      left -= taken.length;
    }
    return parts.length === 1 ? parts[0] : Buffer.concat(parts);
  }

  /** Drives the steps of a packet reader (see `LinksPacket.reader`). */
  async run(steps) {
    let step = steps.next();
    while (!step.done) {
      const count = Math.max(step.value, 1);
      const available = this.buffer;
      let bytes;
      if (available.length >= count) {
        bytes = available.subarray(0, count);
        this.consume(count);
      } else {
        bytes = await this.readExact(count);
      }
      step = steps.next(bytes);
    }
    return step.value;
  }
}

/** Wraps errors of the transport into `ProtocolError::Io`. */
async function transport(operation) {
  try {
    return await operation();
  } catch (error) {
    throw protocolError(error);
  }
}

/** Writes a whole message and flushes, `write_all` + `flush`. */
export async function writeMessage(stream, bytes) {
  await transport(async () => {
    await stream.write(bytes);
    await stream.flush();
  });
}

/**
 * UTF-8 LiNo text, one message per block of lines ended by a line holding
 * only `.`; lines starting with `.` get one more (SMTP dot-stuffing).
 */
export class TextLinoProtocol {
  constructor({ maxTextBytes = DEFAULT_MAX_TEXT_BYTES } = {}) {
    this.maxTextBytes = maxTextBytes;
  }

  /** The framed bytes of already formatted LiNo text. */
  static frameText(text) {
    let out = '';
    if (text !== '') {
      for (const line of text.split('\n')) out += `${line.startsWith('.') ? '.' : ''}${line}\n`;
    }
    return Buffer.from(`${out}.\n`, 'utf8');
  }

  /** `write_text`: frames text as one message on `stream`. */
  static async writeText(stream, text) {
    await writeMessage(stream, TextLinoProtocol.frameText(text));
  }

  /** `read_text`: one framed message as raw text, or null at a clean end. */
  async readText(reader) {
    const parts = [];
    let length = 0;
    let first = true;
    for (;;) {
      const budget = Math.max(this.maxTextBytes - length, 0) + 2;
      let line = await transport(() => reader.readUntil(0x0a, budget));
      if (line.length === 0) {
        if (first) return null;
        throw ProtocolError.malformed("stream ended before the '.' terminator line");
      }
      if (line[line.length - 1] !== 0x0a) {
        if (line.length >= budget) {
          throw ProtocolError.limitExceeded(`text message longer than ${this.maxTextBytes} bytes`);
        }
        throw ProtocolError.malformed("stream ended before the '.' terminator line");
      }
      line = line.subarray(0, line.length - 1);
      if (line[line.length - 1] === 0x0d) line = line.subarray(0, line.length - 1);
      if (line.length === 1 && line[0] === 0x2e) break;
      if (!first) {
        parts.push(Buffer.from('\n'));
        length += 1;
      }
      first = false;
      const content = line[0] === 0x2e ? line.subarray(1) : line;
      parts.push(content);
      length += content.length;
    }
    const bytes = Buffer.concat(parts);
    let text;
    try {
      text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes);
    } catch {
      throw ProtocolError.malformed('text message is not valid UTF-8');
    }
    return text;
  }

  encode(document) {
    return TextLinoProtocol.frameText(formatDocument(document));
  }

  async writeDocument(stream, document) {
    await writeMessage(stream, this.encode(document));
  }

  async readDocument(reader) {
    const text = await this.readText(reader);
    return text === null ? null : parseDocument(text);
  }
}

/** Binary links packets; every message is self-delimiting. */
export class BinaryLinoProtocol {
  constructor({ options = new BinaryLinoOptions(), limits = new DecodeLimits() } = {}) {
    this.options = options;
    this.limits = limits;
  }

  /** A document as packet bytes. */
  encode(document) {
    return encodeDocumentWithLimits(document, this.options, this.limits).toBytes();
  }

  /** Packet bytes as a document. */
  decode(bytes) {
    return decodeDocument(LinksPacket.fromBytes(bytes, this.limits), this.limits);
  }

  async writeDocument(stream, document) {
    await writeMessage(stream, this.encode(document));
  }

  async readDocument(reader) {
    const packet = await transport(() => reader.run(LinksPacket.reader(this.limits)));
    return packet === null ? null : decodeDocument(packet, this.limits);
  }
}

/** The wire format a message arrived in, so a reply can use the same one. */
export class MessageFormat {
  constructor(options) {
    /** null for text, the `BinaryLinoOptions` of a binary message. */
    this.options = options;
  }

  static binary(options) {
    return new MessageFormat(options);
  }

  isBinary() {
    return this.options !== null;
  }

  /** A protocol that writes messages in this format. */
  protocol(limits) {
    if (this.options === null) return new TextLinoProtocol({ maxTextBytes: limits.maxTextBytes });
    return new BinaryLinoProtocol({ options: this.options, limits: limits.binary });
  }
}

MessageFormat.TEXT = new MessageFormat(null);

/** True when `byte` starts a binary message rather than a text one. */
export const isBinaryStart = (byte) => (byte & 0xf0) === BINARY_VERSION_1;

/** Reads one message in whichever protocol the peer used: `[document, format]` or null. */
export function readAnyDocument(reader, limits) {
  return readAnyDocumentWithQueryLimits(reader, limits, null);
}

/** `read_any_document_with_query_limits`: checked limits bound framing and decoding. */
export async function readAnyDocumentWithQueryLimits(reader, limits, queryLimits) {
  let maxTextBytes = limits.maxTextBytes;
  let binary = limits.binary;
  if (queryLimits) {
    maxTextBytes = Math.min(maxTextBytes, queryLimits.maxBytes);
    const maxNodes = BigInt(queryLimits.maxNodes);
    binary = new DecodeLimits({
      maxDepth: Math.min(binary.maxDepth, queryLimits.maxDepth),
      maxNodes: Math.min(binary.maxNodes, queryLimits.maxNodes),
      maxStringBytes: Math.min(binary.maxStringBytes, queryLimits.maxBytes),
      maxLinks: binary.maxLinks < maxNodes ? binary.maxLinks : maxNodes,
      maxReferences: binary.maxReferences < maxNodes ? binary.maxReferences : maxNodes,
    });
  }
  const available = await transport(() => reader.fill());
  if (available.length === 0) return null;
  if (!isBinaryStart(available[0])) {
    // The byte just seen starts the message, so it is never a clean end.
    const text = await new TextLinoProtocol({ maxTextBytes }).readText(reader);
    const document = queryLimits ? parseDocumentWithLimits(text, queryLimits) : parseDocument(text);
    return [document, MessageFormat.TEXT];
  }
  const packet = await transport(() => reader.run(LinksPacket.reader(binary)));
  const document = decodeDocument(packet, binary);
  return [document, MessageFormat.binary(BinaryLinoOptions.ofPacket(packet))];
}

/** A byte stream decorated with a protocol. */
export class LinoConnection {
  constructor(stream, protocol) {
    this.stream = stream;
    this.reader = new StreamReader(stream);
    this.protocol = protocol;
  }

  /** Sends one document. */
  async send(document) {
    await writeMessage(this.stream, this.protocol.encode(document));
  }

  /** Receives one document; null when the peer closed the stream. */
  receive() {
    return this.protocol.readDocument(this.reader);
  }

  /** Sends `document` and waits for the reply. */
  async request(document) {
    await this.send(document);
    const reply = await this.receive();
    if (reply === null) throw ProtocolError.malformed('connection closed before the reply');
    return reply;
  }

  /**
   * Waits up to `timeoutMs` for the next message to start arriving; false
   * when none did. True as well when the peer closed the stream.
   */
  async waitForMessage(timeoutMs) {
    if (this.reader.buffer.length > 0) return true;
    const previous = this.stream.readTimeout;
    this.stream.setReadTimeout(Math.max(timeoutMs, 1));
    let failure = null;
    try {
      await this.reader.fill();
    } catch (error) {
      failure = error;
    }
    this.stream.setReadTimeout(previous);
    if (failure === null) return true;
    if (['WouldBlock', 'TimedOut', 'Interrupted'].includes(ioErrorKind(failure))) return false;
    throw protocolError(failure);
  }
}
