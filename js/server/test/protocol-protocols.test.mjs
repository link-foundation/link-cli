// Ports rust/tests/protocol_transport_tests.rs: text framing, protocol
// detection per message, connections over any byte stream and the limits of
// both protocols (src/protocol-protocols.js). The streams are in memory.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ArityRange, DecodeLimits } from '../src/protocol-binary-packet.js';
import { BinaryLinoOptions } from '../src/protocol-binary.js';
import { IoError, ProtocolError } from '../src/protocol-errors.js';
import { formatDocument, parseDocument } from '../src/protocol-lino.js';
import {
  BinaryLinoProtocol,
  DEFAULT_MAX_TEXT_BYTES,
  LinoConnection,
  MessageFormat,
  ProtocolLimits,
  StreamReader,
  TextLinoProtocol,
  isBinaryStart,
  readAnyDocument,
  readAnyDocumentWithQueryLimits,
  writeMessage,
} from '../src/protocol-protocols.js';
import { QueryLimits } from '../src/query-options.js';

/**
 * An in-memory stream: `read` hands out the queued chunks (an `Error` in the
 * queue is thrown instead), then the empty Buffer of the end of the stream.
 */
class FakeStream {
  constructor(chunks = []) {
    this.chunks = chunks.map((chunk) => (chunk instanceof Error ? chunk : Buffer.from(chunk)));
    this.written = [];
    this.flushes = 0;
    this.timeout = null;
    this.timeouts = [];
    this.closed = false;
    this.failWrites = null;
  }

  async read(max) {
    const chunk = this.chunks.shift();
    if (chunk === undefined) return Buffer.alloc(0);
    if (chunk instanceof Error) throw chunk;
    if (chunk.length > max) {
      this.chunks.unshift(chunk.subarray(max));
      return chunk.subarray(0, max);
    }
    return chunk;
  }

  async write(bytes) {
    if (this.failWrites !== null) throw this.failWrites;
    this.written.push(Buffer.from(bytes));
  }

  async flush() {
    this.flushes += 1;
  }

  shutdown() {
    this.closed = true;
  }

  setReadTimeout(timeout) {
    this.timeout = timeout;
    this.timeouts.push(timeout);
  }

  get readTimeout() {
    return this.timeout;
  }

  /** Everything written, as bytes. */
  wire() {
    return Buffer.concat(this.written);
  }
}

const reader = (...chunks) => new StreamReader(new FakeStream(chunks));

async function rejection(promise) {
  try {
    await promise;
  } catch (error) {
    return error;
  }
  assert.fail('expected a rejection');
}

test('text messages are dot-stuffed', async () => {
  const document = parseDocument(".dot\n'first\n.second'\n(a b)");
  const stream = new FakeStream();
  await new TextLinoProtocol().writeDocument(stream, document);
  const wire = stream.wire();
  assert.equal(wire.toString(), '..dot\n~1{66697273740a2e7365636f6e64}\na b\n.\n');
  assert.equal(stream.flushes, 1);
  // Legacy quoted multiline references must still be unstuffed when read.
  const legacy = reader("..dot\n'first\n..second'\na b\n.\n");
  assert.deepEqual(await new TextLinoProtocol().readDocument(legacy), document);
  const back = reader(wire);
  assert.deepEqual(await new TextLinoProtocol().readDocument(back), document);
  assert.equal(await new TextLinoProtocol().readDocument(back), null);
});

test('text messages preserve CRLF inside lossless reference literals', async () => {
  const document = ['first\r\n.second'];
  const stream = new FakeStream();
  await new TextLinoProtocol().writeDocument(stream, document);
  assert.deepEqual(await new TextLinoProtocol().readDocument(reader(stream.wire())), document);
});

test('frameText frames the empty text as the terminator alone', async () => {
  assert.equal(TextLinoProtocol.frameText('').toString(), '.\n');
  assert.equal(TextLinoProtocol.frameText('a\n.b').toString(), 'a\n..b\n.\n');
  const stream = new FakeStream();
  await TextLinoProtocol.writeText(stream, '(count: 2)');
  assert.equal(stream.wire().toString(), '(count: 2)\n.\n');
});

test('connections decorate any byte stream', async () => {
  const document = parseDocument('() ((1 1))');
  const stream = new FakeStream();
  const connection = new LinoConnection(stream, new TextLinoProtocol());
  assert.equal(connection.protocol.maxTextBytes, DEFAULT_MAX_TEXT_BYTES);
  // Nothing answers, so no reply follows.
  const error = await rejection(connection.request(document));
  assert.equal(error.message, 'malformed message: connection closed before the reply');
  assert.equal(stream.wire().toString(), '() ((1 1))\n.\n');
});

test('a connection request returns the reply', async () => {
  const stream = new FakeStream(['(count: 2)\n.\n', '(1: 1 1)\n.\n']);
  const connection = new LinoConnection(stream, new TextLinoProtocol());
  assert.deepEqual(await connection.request(parseDocument('(count: ())')), [
    { id: 'count', values: ['2'] },
  ]);
  assert.equal(stream.wire().toString(), '(count: ())\n.\n');
  assert.deepEqual(await connection.receive(), [{ id: '1', values: ['1', '1'] }]);
  assert.equal(await connection.receive(), null);
});

test('text messages accept CRLF and reject truncation', async () => {
  const document = await new TextLinoProtocol().readDocument(reader('() ((1 1))\r\n.\r\n'));
  assert.equal(formatDocument(document), '() ((1 1))');
  assert.deepEqual(await new TextLinoProtocol().readDocument(reader('.\n')), []);
  // Without the terminator line, with or without the last newline.
  for (const truncated of ['() ((1 1))\n', '() ((1 1))']) {
    const error = await rejection(new TextLinoProtocol().readDocument(reader(truncated)));
    assert.equal(error.message, "malformed message: stream ended before the '.' terminator line");
  }
  const tiny = new TextLinoProtocol({ maxTextBytes: 8 });
  const error = await rejection(tiny.readDocument(reader(`${'a'.repeat(100)}\n.\n`)));
  assert.ok(error instanceof ProtocolError);
  assert.equal(error.kind, 'limitExceeded');
  assert.equal(error.message, 'limit exceeded: text message longer than 8 bytes');
  const invalid = reader(Buffer.from([0xc3, 0x28, 0x0a, 0x2e, 0x0a]));
  const utf8 = await rejection(new TextLinoProtocol().readDocument(invalid));
  assert.equal(utf8.message, 'malformed message: text message is not valid UTF-8');
});

test('a text message may arrive in many chunks', async () => {
  const chunks = ['(cou', 'nt: ', '())\r', '\n', '.', '\n'];
  const document = await new TextLinoProtocol().readDocument(reader(...chunks));
  assert.equal(formatDocument(document), '(count: ())');
  // The budget of a line is what is left of the limit plus two bytes, room
  // for a CRLF, as Rust's `take(budget).read_until(b'\n')`.
  const exact = new TextLinoProtocol({ maxTextBytes: 3 });
  assert.equal(await exact.readText(reader('abc\n.\n')), 'abc');
  assert.equal(await exact.readText(reader('abc\r\n.\n')), 'abc');
  const over = await rejection(exact.readText(reader('abcde\n.\n')));
  assert.equal(over.message, 'limit exceeded: text message longer than 3 bytes');
  // The second line has what the first one left.
  const lines = await rejection(exact.readText(reader('ab\ncdef\n.\n')));
  assert.equal(lines.kind, 'limitExceeded');
});

test('protocols are detected per message', async () => {
  const document = parseDocument('() ((1 1))');
  const options = new BinaryLinoOptions({ externalReferences: true });
  const stream = new FakeStream();
  await new TextLinoProtocol().writeDocument(stream, document);
  await new BinaryLinoProtocol({ options }).writeDocument(stream, document);
  await new TextLinoProtocol().writeDocument(stream, document);
  const source = reader(stream.wire());
  const limits = new ProtocolLimits();
  const formats = [];
  for (let message = await readAnyDocument(source, limits); message !== null;) {
    const [received, format] = message;
    assert.deepEqual(received, document);
    formats.push(format);
    message = await readAnyDocument(source, limits);
  }
  assert.equal(formats.length, 3);
  assert.equal(formats[0], MessageFormat.TEXT);
  assert.equal(formats[0].isBinary(), false);
  assert.equal(formats[1].isBinary(), true);
  assert.ok(formats[1].options.equals(options));
  assert.equal(formats[2], MessageFormat.TEXT);
});

test('protocol limits split binary and text budgets', async () => {
  const limits = new ProtocolLimits();
  assert.deepEqual(limits.binary, new DecodeLimits());
  assert.equal(limits.maxTextBytes, DEFAULT_MAX_TEXT_BYTES);
  assert.equal(DEFAULT_MAX_TEXT_BYTES, 64 * 1024 * 1024);
  assert.equal(new TextLinoProtocol().maxTextBytes, DEFAULT_MAX_TEXT_BYTES);
  const unlimited = ProtocolLimits.unlimited();
  assert.deepEqual(unlimited.binary, DecodeLimits.unlimited());
  assert.equal(unlimited.maxTextBytes, Number.MAX_SAFE_INTEGER);
  // The text limit of a reply protocol comes from the server limits.
  const tiny = new ProtocolLimits({ maxTextBytes: 8 });
  const error = await rejection(readAnyDocument(reader(`${'a'.repeat(100)}\n.\n`), tiny));
  assert.equal(error.message, 'limit exceeded: text message longer than 8 bytes');
});

test('a message format writes replies in its own protocol', () => {
  const limits = new ProtocolLimits({ maxTextBytes: 9, binary: DecodeLimits.unlimited() });
  const text = MessageFormat.TEXT.protocol(limits);
  assert.ok(text instanceof TextLinoProtocol);
  assert.equal(text.maxTextBytes, 9);
  const options = new BinaryLinoOptions({ packedWidths: true });
  const binary = MessageFormat.binary(options).protocol(limits);
  assert.ok(binary instanceof BinaryLinoProtocol);
  assert.equal(binary.options, options);
  assert.equal(binary.limits, limits.binary);
});

test('a binary message starts with the version nibble', () => {
  const bytes = new BinaryLinoProtocol().encode(parseDocument('(1 1)'));
  assert.equal(isBinaryStart(bytes[0]), true);
  assert.equal(isBinaryStart('('.charCodeAt(0)), false);
  assert.equal(isBinaryStart(0x0a), false);
});

test('the binary protocol refuses to write what its peer would reject', () => {
  // The text parser stops at the same depth, so build the model directly.
  let node = 'x';
  for (let i = 0; i < 70; i++) node = { id: null, values: ['y', node] };
  const document = [node];
  const error = (() => {
    try {
      new BinaryLinoProtocol().encode(document);
    } catch (thrown) {
      return thrown;
    }
    return null;
  })();
  assert.equal(error?.message, 'cannot encode: nesting deeper than 64');
  const trusted = new BinaryLinoProtocol({ limits: DecodeLimits.unlimited() });
  assert.deepEqual(trusted.decode(trusted.encode(document)), document);
});

test('binary documents read back from a stream, one packet at a time', async () => {
  const protocol = new BinaryLinoProtocol();
  const first = parseDocument('() ((1 1))');
  const second = parseDocument('(count: 2)');
  const bytes = Buffer.concat([protocol.encode(first), protocol.encode(second)]);
  // One byte per read: every step of the packet reader waits for more.
  const chunks = [...bytes].map((byte) => Buffer.from([byte]));
  const source = new StreamReader(new FakeStream(chunks));
  assert.deepEqual(await protocol.readDocument(source), first);
  assert.deepEqual(await protocol.readDocument(source), second);
  assert.equal(await protocol.readDocument(source), null);
  // The whole wire in one read: every step finds its bytes buffered.
  const whole = reader(bytes);
  assert.deepEqual(await protocol.readDocument(whole), first);
  assert.deepEqual(await protocol.readDocument(whole), second);
});

test('a binary message cut short is malformed', async () => {
  const bytes = new BinaryLinoProtocol().encode(parseDocument('() ((1 1))'));
  const error = await rejection(
    new BinaryLinoProtocol().readDocument(reader(bytes.subarray(0, 3))),
  );
  assert.ok(error instanceof ProtocolError);
  const any = await rejection(readAnyDocument(reader(bytes.subarray(0, 3)), new ProtocolLimits()));
  assert.ok(any instanceof ProtocolError);
});

test('the stream reader reads exactly, up to the end of the stream', async () => {
  const source = reader('ab', 'cd', 'e');
  assert.equal((await source.readExact(3)).toString(), 'abc');
  assert.equal((await source.readExact(1)).toString(), 'd');
  assert.equal((await source.readExact(5)).toString(), 'e');
  assert.equal((await source.readExact(2)).length, 0);
  const lines = reader('one\ntwo');
  assert.equal((await lines.readUntil(0x0a, 100)).toString(), 'one\n');
  assert.equal((await lines.readUntil(0x0a, 2)).toString(), 'tw');
  assert.equal((await lines.readUntil(0x0a, 100)).toString(), 'o');
  assert.equal((await lines.readUntil(0x0a, 100)).length, 0);
});

test('query limits bound text and binary messages', async () => {
  const limits = new QueryLimits({ ...QueryLimits.causal(), maxBytes: 16, maxDepth: 2 });
  const text = await readAnyDocumentWithQueryLimits(
    reader('(count: ())\n.\n'),
    new ProtocolLimits(),
    limits,
  );
  assert.deepEqual(text, [parseDocument('(count: ())'), MessageFormat.TEXT]);
  const long = await rejection(
    readAnyDocumentWithQueryLimits(
      reader(`(${'a'.repeat(20)})\n.\n`),
      new ProtocolLimits(),
      limits,
    ),
  );
  assert.equal(long.message, 'limit exceeded: text message longer than 16 bytes');
  const deep = await rejection(
    readAnyDocumentWithQueryLimits(reader('(a (b (c (d))))\n.\n'), new ProtocolLimits(), limits),
  );
  assert.match(deep.message, /depth/);
  const packet = new BinaryLinoProtocol().encode(parseDocument('(1 1)'));
  const [document, format] = await readAnyDocumentWithQueryLimits(
    reader(packet),
    new ProtocolLimits(),
    limits,
  );
  assert.deepEqual(document, parseDocument('(1 1)'));
  assert.ok(format.options.equals(new BinaryLinoOptions()));
  // Binary limits tighter than the query ones stay as they are.
  const tight = new ProtocolLimits({
    binary: new DecodeLimits({ maxLinks: 1n, maxReferences: 1n }),
  });
  const refused = await rejection(readAnyDocumentWithQueryLimits(reader(packet), tight, limits));
  assert.ok(refused instanceof ProtocolError);
  assert.equal(await readAnyDocumentWithQueryLimits(reader(), new ProtocolLimits(), limits), null);
});

test('packets of a wider arity are told apart by their options', async () => {
  const options = new BinaryLinoOptions({ arity: ArityRange.between(1, 3) });
  const document = parseDocument('(1 2 3)');
  const bytes = new BinaryLinoProtocol({ options }).encode(document);
  const [received, format] = await readAnyDocument(reader(bytes), new ProtocolLimits());
  assert.deepEqual(received, document);
  assert.equal(format.isBinary(), true);
});

test('transport failures become I/O protocol errors', async () => {
  const failure = new IoError('ConnectionReset', 'connection reset by peer');
  const read = await rejection(new TextLinoProtocol().readText(reader(failure)));
  assert.ok(read instanceof ProtocolError);
  assert.equal(read.kind, 'io');
  assert.equal(read.cause, failure);
  const binary = await rejection(new BinaryLinoProtocol().readDocument(reader(failure)));
  assert.equal(binary.kind, 'io');
  const any = await rejection(readAnyDocument(reader(failure), new ProtocolLimits()));
  assert.equal(any.kind, 'io');
  const stream = new FakeStream();
  stream.failWrites = new IoError('BrokenPipe', 'broken pipe');
  const write = await rejection(writeMessage(stream, Buffer.from('x')));
  assert.equal(write.kind, 'io');
  assert.equal(write.cause, stream.failWrites);
  // A protocol error passes through as it is.
  const malformed = ProtocolError.malformed('m');
  stream.failWrites = malformed;
  assert.equal(await rejection(writeMessage(stream, Buffer.from('x'))), malformed);
});

test('waiting for a message restores the read timeout', async () => {
  const stream = new FakeStream(['(notify: 1)\n.\n']);
  stream.timeout = 250;
  const connection = new LinoConnection(stream, new TextLinoProtocol());
  assert.equal(await connection.waitForMessage(0), true);
  assert.deepEqual(stream.timeouts, [1, 250]);
  // The message is buffered now: no read is needed.
  assert.equal(await connection.waitForMessage(10), true);
  assert.deepEqual(stream.timeouts, [1, 250]);
  assert.deepEqual(await connection.receive(), [{ id: 'notify', values: ['1'] }]);
  // At the end of the stream a message "arrives": the receive finds it closed.
  assert.equal(await connection.waitForMessage(10), true);
  assert.deepEqual(stream.timeouts, [1, 250, 10, 250]);
});

test('waiting for a message that does not come in time is false', async () => {
  for (const kind of ['WouldBlock', 'TimedOut', 'Interrupted']) {
    const stream = new FakeStream([new IoError(kind, 'no data')]);
    const connection = new LinoConnection(stream, new TextLinoProtocol());
    assert.equal(await connection.waitForMessage(5), false, kind);
    assert.deepEqual(stream.timeouts, [5, null]);
  }
  // A Node.js error code maps to its kind as well.
  const again = Object.assign(new Error('resource temporarily unavailable'), { code: 'EAGAIN' });
  const connection = new LinoConnection(new FakeStream([again]), new TextLinoProtocol());
  assert.equal(await connection.waitForMessage(5), false);
});

test('waiting for a message fails on any other error', async () => {
  const failure = new IoError('ConnectionReset', 'connection reset by peer');
  const stream = new FakeStream([failure]);
  const connection = new LinoConnection(stream, new TextLinoProtocol());
  const error = await rejection(connection.waitForMessage(5));
  assert.ok(error instanceof ProtocolError);
  assert.equal(error.kind, 'io');
  assert.equal(error.cause, failure);
  assert.deepEqual(stream.timeouts, [5, null]);
});

test('a connection speaks binary just as well', async () => {
  const protocol = new BinaryLinoProtocol();
  const reply = parseDocument('(count: 1)');
  const stream = new FakeStream([protocol.encode(reply)]);
  const connection = new LinoConnection(stream, protocol);
  assert.deepEqual(await connection.request(parseDocument('(count: ())')), reply);
  assert.deepEqual(protocol.decode(stream.wire()), parseDocument('(count: ())'));
});
