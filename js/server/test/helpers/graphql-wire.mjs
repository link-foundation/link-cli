// A client of the GraphQL server on raw sockets, as rust/tests/graphql_server.rs
// and rust/tests/graphql_server_protocol.rs drive it: HTTP requests written
// byte for byte, and WebSocket frames masked as a client must mask them.
import assert from 'node:assert/strict';
import { connect } from 'node:net';

const INIT = '{"type":"connection_init"}';

/** `[host, port]` of an address as `SocketAddr` prints it. */
export function hostAndPort(address) {
  const colon = address.lastIndexOf(':');
  return [address.slice(0, colon).replace(/^\[(.*)\]$/, '$1'), Number(address.slice(colon + 1))];
}

/** A client frame: `finAndOpcode`, the length in its shortest form, the payload. */
export function clientFrame(finAndOpcode, payload, masked = true) {
  const bytes = Buffer.from(payload);
  const maskBit = masked ? 0x80 : 0;
  let head;
  if (bytes.length <= 125) head = Buffer.from([finAndOpcode, maskBit | bytes.length]);
  else if (bytes.length <= 65535) {
    head = Buffer.from([finAndOpcode, maskBit | 126, 0, 0]);
    head.writeUInt16BE(bytes.length, 2);
  } else {
    head = Buffer.alloc(10);
    head[0] = finAndOpcode;
    head[1] = maskBit | 127;
    head.writeBigUInt64BE(BigInt(bytes.length), 2);
  }
  if (!masked) return Buffer.concat([head, bytes]);
  const mask = [9, 8, 7, 6];
  return Buffer.concat([head, Buffer.from(mask), bytes.map((byte, at) => byte ^ mask[at % 4])]);
}

/** One connection: what it reads is kept until asked for. */
export class Wire {
  #socket;
  #buffer = Buffer.alloc(0);
  #ended = false;
  #wake = () => {};

  constructor(socket) {
    this.#socket = socket;
    socket.on('data', (chunk) => {
      this.#buffer = Buffer.concat([this.#buffer, chunk]);
      this.#wake();
    });
    const end = () => {
      this.#ended = true;
      this.#wake();
    };
    socket.on('end', end);
    socket.on('close', end);
    socket.on('error', end);
  }

  /** Connects to `address`; `allowHalfOpen` keeps the connection when the server ends its side. */
  static connect(address, { allowHalfOpen = false } = {}) {
    const [host, port] = hostAndPort(address);
    return new Promise((resolve, reject) => {
      const socket = connect({ host, port, allowHalfOpen }, () => resolve(new Wire(socket)));
      socket.once('error', reject);
    });
  }

  get socket() {
    return this.#socket;
  }

  #more() {
    return new Promise((resolve) => {
      this.#wake = resolve;
    });
  }

  write(bytes) {
    this.#socket.write(bytes);
  }

  /** Ends the writing side, as `shutdown(Shutdown::Write)`. */
  finish() {
    this.#socket.end();
  }

  destroy() {
    this.#socket.destroy();
  }

  async readExact(count) {
    while (this.#buffer.length < count) {
      assert.ok(!this.#ended, 'the server hung up');
      await this.#more();
    }
    const taken = this.#buffer.subarray(0, count);
    this.#buffer = this.#buffer.subarray(count);
    return taken;
  }

  async readLine() {
    for (;;) {
      const at = this.#buffer.indexOf(10);
      if (at >= 0) return (await this.readExact(at + 1)).toString('utf8');
      assert.ok(!this.#ended, 'the server hung up');
      await this.#more();
    }
  }

  /** Everything up to the end of the connection. */
  async rest() {
    while (!this.#ended) await this.#more();
    return this.#buffer;
  }

  /** Sends `request` and reads one response: `[head, body]`. */
  async exchange(request) {
    this.write(request);
    let head = '';
    let length = 0;
    for (;;) {
      const line = await this.readLine();
      if (line.trim() === '') break;
      const value = /^content-length:(.*)$/is.exec(line);
      if (value !== null) length = Number(value[1].trim());
      head += line;
    }
    return [head, (await this.readExact(length)).toString('utf8')];
  }

  /** The body of a POST of `body` to /v1/graphql, with `headers` as they are. */
  async post(body, headers = '') {
    const length = Buffer.byteLength(body);
    const request = `POST /v1/graphql HTTP/1.1\r\nHost: x\r\n${headers}Content-Length: ${length}\r\n\r\n${body}`;
    return (await this.exchange(request))[1];
  }

  /** Upgrades on `path` with the extra request lines `lines`; the head of the answer. */
  async upgrade(path, lines = '') {
    const [head] = await this.exchange(
      `GET ${path} HTTP/1.1\r\nUpgrade: WebSocket\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n${lines}\r\n`,
    );
    assert.ok(head.startsWith('HTTP/1.1 101'), head);
    return head;
  }

  send(text) {
    this.write(clientFrame(0x81, text));
  }

  init() {
    this.send(INIT);
    return this.kind().then((kind) => assert.equal(kind, 'connection_ack'));
  }

  subscribe(kind, id, query) {
    this.send(JSON.stringify({ id, type: kind, payload: { query } }));
  }

  /** One frame: `[opcode, payload]`. */
  async raw() {
    const head = await this.readExact(2);
    let length = head[1] & 0x7f;
    if (length === 126) length = (await this.readExact(2)).readUInt16BE(0);
    else if (length === 127) length = Number((await this.readExact(8)).readBigUInt64BE(0));
    return [head[0] & 0x0f, Buffer.from(await this.readExact(length))];
  }

  /** One text message, parsed. */
  async message() {
    const [opcode, payload] = await this.raw();
    assert.equal(opcode, 1);
    return JSON.parse(payload.toString('utf8'));
  }

  async kind() {
    return (await this.message()).type;
  }

  /** The server hung up: nothing more can be read. */
  async assertClosed() {
    assert.equal((await this.rest()).length, 0, 'still talking');
  }
}

/** A WebSocket on `path` of `address`: the modern protocol unless `legacy`. */
export async function openSocket(address, { legacy = false, path = '/v1/graphql' } = {}) {
  const wire = await Wire.connect(address);
  await wire.upgrade(path, legacy ? '' : 'Sec-WebSocket-Protocol: graphql-transport-ws\r\n');
  return wire;
}
