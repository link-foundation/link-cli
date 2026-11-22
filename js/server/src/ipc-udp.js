// `udp:HOST:PORT`, the reliable datagram sessions of the "UDP" section of
// docs/protocol/ipc-transports-v1.md: a port of the `udp` part of
// rust/src/protocol/ipc/net.rs (`UdpLink`, `connect_udp`, `listen_udp`) over
// Node.js `dgram`, adapted from the `udp` connector of
// js/client/src/ipc-datagram.js.
//
// The client sends HELLO (a token and its chunk) every 100 ms until a WELCOME
// with the same token arrives from the session socket the listener bound for
// it; both then run a `Session` (ipc-datagram.js) over a connected socket.
// Rust's listener polls its socket with a 100 ms read timeout; here datagrams
// queue up as `dgram` delivers them.
import * as dgram from 'node:dgram';
import { IoError, ioErrorKind } from './protocol-errors.js';
import { trace } from './ipc-trace.js';
import { IpcStream } from './ipc-stream.js';
import { agreeChunk, frame, HELLO, parseFrame, Session, WELCOME } from './ipc-datagram.js';
import { display, lookupAll, resolve, resolveAll } from './ipc-net.js';
import { randomToken, Signal } from './ipc-util.js';

const IGNORABLE = new Set([
  'ConnectionRefused',
  'ConnectionReset',
  'WouldBlock',
  'TimedOut',
  'Interrupted',
]);

/** `ignorable`: errors a datagram exchange shrugs off. */
export const ignorable = (error) => IGNORABLE.has(ioErrorKind(error));

const WELCOME_KEPT = 30_000;

/** The `Datagrams` of a connected UDP socket: frames go to the session. */
export class UdpLink {
  #socket;

  constructor(socket) {
    this.#socket = socket;
    this.onFrame = () => {};
    this.onError = () => {};
    socket.on('message', (bytes) => this.onFrame(bytes));
    socket.on('error', (error) => {
      if (!ignorable(error)) this.onError(error);
    });
  }

  send(bytes) {
    return new Promise((resolve, reject) => {
      this.#socket.send(bytes, (error) => {
        if (!error) resolve(true);
        else if (ignorable(error)) resolve(false);
        else reject(error);
      });
    });
  }

  close() {
    try {
      this.#socket.close();
    } catch {
      // Already closed.
    }
  }
}

/** `udp_settings`. */
export const udpSettings = (endpoint, chunk) => ({
  chunk,
  rto: endpoint.millis('rto', 20),
  heartbeat: Math.max(endpoint.millis('heartbeat', 1000), 1),
  dead: Math.max(endpoint.millis('dead', 10_000), 1),
});

/** `hello_payload`: the 16-byte token and the chunk (u32 LE). */
export function helloPayload(token, chunk) {
  const payload = Buffer.alloc(20);
  token.copy(payload);
  payload.writeUInt32LE(chunk, 16);
  return payload;
}

/** `token_and_chunk`: `{token, chunk}` of exactly 20 bytes, else `null`. */
export const tokenAndChunk = (payload) =>
  payload.length === 20
    ? { token: Buffer.from(payload.subarray(0, 16)), chunk: payload.readUInt32LE(16) }
    : null;

const socketType = (family) => (family === 6 || family === 'IPv6' ? 'udp6' : 'udp4');

/** Binds a `dgram` socket of `family` to `address:port`. */
export function bindUdp(family, address, port, api = dgram) {
  return new Promise((resolve, reject) => {
    const socket = api.createSocket({ type: socketType(family), ipv6Only: false });
    const failed = (error) => {
      socket.close();
      reject(error);
    };
    socket.once('error', failed);
    socket.bind({ address, port }, () => {
      socket.off('error', failed);
      resolve(socket);
    });
  });
}

/** `socket.connect(source)`. */
const connectUdpSocket = (socket, port, address) =>
  new Promise((resolve, reject) => {
    socket.connect(port, address, (error) => (error ? reject(error) : resolve()));
  });

/** Queues the datagrams of an unconnected socket for a receiving loop. */
class Inbox {
  #items = [];
  #signal = new Signal();

  constructor(socket) {
    socket.on('message', (bytes, source) => this.#push({ bytes, source }));
    socket.on('error', (error) => this.#push({ error }));
  }

  #push(item) {
    this.#items.push(item);
    this.#signal.notify();
  }

  wake() {
    this.#signal.notify();
  }

  /** The next datagram, `null` after `ms`; errors other than ignorable throw. */
  async next(ms) {
    for (;;) {
      const item = this.#items.shift();
      if (item === undefined) {
        if (ms === 0) return null;
        await this.#signal.wait(ms);
        ms = 0;
      } else if (item.error === undefined) {
        return item;
      } else if (!ignorable(item.error)) {
        throw item.error;
      }
    }
  }
}

/** `connect_udp`; `lookup` and the `dgram` `api` are injectable for tests. */
export async function connectUdp(endpoint, lookup = lookupAll, api = dgram) {
  const [server] = await resolve(endpoint.address, lookup);
  const unspecified = server.family === 6 ? '::' : '0.0.0.0';
  const socket = await bindUdp(server.family, unspecified, 0, api);
  const serverText = display(server.address, server.port);
  try {
    const ownChunk = endpoint.number('chunk', 1200, 16, 65000);
    const token = randomToken();
    const hello = frame(HELLO, 0, helloPayload(token, ownChunk));
    const deadline = performance.now() + endpoint.timeout();
    const inbox = new Inbox(socket);
    let nextHello = performance.now();
    for (;;) {
      const now = performance.now();
      if (now >= deadline) {
        throw new IoError('TimedOut', `no \`udp\` listener answered at ${serverText}`);
      }
      if (now >= nextHello) {
        await sendTo(socket, hello, server);
        nextHello = now + 100;
      }
      const received = await inbox.next(Math.min(100, Math.max(deadline - now, 0)));
      const parsed = received === null ? null : parseFrame(received.bytes);
      if (parsed === null || parsed.kind !== WELCOME) continue;
      const welcome = tokenAndChunk(parsed.payload);
      if (welcome === null || !welcome.token.equals(token)) continue;
      const { address, port } = received.source;
      socket.removeAllListeners('message');
      socket.removeAllListeners('error');
      await connectUdpSocket(socket, port, address);
      const chunk = agreeChunk(ownChunk, welcome.chunk);
      trace(`udp connected to ${display(address, port)} through ${serverText}, chunk ${chunk}`);
      return startSession(endpoint, socket, chunk);
    }
  } catch (error) {
    socket.close();
    throw error;
  }
}

function sendTo(socket, bytes, { address, port }) {
  return new Promise((resolve, reject) => {
    socket.send(bytes, port, address, (error) => {
      if (error && !ignorable(error)) reject(error);
      else resolve();
    });
  });
}

function startSession(endpoint, socket, chunk) {
  const settings = udpSettings(endpoint, chunk);
  return new IpcStream(Session.start('udp', new UdpLink(socket), settings));
}

/** `UdpIpcListener`: hands out one session per HELLO token. */
export class UdpListener {
  #socket;
  #inbox;
  #api;
  #sessions = new Map();
  #closed = false;

  /** How long a session's WELCOME is resent to a repeated HELLO, in milliseconds. */
  welcomeKept = WELCOME_KEPT;

  constructor(endpoint, socket, api = dgram) {
    this.endpointOption = endpoint;
    this.#socket = socket;
    this.#api = api;
    this.#inbox = new Inbox(socket);
    this.address = socket.address();
    this.ownChunk = endpoint.number('chunk', 1200, 16, 65000);
  }

  async accept() {
    for (;;) {
      if (this.#closed) return null;
      const received = await this.#inbox.next(100);
      if (this.#closed) return null;
      if (received === null) continue;
      const now = performance.now();
      for (const [key, session] of this.#sessions) {
        if (now - session.created >= this.welcomeKept) this.#sessions.delete(key);
      }
      const parsed = parseFrame(received.bytes);
      if (parsed === null || parsed.kind !== HELLO) continue;
      const hello = tokenAndChunk(parsed.payload);
      if (hello === null) continue;
      const { address, port } = received.source;
      const key = `${display(address, port)} ${hello.token.toString('hex')}`;
      const known = this.#sessions.get(key);
      if (known !== undefined) {
        known.link.send(known.welcome).catch(() => {});
        continue;
      }
      const socket = await bindUdp(this.address.family, this.address.address, 0, this.#api);
      try {
        await connectUdpSocket(socket, port, address);
      } catch (error) {
        socket.close();
        throw error;
      }
      const chunk = agreeChunk(this.ownChunk, hello.chunk);
      const welcome = frame(WELCOME, 0, helloPayload(hello.token, chunk));
      const link = new UdpLink(socket);
      link.send(welcome).catch(() => {});
      this.#sessions.set(key, { link, welcome, created: now });
      const local = socket.address();
      trace(
        `udp accepted ${display(address, port)} on ${display(local.address, local.port)}, chunk ${chunk}`,
      );
      const settings = udpSettings(this.endpointOption, chunk);
      return new IpcStream(Session.start('udp', link, settings));
    }
  }

  endpoint() {
    return `udp:${display(this.address.address, this.address.port)}`;
  }

  endpoints() {
    return [this.endpoint()];
  }

  localAddr() {
    return null;
  }

  close() {
    if (this.#closed) return;
    this.#closed = true;
    this.#inbox.wake();
    this.#socket.close();
  }
}

/** `listen_udp`: binds the first resolved address, as `UdpSocket::bind` does. */
export async function listenUdp(endpoint, lookup = lookupAll, api = dgram) {
  let last;
  const addresses = await resolveAll(
    endpoint.address,
    'could not resolve to any addresses',
    lookup,
  );
  for (const { address, port, family } of addresses) {
    try {
      return new UdpListener(endpoint, await bindUdp(family, address, port, api), api);
    } catch (error) {
      last = error;
    }
  }
  throw last;
}
