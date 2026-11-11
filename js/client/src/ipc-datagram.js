// The reliable datagram session (the "Reliable datagram session" section of
// docs/protocol/ipc-transports-v1.md; the reference is
// rust/src/protocol/ipc/datagram.rs), and the `udp` kind over it.

import { randomBytes } from 'node:crypto';
import dgram from 'node:dgram';
import dns from 'node:dns/promises';
import net from 'node:net';
import { Duplex } from 'node:stream';
import { brokenPipe, hungUp, IpcError, outgoingClosed, timedOut } from './ipc-endpoint.js';
import { trace } from './ipc-trace.js';

export const FRAME_HEADER = 8;
export const DATA = 1;
export const ACK = 2;
export const EOF = 3;
export const PING = 4;
export const ABORT = 5;
export const HELLO = 6;
export const WELCOME = 7;

/** At most this many received bytes wait to be read. */
const WINDOW = 1 << 20;
const MAX_RTO = 1000;
const TICK = 100;
/** How long hanging up waits for the end-of-stream frame to be acknowledged. */
const LINGER = 1000;
const HALF = 0x7fffffff;

/** Builds one frame. */
export function frame(type, seq, payload = Buffer.alloc(0)) {
  const bytes = Buffer.alloc(FRAME_HEADER + payload.length);
  bytes[0] = type;
  bytes.writeUInt32LE(seq >>> 0, 4);
  Buffer.from(payload.buffer, payload.byteOffset, payload.length).copy(bytes, FRAME_HEADER);
  return bytes;
}

/** Splits a frame into `{ type, seq, payload }`, or `null` when too short. */
export function parseFrame(bytes) {
  if (bytes.length < FRAME_HEADER) return null;
  return { type: bytes[0], seq: bytes.readUInt32LE(4), payload: bytes.subarray(FRAME_HEADER) };
}

/** The chunk both sides use: the smaller proposal, within 16..65000. */
export function agreeChunk(own, peer) {
  const chunk = peer === undefined ? own : Math.min(own, peer);
  return Math.min(Math.max(chunk, 16), 65000);
}

const wrapping = (a, b) => (a - b) >>> 0;

/**
 * A reliable byte stream over a link that moves whole frames, possibly losing
 * some. The link is `{ send(frame), close(), peerAlive?() }`; incoming frames
 * are handed to `receive(frame)`.
 *
 * Settings: `chunk`, `rto` (ms), and for `udp` `heartbeat` and `dead` (ms).
 */
export class DatagramSession extends Duplex {
  #link;
  #settings;
  #received = 0;
  #ended = false;
  #aborted = false;
  #sent = 0;
  #acknowledged = 0;
  #writeClosed = false;
  #closed = false;
  #stopped = false;
  #lastHeard = Date.now();
  #lastSent = Date.now();
  #changed = new Set();
  #timer;

  constructor(kind, link, settings) {
    super({ allowHalfOpen: true });
    this.kind = kind;
    this.#link = link;
    this.#settings = settings;
    const { heartbeat } = settings;
    const tick = heartbeat ? Math.max(Math.min(TICK, heartbeat / 2), 1) : TICK;
    this.#timer = setInterval(() => this.#housekeeping(), tick);
    trace(`${kind} session started, chunk ${settings.chunk}`);
  }

  get chunk() {
    return this.#settings.chunk;
  }

  #notify() {
    for (const wake of [...this.#changed]) wake();
  }

  #waitChange(ms) {
    return new Promise((resolve) => {
      const wake = () => {
        clearTimeout(timer);
        this.#changed.delete(wake);
        resolve();
      };
      const timer = setTimeout(wake, Math.max(ms, 0));
      this.#changed.add(wake);
    });
  }

  #send(bytes) {
    this.#lastSent = Date.now();
    try {
      this.#link.send(bytes);
    } catch (error) {
      trace(`${this.kind} send failed: ${error.message}`);
    }
  }

  /** Hands one incoming frame to the session. */
  receive(bytes) {
    const parsed = parseFrame(bytes);
    if (parsed === null || this.#stopped) return;
    const { type, seq, payload } = parsed;
    this.#lastHeard = Date.now();
    let reply = null;
    if (type === DATA || type === EOF) {
      if (
        seq === (this.#received + 1) >>> 0 &&
        !this.#ended &&
        this.readableLength <= WINDOW &&
        payload.length <= this.#settings.chunk
      ) {
        this.#received = seq;
        if (type === EOF) {
          this.#ended = true;
          this.push(null);
        } else if (payload.length > 0) {
          this.push(Buffer.from(payload));
        }
        reply = this.#received;
      } else if (wrapping(seq, this.#received) > HALF || seq === this.#received) {
        // A retransmission of something already accepted.
        reply = this.#received;
      }
    } else if (type === ACK) {
      if (wrapping(seq, this.#acknowledged) <= HALF) this.#acknowledged = seq;
    } else if (type === ABORT) {
      if (!this.#aborted) trace(`${this.kind} peer hung up`);
      this.#hangUp();
    }
    this.#notify();
    if (reply !== null) this.#send(frame(ACK, reply));
  }

  #hangUp() {
    this.#aborted = true;
    this.#notify();
    if (!this.#ended && !this.#closed) this.destroy(hungUp());
  }

  #housekeeping() {
    if (this.#aborted || this.#stopped) return;
    const now = Date.now();
    const { heartbeat, dead } = this.#settings;
    const silent = dead !== undefined && now - this.#lastHeard >= dead;
    const peerAlive = this.#link.peerAlive?.() ?? true;
    if (silent || !peerAlive) {
      trace(`${this.kind} peer is gone`);
      this.#hangUp();
    } else if (heartbeat !== undefined && now - this.#lastSent >= heartbeat) {
      this.#send(frame(PING, 0));
    }
  }

  /** Sends a DATA or EOF frame and waits for its acknowledgement. */
  async #sendReliably(type, payload, linger) {
    this.#sent = (this.#sent + 1) >>> 0;
    const seq = this.#sent;
    const bytes = frame(type, seq, payload);
    let rto = Math.max(this.#settings.rto, 1);
    for (;;) {
      this.#send(bytes);
      const deadline = Date.now() + rto;
      for (;;) {
        if (wrapping(this.#acknowledged, seq) <= HALF) return;
        if (this.#aborted) throw brokenPipe();
        if (this.#closed && linger === undefined) {
          throw new IpcError('the connection was closed', 'EPIPE');
        }
        const now = Date.now();
        if (linger !== undefined && now >= linger) throw timedOut();
        if (now >= deadline) break;
        await this.#waitChange(Math.min(deadline, linger ?? deadline) - now);
      }
      rto = Math.min(rto * 2, MAX_RTO);
    }
  }

  _read() {}

  _write(chunk, _encoding, callback) {
    this.#writeAll(chunk).then(() => callback(), callback);
  }

  async #writeAll(chunk) {
    const size = this.#settings.chunk;
    for (let offset = 0; offset < chunk.length; offset += size) {
      if (this.#writeClosed || this.#closed) throw outgoingClosed();
      if (this.#aborted) throw brokenPipe();
      await this.#sendReliably(DATA, chunk.subarray(offset, offset + size));
    }
  }

  _final(callback) {
    if (this.#writeClosed || this.#aborted || this.#closed) {
      callback();
      return;
    }
    this.#writeClosed = true;
    this.#sendReliably(EOF, Buffer.alloc(0)).then(() => callback(), callback);
  }

  _destroy(error, callback) {
    if (this.#closed) {
      callback(error);
      return;
    }
    this.#closed = true;
    this.#notify();
    trace(`${this.kind} session hanging up`);
    const end = !this.#writeClosed && !this.#aborted;
    this.#writeClosed = true;
    const ending = end
      ? this.#sendReliably(EOF, Buffer.alloc(0), Date.now() + LINGER).catch(() => {})
      : Promise.resolve();
    ending.then(() => {
      if (!this.#aborted) for (let index = 0; index < 3; index += 1) this.#send(frame(ABORT, 0));
      this.#stopped = true;
      clearInterval(this.#timer);
      try {
        this.#link.close();
      } catch {
        // Already closed.
      }
      trace(`${this.kind} session stopped`);
      callback(error);
    });
  }
}

const isIgnorable = (error) =>
  ['ECONNREFUSED', 'ECONNRESET', 'EAGAIN', 'EWOULDBLOCK', 'ETIMEDOUT', 'EINTR'].includes(
    error?.code,
  );

/**
 * Splits `host:port` or `[v6]:port` and resolves it to every address, in the
 * resolver's order, as `{ host, port, family }` entries.
 */
export async function resolveAddresses(address) {
  const colon = address.lastIndexOf(':');
  const port = colon < 0 ? NaN : Number(address.slice(colon + 1));
  if (colon < 0 || !/^[0-9]+$/.test(address.slice(colon + 1)) || port > 65535) {
    throw new IpcError(`invalid socket address \`${address}\``, 'EINVAL');
  }
  const host = address.slice(0, colon).replace(/^\[(.*)\]$/, '$1');
  if (net.isIP(host)) return [{ host, port, family: net.isIP(host) }];
  let found;
  try {
    found = await dns.lookup(host, { all: true });
  } catch (cause) {
    throw new IpcError(`\`${address}\` resolves to no address`, 'EINVAL', { cause });
  }
  if (found.length === 0) throw new IpcError(`\`${address}\` resolves to no address`, 'EINVAL');
  return found.map((entry) => ({ host: entry.address, port, family: entry.family }));
}

/** The display of a socket address, as Rust prints it. */
export const display = (host, port) => (net.isIPv6(host) ? `[${host}]:${port}` : `${host}:${port}`);

/** Connects to a `udp:` listener: HELLO, WELCOME, then a session on the WELCOME's source. */
export async function connectUdp(endpoint, { signal } = {}) {
  const [server] = await resolveAddresses(endpoint.address);
  const serverText = display(server.host, server.port);
  const socket = dgram.createSocket(server.family === 6 ? 'udp6' : 'udp4');
  const ownChunk = endpoint.number('chunk', 1200, 16, 65000);
  const rto = endpoint.millis('rto', 20);
  const heartbeat = Math.max(endpoint.millis('heartbeat', 1000), 1);
  const dead = Math.max(endpoint.millis('dead', 10_000), 1);
  const token = randomBytes(16);
  const payload = Buffer.alloc(20);
  token.copy(payload);
  payload.writeUInt32LE(ownChunk, 16);
  const hello = frame(HELLO, 0, payload);
  const timeout = endpoint.timeout();
  let session = null;
  let early = [];
  try {
    await new Promise((done, fail) => {
      socket.once('error', fail);
      socket.bind(0, () => {
        socket.off('error', fail);
        done();
      });
    });
    const welcome = await new Promise((done, fail) => {
      let timer;
      let resend;
      const finish = (error, value) => {
        clearTimeout(timer);
        clearInterval(resend);
        socket.off('message', onMessage);
        socket.off('error', onError);
        signal?.removeEventListener('abort', onAbort);
        if (error) fail(error);
        else done(value);
      };
      const onMessage = (bytes, source) => {
        const parsed = parseFrame(bytes);
        if (parsed?.type !== WELCOME || parsed.payload.length !== 20) return;
        if (!parsed.payload.subarray(0, 16).equals(token)) return;
        finish(null, { source, chunk: parsed.payload.readUInt32LE(16) });
      };
      const onError = (error) => {
        if (!isIgnorable(error)) finish(error);
      };
      const onAbort = () => finish(signal.reason);
      const sendHello = () =>
        socket.send(hello, server.port, server.host, (error) => {
          if (error && !isIgnorable(error)) finish(error);
        });
      socket.on('message', onMessage);
      socket.on('error', onError);
      signal?.addEventListener('abort', onAbort, { once: true });
      timer = setTimeout(
        () => finish(new IpcError(`no \`udp\` listener answered at ${serverText}`, 'ETIMEDOUT')),
        timeout,
      );
      resend = setInterval(sendHello, 100);
      sendHello();
    });
    const { source } = welcome;
    // Frames that arrive before the session exists are kept for it.
    const collect = (bytes, from) => {
      if (from.address === source.address && from.port === source.port) early.push(bytes);
    };
    socket.on('message', collect);
    await new Promise((done, fail) => {
      socket.once('error', fail);
      socket.connect(source.port, source.address, () => {
        socket.off('error', fail);
        done();
      });
    });
    socket.off('message', collect);
    const chunk = agreeChunk(ownChunk, welcome.chunk);
    trace(
      `udp connected to ${display(source.address, source.port)} through ${serverText}, chunk ${chunk}`,
    );
    let closed = false;
    session = new DatagramSession(
      'udp',
      {
        send: (bytes) => {
          if (!closed) socket.send(bytes, () => {});
        },
        close: () => {
          closed = true;
          socket.close();
        },
      },
      { chunk, rto, heartbeat, dead },
    );
    socket.on('message', (bytes) => session.receive(bytes));
    socket.on('error', (error) => {
      if (!isIgnorable(error)) trace(`udp receive failed: ${error.message}`);
    });
    for (const bytes of early) session.receive(bytes);
    early = [];
    return session;
  } catch (error) {
    if (session === null) {
      try {
        socket.close();
      } catch {
        // Not bound.
      }
    }
    throw error;
  }
}
