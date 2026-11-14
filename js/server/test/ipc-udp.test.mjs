// Unit tests of ipc-udp.js, the `udp` part of rust/src/protocol/ipc/net.rs:
// the HELLO/WELCOME handshake over real sockets, and over a `dgram` stand-in
// the datagrams a handshake skips (garbage, other frames, other tokens),
// ignorable and fatal errors, resent and expired WELCOMEs, and bind fallbacks.
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { describe, it } from 'node:test';
import { DATA, frame, HELLO, parseFrame, WELCOME } from '../src/ipc-datagram.js';
import { Endpoint } from '../src/ipc-endpoint.js';
import { setTrace } from '../src/ipc-trace.js';
import {
  bindUdp,
  connectUdp,
  helloPayload,
  ignorable,
  listenUdp,
  tokenAndChunk,
  UdpLink,
  udpSettings,
} from '../src/ipc-udp.js';
import { sleep } from '../src/ipc-util.js';
import { diagnostics, IoError } from '../src/protocol-errors.js';

const failure = (code) => Object.assign(new Error(code), { code });

/** Collects the `[ipc]` traces of `body`. */
async function traced(body) {
  const lines = [];
  const original = diagnostics.stderr;
  diagnostics.stderr = (line) => lines.push(line);
  setTrace(true);
  try {
    await body();
  } finally {
    setTrace(false);
    diagnostics.stderr = original;
  }
  return lines;
}

/** A `dgram.Socket` stand-in driven by its `FakeDgram`. */
class FakeUdp extends EventEmitter {
  closed = false;
  sent = [];
  local = null;

  constructor(api, type) {
    super();
    this.api = api;
    this.type = type;
  }

  bind({ address, port }, callback) {
    const error = this.api.bindError(address);
    if (error !== null) {
      setImmediate(() => this.emit('error', error));
      return;
    }
    const family = this.type === 'udp6' ? 'IPv6' : 'IPv4';
    this.local = { address, port: port || this.api.nextPort++, family };
    setImmediate(callback);
  }

  address() {
    return this.local;
  }

  connect(port, address, callback) {
    this.peer = { address, port };
    setImmediate(() => callback(this.api.connectError));
  }

  send(bytes, ...rest) {
    const callback = rest.pop();
    this.sent.push(Buffer.from(bytes));
    const error = this.api.onSend(this, Buffer.from(bytes), ...rest);
    setImmediate(() => callback(error));
  }

  close() {
    if (this.closed) throw failure('ERR_SOCKET_DGRAM_NOT_RUNNING');
    this.closed = true;
  }
}

/** A `dgram` stand-in: every socket it creates, scripted binds, connects and sends. */
class FakeDgram {
  sockets = [];
  nextPort = 40000;
  connectError = null;
  refused = new Set();
  bindError = (address) => (this.refused.has(address) ? failure('EADDRNOTAVAIL') : null);
  onSend = () => null;

  createSocket({ type }) {
    const socket = new FakeUdp(this, type);
    this.sockets.push(socket);
    return socket;
  }
}

const later = (socket, event, ...args) => setImmediate(() => socket.emit(event, ...args));

describe('UDP handshake pieces', () => {
  it('encode HELLO payloads and settings as Rust does', () => {
    const token = Buffer.alloc(16, 7);
    const payload = helloPayload(token, 1200);
    assert.equal(payload.length, 20);
    assert.deepEqual(tokenAndChunk(payload), { token, chunk: 1200 });
    assert.equal(tokenAndChunk(Buffer.alloc(19)), null);
    assert.deepEqual(udpSettings(Endpoint.parse('udp:h:1?heartbeat=0&dead=0&rto=5'), 64), {
      chunk: 64,
      rto: 5,
      heartbeat: 1,
      dead: 1,
    });
    assert.deepEqual(udpSettings(Endpoint.parse('udp:h:1'), 1200), {
      chunk: 1200,
      rto: 20,
      heartbeat: 1000,
      dead: 10_000,
    });
    assert.equal(ignorable(failure('ECONNREFUSED')), true);
    assert.equal(ignorable(new IoError('TimedOut', 'late')), true);
    assert.equal(ignorable(failure('EACCES')), false);
  });

  it('link frames, errors and sends of a connected socket', async () => {
    const api = new FakeDgram();
    const socket = api.createSocket({ type: 'udp4' });
    const link = new UdpLink(socket);
    socket.emit('message', Buffer.from('ignored before a session listens'));
    socket.emit('error', failure('EACCES'));
    const frames = [];
    const errors = [];
    link.onFrame = (bytes) => frames.push(bytes.toString());
    link.onError = (error) => errors.push(error.code);
    socket.emit('message', Buffer.from('frame'));
    socket.emit('error', failure('ECONNREFUSED'));
    socket.emit('error', failure('EACCES'));
    assert.deepEqual([frames, errors], [['frame'], ['EACCES']]);
    assert.equal(await link.send(Buffer.from('a')), true);
    api.onSend = () => failure('ECONNREFUSED');
    assert.equal(await link.send(Buffer.from('b')), false);
    api.onSend = () => failure('EACCES');
    await assert.rejects(link.send(Buffer.from('c')), { code: 'EACCES' });
    link.close();
    link.close();
    assert.equal(socket.closed, true);
  });

  it('closes a socket whose bind fails', async () => {
    const api = new FakeDgram();
    api.refused.add('192.0.2.1');
    await assert.rejects(bindUdp(6, '192.0.2.1', 0, api), { code: 'EADDRNOTAVAIL' });
    assert.deepEqual(
      api.sockets.map((socket) => [socket.type, socket.closed]),
      [['udp6', true]],
    );
    const socket = await bindUdp('IPv4', '127.0.0.1', 0, api);
    assert.equal(socket.type, 'udp4');
  });
});

describe('UDP over real sockets', { timeout: 30_000 }, () => {
  it('connect and talk both ways', async () => {
    const listener = await listenUdp(Endpoint.parse('udp:127.0.0.1:0?heartbeat=50'));
    const [endpoint] = listener.endpoints();
    assert.match(endpoint, /^udp:127\.0\.0\.1:[0-9]+$/);
    const [client, served] = await Promise.all([
      connectUdp(Endpoint.parse(`${endpoint}?chunk=64`)),
      listener.accept(),
    ]);
    await client.write(Buffer.from('ping'));
    assert.equal((await served.read(4)).toString(), 'ping');
    await served.write(Buffer.from('pong'));
    assert.equal((await client.read(4)).toString(), 'pong');
    client.shutdown();
    served.shutdown();
    listener.close();
    await Promise.all([client.transport.stopped, served.transport.stopped]);
  });
});

/** Answers every HELLO of the client with `replies(token)`, from port 5000. */
function answering(api, replies) {
  api.onSend = (socket, bytes) => {
    const parsed = parseFrame(bytes);
    if (parsed.kind !== HELLO) return null;
    const token = parsed.payload.subarray(0, 16);
    const source = { address: '::1', port: 5000 };
    for (const reply of replies(token)) {
      if (reply instanceof Error) later(socket, 'error', reply);
      else later(socket, 'message', reply, source);
    }
    return null;
  };
}

describe('UDP connect', { timeout: 30_000 }, () => {
  it('skips what is no WELCOME of its own token', async () => {
    const api = new FakeDgram();
    answering(api, (token) => [
      failure('ECONNRESET'),
      Buffer.alloc(3),
      frame(DATA, 1, Buffer.from('x')),
      frame(WELCOME, 0, Buffer.alloc(5)),
      frame(WELCOME, 0, helloPayload(Buffer.alloc(16), 64)),
      frame(WELCOME, 0, helloPayload(token, 64)),
    ]);
    let stream;
    const lines = await traced(async () => {
      stream = await connectUdp(Endpoint.parse('udp:[::1]:9?chunk=100'), undefined, api);
    });
    const [socket] = api.sockets;
    assert.deepEqual([socket.type, socket.local.address, socket.peer.port], ['udp6', '::', 5000]);
    assert.deepEqual(lines, [
      '[ipc] udp connected to [::1]:5000 through [::1]:9, chunk 64',
      '[ipc] udp session started, chunk 64',
    ]);
    assert.equal(stream.transport.chunk, 64);
    stream.shutdown();
  });

  it('resends HELLO until the timeout', async () => {
    const api = new FakeDgram();
    let hellos = 0;
    api.onSend = () => {
      hellos += 1;
      return failure('ECONNREFUSED');
    };
    await assert.rejects(
      connectUdp(Endpoint.parse('udp:127.0.0.1:9?timeout=250'), undefined, api),
      {
        kind: 'TimedOut',
        message: 'no `udp` listener answered at 127.0.0.1:9',
      },
    );
    assert.ok(hellos >= 2, `${hellos} HELLOs`);
    assert.equal(api.sockets[0].closed, true);
  });

  it('fails on errors that are not ignorable', async () => {
    const endpoint = Endpoint.parse('udp:127.0.0.1:9?timeout=2000');
    const sending = new FakeDgram();
    sending.onSend = () => failure('EACCES');
    await assert.rejects(connectUdp(endpoint, undefined, sending), { code: 'EACCES' });
    assert.equal(sending.sockets[0].closed, true);

    const receiving = new FakeDgram();
    answering(receiving, () => [failure('EACCES')]);
    await assert.rejects(connectUdp(endpoint, undefined, receiving), { code: 'EACCES' });

    const connecting = new FakeDgram();
    connecting.connectError = failure('EACCES');
    answering(connecting, (token) => [frame(WELCOME, 0, helloPayload(token, 64))]);
    await assert.rejects(connectUdp(endpoint, undefined, connecting), { code: 'EACCES' });
    assert.equal(connecting.sockets[0].closed, true);
  });
});

describe('UDP listen', { timeout: 30_000 }, () => {
  it('binds the first address that accepts the bind', async () => {
    const api = new FakeDgram();
    api.refused.add('192.0.2.1');
    const lookup = async () => [
      { address: '192.0.2.1', family: 4 },
      { address: '127.0.0.1', family: 4 },
    ];
    const listener = await listenUdp(Endpoint.parse('udp:here:7'), lookup, api);
    assert.deepEqual(listener.endpoints(), ['udp:127.0.0.1:7']);
    assert.equal(listener.localAddr(), null);
    listener.close();
    api.refused.add('127.0.0.1');
    await assert.rejects(listenUdp(Endpoint.parse('udp:here:7'), lookup, api), {
      code: 'EADDRNOTAVAIL',
    });
  });

  it('hands out one session per HELLO token', async () => {
    const api = new FakeDgram();
    const listener = await listenUdp(Endpoint.parse('udp:127.0.0.1:0?chunk=64'), undefined, api);
    const [server] = api.sockets;
    const source = { address: '127.0.0.1', port: 5555 };
    const token = Buffer.alloc(16, 1);
    const hello = frame(HELLO, 0, helloPayload(token, 1000));
    const accepting = listener.accept();
    // A quiet poll first, then what is no HELLO.
    await sleep(150);
    server.emit('error', failure('ECONNREFUSED'));
    server.emit('message', Buffer.alloc(2), source);
    server.emit('message', frame(DATA, 1, Buffer.from('x')), source);
    server.emit('message', frame(HELLO, 0, Buffer.alloc(3)), source);
    let first;
    const lines = await traced(async () => {
      server.emit('message', hello, source);
      first = await accepting;
    });
    assert.deepEqual(lines, [
      '[ipc] udp accepted 127.0.0.1:5555 on 127.0.0.1:40001, chunk 64',
      '[ipc] udp session started, chunk 64',
    ]);
    const session = api.sockets[1];
    assert.deepEqual(session.peer, { address: '127.0.0.1', port: 5555 });
    await sleep(10);
    const welcome = parseFrame(session.sent[0]);
    assert.equal(welcome.kind, WELCOME);
    assert.deepEqual(tokenAndChunk(welcome.payload), { token, chunk: 64 });

    // A repeated HELLO gets the WELCOME again; an expired one a new session.
    // A WELCOME that fails to go out is resent with the next HELLO.
    api.onSend = () => failure('EACCES');
    const again = listener.accept();
    server.emit('message', hello, source);
    await sleep(150);
    assert.ok(session.sent.filter((bytes) => parseFrame(bytes).kind === WELCOME).length >= 2);
    listener.welcomeKept = 0;
    server.emit('message', hello, source);
    const second = await again;
    assert.equal(api.sockets.length, 3);

    api.onSend = () => null;
    api.connectError = failure('EACCES');
    const refusing = listener.accept();
    server.emit('message', frame(HELLO, 0, helloPayload(Buffer.alloc(16, 2), 64)), source);
    await assert.rejects(refusing, { code: 'EACCES' });
    assert.equal(api.sockets[3].closed, true);

    server.emit('error', failure('EACCES'));
    await assert.rejects(listener.accept(), { code: 'EACCES' });

    const waiting = listener.accept();
    listener.close();
    listener.close();
    assert.equal(await waiting, null);
    assert.equal(await listener.accept(), null);
    assert.equal(server.closed, true);
    first.shutdown();
    second.shutdown();
  });
});
