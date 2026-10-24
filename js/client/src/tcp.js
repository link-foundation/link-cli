// The links protocol over TCP for Node.js, Bun and Deno (all three provide
// `node:net`). Browsers have no raw sockets: use the WebAssembly backend, or a
// server-side proxy, there.

import net from 'node:net';
import { tcpServerCapabilities } from './capabilities.js';
import { LinksClient } from './client.js';
import { scopeError, scopeOf, watchScope } from './deadline.js';
import { ConnectionClosedError, LimitExceededError, LinksError, MalformedMessageError, ProtocolMismatchError } from './errors.js';
import { DEFAULT_MAX_MESSAGE_BYTES, encodeMessage, MessageReader } from './framing.js';
import { parseDocument } from './lino.js';
import { errorMessage } from './operations.js';

/** Parses `host:port`, `[v6]:port` or `{ host, port }`. */
export function parseAddress(address) {
  if (typeof address === 'object' && address !== null) return { host: address.host ?? '127.0.0.1', port: Number(address.port) };
  const text = String(address);
  const colon = text.lastIndexOf(':');
  if (colon === -1) throw new TypeError(`expected host:port, got "${text}"`);
  const host = text.slice(0, colon).replace(/^\[(.*)\]$/, '$1');
  const port = Number(text.slice(colon + 1));
  if (!Number.isInteger(port) || port < 0 || port > 65535) throw new TypeError(`invalid port in "${text}"`);
  return { host, port };
}

/**
 * One connection to a `clink --serve` server. Requests are written in order
 * and replies matched first-in first-out, so several may be in flight
 * (pipelining); the server still executes them one at a time.
 *
 * - Requests above `maxRequestBytes` are refused before sending; a reply above
 *   `maxReplyBytes` breaks the connection.
 * - Socket backpressure: when the kernel buffer is full, writes wait for
 *   `drain`, and at most `maxInFlight` requests are written ahead of their replies.
 * - Aborting a request that is still queued drops it (`not-sent`). Aborting
 *   one the server may be running leaves no way to skip its reply (the
 *   protocol has no request ids), so the connection is closed and every
 *   in-flight request ends `unknown`.
 */
export class TcpTransport {
  #socket;
  #owned;
  #reader;
  #queue = [];
  #inFlight = [];
  #blocked = false;
  #closedError = null;
  #closed;
  #idleWaiters = [];

  constructor(socket, { owned = true, maxRequestBytes = DEFAULT_MAX_MESSAGE_BYTES, maxReplyBytes = DEFAULT_MAX_MESSAGE_BYTES, maxInFlight = 64, capabilities } = {}) {
    this.#socket = socket;
    this.#owned = owned;
    this.maxRequestBytes = maxRequestBytes;
    this.maxInFlight = maxInFlight;
    this.#reader = new MessageReader({ maxBytes: maxReplyBytes });
    this.capabilities = tcpServerCapabilities({ ...capabilities, limits: { maxRequestBytes, maxReplyBytes, maxInFlight } });
    this.stats = { requests: 0, bytesSent: 0, bytesReceived: 0 };
    this.#closed = new Promise((resolve) => socket.once('close', resolve));
    this.onData = (chunk) => this.#receive(chunk);
    this.onDrain = () => {
      this.#blocked = false;
      this.#pump();
    };
    this.onClose = () => this.#fail(new ConnectionClosedError('the server closed the connection'));
    this.onError = (error) => this.#fail(new ConnectionClosedError(`connection error: ${error.message}`, { cause: error }));
    socket.on('data', this.onData);
    socket.on('drain', this.onDrain);
    socket.on('close', this.onClose);
    socket.on('error', this.onError);
  }

  /** Connects to `address` (`host:port`), honouring `signal` and `deadline`. */
  static async connect(address, options = {}) {
    const { host, port } = parseAddress(address);
    const scope = scopeOf(options);
    const early = scopeError(scope, 'not-sent');
    if (early) throw early;
    const socket = net.connect({ host, port });
    await new Promise((resolve, reject) => {
      const stop = watchScope(scope, (errorOf) => {
        socket.destroy();
        reject(errorOf('not-sent'));
      });
      socket.once('connect', () => {
        stop();
        resolve();
      });
      socket.once('error', (error) => {
        stop();
        reject(new ConnectionClosedError(`cannot connect to ${host}:${port}: ${error.message}`, { outcome: 'not-sent', cause: error }));
      });
    });
    socket.setNoDelay(true);
    return new TcpTransport(socket, { ...options, owned: true });
  }

  /** Sends one request (LiNo text) and resolves to the reply text. */
  send(text, scope = {}) {
    if (this.#closedError) return Promise.reject(new ConnectionClosedError(this.#closedError.message, { outcome: 'not-sent' }));
    const bytes = encodeMessage(text);
    // The limit applies to the message as the server counts it: its text.
    if (bytes.length - 2 > this.maxRequestBytes) {
      return Promise.reject(new LimitExceededError(`request of ${bytes.length - 2} bytes exceeds maxRequestBytes ${this.maxRequestBytes}`, { outcome: 'not-sent' }));
    }
    const early = scopeError(scope, 'not-sent');
    if (early) return Promise.reject(early);
    return new Promise((resolve, reject) => {
      const entry = { bytes, resolve, reject, sent: false };
      // The deadline can expire synchronously while watchScope registers.
      // Queue first so cancellation removes this entry, never its neighbor.
      this.#queue.push(entry);
      entry.stop = watchScope(scope, (errorOf) => {
        if (!entry.sent) {
          this.#queue.splice(this.#queue.indexOf(entry), 1);
          reject(errorOf('not-sent'));
        } else {
          // Its reply can no longer be told apart from the next one's.
          this.#fail(errorOf('unknown'), entry);
        }
      });
      if (!this.#closedError) this.#pump();
    });
  }

  #pump() {
    while (!this.#blocked && this.#queue.length > 0 && this.#inFlight.length < this.maxInFlight && !this.#closedError) {
      const entry = this.#queue.shift();
      entry.sent = true;
      this.#inFlight.push(entry);
      this.stats.requests += 1;
      this.stats.bytesSent += entry.bytes.length;
      if (!this.#socket.write(entry.bytes)) this.#blocked = true;
    }
  }

  #receive(chunk) {
    this.stats.bytesReceived += chunk.length;
    let messages;
    try {
      messages = this.#reader.push(chunk);
    } catch (error) {
      this.#fail(error instanceof LinksError ? error : new MalformedMessageError(error.message));
      return;
    }
    for (const message of messages) {
      const entry = this.#inFlight.shift();
      if (!entry) {
        this.#fail(new MalformedMessageError('the server sent a reply nobody asked for'));
        return;
      }
      entry.stop();
      entry.resolve(message);
    }
    this.#pump();
    this.#notifyIdle();
  }

  /** Ends the connection: every request in flight ends `unknown`, the queued ones `not-sent`. */
  #fail(error, culprit) {
    if (this.#closedError) return;
    this.#closedError = error;
    for (const entry of this.#inFlight) {
      entry.stop();
      entry.reject(entry === culprit || this.#inFlight.length === 1 ? error : new ConnectionClosedError(`the connection broke: ${error.message}`, { outcome: 'unknown', cause: error }));
    }
    for (const entry of this.#queue) {
      entry.stop();
      entry.reject(new ConnectionClosedError(`the connection broke: ${error.message}`, { outcome: 'not-sent', cause: error }));
    }
    this.#inFlight = [];
    this.#queue = [];
    this.#detach();
    // A socket handed in unowned stays its owner's to close.
    if (this.#owned) this.#socket.destroy();
    this.#notifyIdle();
  }

  #detach() {
    this.#socket.off('data', this.onData);
    this.#socket.off('drain', this.onDrain);
    this.#socket.off('close', this.onClose);
    this.#socket.off('error', this.onError);
  }

  #notifyIdle() {
    if (this.#queue.length === 0 && this.#inFlight.length === 0) {
      for (const resolve of this.#idleWaiters.splice(0)) resolve();
    }
  }

  get closed() {
    return this.#closedError !== null;
  }

  /**
   * Waits for the requests already made, then closes the connection if this
   * transport opened it. A socket handed in with `owned: false` is left open
   * (its listeners removed) for its owner to close.
   */
  async close() {
    if (this.#queue.length > 0 || this.#inFlight.length > 0) {
      await new Promise((resolve) => this.#idleWaiters.push(resolve));
    }
    if (this.#closedError) {
      if (this.#owned) await this.#closed;
      return;
    }
    this.#closedError = new ConnectionClosedError('the client closed the connection', { outcome: 'not-sent' });
    this.#detach();
    if (!this.#owned) return;
    this.#socket.end();
    const timer = setTimeout(() => this.#socket.destroy(), 1000);
    await this.#closed;
    clearTimeout(timer);
  }

  /** Closes the connection now, failing whatever is in flight. */
  destroy() {
    this.#fail(new ConnectionClosedError('the client destroyed the connection'));
  }
}

/**
 * Checks that the peer speaks the links protocol: `(count: (0))` must come
 * back as `(count: N)`. A peer of another protocol, or a link-cli server too
 * old to know LinksOperation documents, fails with ProtocolMismatchError.
 */
export async function verifyProtocol(transport, scope) {
  let reply;
  try {
    reply = await transport.send('(count: (0))', scope);
  } catch (error) {
    if (error instanceof MalformedMessageError) {
      throw new ProtocolMismatchError(`the peer does not frame links protocol messages: ${error.message}`, { cause: error });
    }
    throw error;
  }
  let document;
  try {
    document = parseDocument(reply);
  } catch (error) {
    throw new ProtocolMismatchError(`the peer's reply is not LiNo: ${error.message}`, { cause: error });
  }
  const message = errorMessage(document);
  const [link] = document;
  if (message !== null || document.length !== 1 || typeof link === 'string' || link.id !== 'count') {
    throw new ProtocolMismatchError(
      `the peer does not answer LinksOperation documents (links-text/1)${message === null ? `; it replied ${JSON.stringify(reply)}` : `: ${message}`}`,
    );
  }
}

/**
 * Connects a {@link LinksClient} to a `clink --serve` server.
 *
 * Options: `signal`, `deadline`, `timeout` (for connecting and the protocol
 * check), `verify` (default true), `maxRequestBytes`, `maxReplyBytes`,
 * `maxInFlight`, `capabilities` (overrides, e.g. for a server built with
 * wider addresses).
 */
export async function connectTcp(address, options = {}) {
  const transport = await TcpTransport.connect(address, options);
  if (options.verify !== false) {
    try {
      await verifyProtocol(transport, scopeOf(options));
    } catch (error) {
      transport.destroy();
      await transport.close();
      throw error;
    }
  }
  return new LinksClient(transport);
}
