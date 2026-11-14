// `IpcStream` and the `Transport` contract of rust/src/protocol/ipc/mod.rs,
// the `SocketTransport` of rust/src/protocol/ipc/net.rs and the buffering
// reader thread of rust/src/protocol/ipc/stdio.rs (`ThreadedReader`), here a
// queue fed by a Node.js `Readable`. Adapted from the duplex wrappers of
// js/client/src/ipc-joined.js and js/client/src/tcp.js.
//
// A transport is an object with `kind`, `read(max, timeoutMs)` (a Buffer of at
// least one byte, or an empty Buffer at the end), `write(buffer)` (the count
// written, at least one), an optional `flush()`, `closeWrite()` and a
// synchronous `shutdown()`. Rust runs a transport on several threads; here
// every call is a promise, and `IpcStream` lets one read and one write run at
// a time, as Rust's `reading` and `writing` locks do.
import { IoError, ioErrorKind } from './protocol-errors.js';
import { trace } from './ipc-trace.js';
import { brokenPipe, ByteQueue, Signal, systemError, timedOut } from './ipc-util.js';

const EMPTY = Buffer.alloc(0);

/** Runs `job` once every earlier job of `chain` settled; returns [result, chain]. */
function serialize(chain, job) {
  const result = chain.then(job);
  return [
    result,
    result.then(
      () => undefined,
      () => undefined,
    ),
  ];
}

/** A cloneable duplex byte stream over any transport (`IpcStream`). */
export class IpcStream {
  #transport;
  #readTimeout = null;
  #reading = Promise.resolve();
  #writing = Promise.resolve();

  constructor(transport) {
    this.#transport = transport;
  }

  /** The endpoint kind of the transport. */
  get kind() {
    return this.#transport.kind;
  }

  /** The transport. */
  get transport() {
    return this.#transport;
  }

  /** Makes reads fail with `TimedOut` after `ms` milliseconds; `null` waits forever. */
  setReadTimeout(ms) {
    if (ms !== null && !(Number.isFinite(ms) && ms >= 0)) {
      throw new IoError('InvalidInput', `a read timeout must be null or milliseconds, found ${ms}`);
    }
    this.#readTimeout = ms;
  }

  /** The read timeout in milliseconds, or `null`. */
  get readTimeout() {
    return this.#readTimeout;
  }

  /**
   * Reads at most `max` bytes: a Buffer of at least one byte, or an empty
   * Buffer at the end of the incoming direction. `Interrupted` is retried.
   */
  read(max = 65536) {
    if (max <= 0) return Promise.resolve(EMPTY);
    const timeout = this.#readTimeout;
    const [result, chain] = serialize(this.#reading, () =>
      retried(() => this.#transport.read(max, timeout)),
    );
    this.#reading = chain;
    return result;
  }

  /** Writes all of `buffer` (`write_all`). */
  write(buffer) {
    const bytes = Buffer.from(buffer.buffer, buffer.byteOffset, buffer.byteLength);
    const [result, chain] = serialize(this.#writing, async () => {
      let offset = 0;
      while (offset < bytes.length) {
        const count = await retried(() => this.#transport.write(bytes.subarray(offset)));
        if (count === 0) throw new IoError('WriteZero', 'failed to write whole buffer');
        offset += count;
      }
    });
    this.#writing = chain;
    return result;
  }

  /** Pushes buffered bytes to the peer. */
  async flush() {
    await this.#transport.flush?.();
  }

  /** Ends the outgoing direction; the peer reads the end of the stream. */
  async closeWrite() {
    await this.#transport.closeWrite();
  }

  /** Hangs up both directions; pending and later operations end. */
  shutdown() {
    this.#transport.shutdown();
  }
}

async function retried(operation) {
  for (;;) {
    try {
      return await operation();
    } catch (error) {
      if (ioErrorKind(error) !== 'Interrupted') throw error;
    }
  }
}

/**
 * Buffers what a Node.js `Readable` delivers, at most `limit` bytes before it
 * pauses the source. `keepError`: a failed source reports its error on every
 * later read, as `ThreadedReader` does; otherwise once, then the end.
 */
export class ReadQueue {
  #queue = new ByteQueue();
  #signal = new Signal();
  #ended = false;
  #error = null;
  #closed = false;
  #discarding = false;

  constructor(readable, { limit = 1 << 20, keepError = false } = {}) {
    this.readable = readable;
    this.limit = limit;
    this.keepError = keepError;
    readable.on('data', (chunk) => {
      if (this.#discarding) return;
      this.#queue.push(chunk);
      if (this.#queue.length >= limit) readable.pause();
      this.#signal.notify();
    });
    readable.on('end', () => this.#end());
    readable.on('close', () => this.#end());
    readable.on('error', (error) => {
      this.#error = error;
      this.#signal.notify();
    });
  }

  #end() {
    this.#ended = true;
    this.#signal.notify();
  }

  /**
   * At most `max` bytes, an empty Buffer at the end, or the source's error;
   * `TimedOut` after `timeoutMs` (null: never). `mapError` converts errors.
   */
  async read(max, timeoutMs, mapError = (error) => error) {
    const deadline = timeoutMs === null ? null : performance.now() + timeoutMs;
    for (;;) {
      if (this.#queue.length > 0) {
        const bytes = this.#queue.take(max);
        if (this.#queue.length < this.limit && !this.#closed) this.readable.resume();
        return bytes;
      }
      if (this.#closed) return EMPTY;
      if (this.#error !== null) {
        const error = this.#error;
        if (!this.keepError) {
          this.#error = null;
          this.#ended = true;
        }
        const mapped = mapError(error);
        if (mapped === null) return EMPTY;
        throw mapped;
      }
      if (this.#ended) return EMPTY;
      const remaining = deadline === null ? null : deadline - performance.now();
      if (remaining !== null && remaining <= 0) throw timedOut();
      await this.#signal.wait(remaining);
    }
  }

  /** Ends reading: later reads return the end at once. */
  close() {
    this.#closed = true;
    this.#signal.notify();
  }

  /** Drops what arrived and what arrives later, so the source never stalls. */
  discard() {
    this.#discarding = true;
    this.#queue.clear();
    this.close();
    this.readable.resume();
  }
}

// `SocketTransport::read`: `NotConnected` and `ConnectionAborted` are the
// end, `WouldBlock` and `TimedOut` the read timeout.
function socketReadError(error) {
  const kind = ioErrorKind(error);
  if (kind === 'NotConnected' || kind === 'ConnectionAborted') return null;
  if (kind === 'WouldBlock' || kind === 'TimedOut') return timedOut();
  return error;
}

// Errors that end a Windows named pipe (`ERROR_BROKEN_PIPE`, `ERROR_NO_DATA`,
// `ERROR_PIPE_NOT_CONNECTED`), as Node.js reports them.
const PIPE_ENDED = new Set(['EPIPE', 'ECONNRESET', 'ENOTCONN', 'EOF']);

/**
 * A connected `net.Socket`: TCP, a Unix domain socket or, with
 * `halfClose: false`, a Windows named pipe, which has no half-close: closing
 * the outgoing direction closes the pipe (`PipeTransport` of
 * rust/src/protocol/ipc/windows/mod.rs).
 */
export class SocketTransport {
  #socket;
  #queue;
  #halfClose;
  #closed = false;
  #pipeClosed = false;

  constructor(kind, socket, { halfClose = true } = {}) {
    this.kind = kind;
    this.#socket = socket;
    this.#halfClose = halfClose;
    this.#queue = new ReadQueue(socket);
    // Errors reach the reader through the queue; a socket error must not crash.
    socket.on('error', () => {});
  }

  get socket() {
    return this.#socket;
  }

  read(max, timeoutMs) {
    if (this.#pipeClosed) return Promise.resolve(EMPTY);
    return this.#queue.read(max, timeoutMs, (error) =>
      !this.#halfClose && PIPE_ENDED.has(error.code) ? null : socketReadError(error),
    );
  }

  write(buffer) {
    const socket = this.#socket;
    if (!this.#halfClose && (this.#pipeClosed || this.#closed)) {
      return Promise.reject(brokenPipe());
    }
    if (this.#closed || socket.destroyed || socket.writableEnded) {
      return Promise.reject(systemError('EPIPE', 'write'));
    }
    return new Promise((resolve, reject) => {
      socket.write(buffer, (error) => {
        if (!error) resolve(buffer.length);
        else if (!this.#halfClose && PIPE_ENDED.has(error.code)) reject(brokenPipe());
        else reject(typeof error.errno === 'number' ? error : systemError('EPIPE', 'write'));
      });
    });
  }

  async closeWrite() {
    if (this.#halfClose) {
      this.#socket.end();
      return;
    }
    this.#closePipe();
  }

  #closePipe() {
    if (this.#pipeClosed) return;
    this.#pipeClosed = true;
    trace('pipe closed');
    this.#queue.discard();
    this.#socket.destroySoon();
  }

  shutdown() {
    if (this.#closed) return;
    this.#closed = true;
    if (!this.#halfClose) {
      this.#closePipe();
      return;
    }
    this.#queue.discard();
    this.#socket.end();
    // Node.js closes the socket once both directions ended; a peer that never
    // ends its direction must not keep the descriptor forever.
    setTimeout(this.#socket.destroy.bind(this.#socket), 5000).unref();
  }
}
