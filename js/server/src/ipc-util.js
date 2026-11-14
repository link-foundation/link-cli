// The shared helpers of rust/src/protocol/ipc/mod.rs: the custom errors
// (`hung_up`, `broken_pipe`, `timed_out`, `unsupported`), `random_id`,
// `random_token`, `process_alive` (rust/src/protocol/ipc/unix/mod.rs) and
// `PeerWatch`, plus the small asynchronous primitives the JavaScript port
// needs in place of Rust's threads, condition variables and sleeps: a `Signal`
// that wakes waiters, a byte queue and `sleep`. Adapted from the error helpers
// of js/client/src/ipc-endpoint.js.
import { randomBytes } from 'node:crypto';
import { constants } from 'node:os';
import { IoError } from './protocol-errors.js';

/** `hung_up()`: the peer went away while we read. */
export const hungUp = () => new IoError('ConnectionReset', 'the peer hung up');

/** `broken_pipe()`: the peer no longer reads what we write. */
export const brokenPipe = () => new IoError('BrokenPipe', 'the peer no longer reads');

/** `timed_out()`: a read waited longer than its timeout. */
export const timedOut = () => new IoError('TimedOut', 'timed out');

/** The write side of a connection was closed by this end. */
export const outgoingClosed = () => new IoError('BrokenPipe', 'the outgoing direction was closed');

/** `unsupported(kind, what)`: Rust's text for a kind this platform lacks. */
export const unsupported = (kind, what) =>
  new IoError('Unsupported', `${what} on \`${kind}:\` endpoints is not supported on this platform`);

/**
 * A kind Rust supports on this platform but Node.js cannot reach, because it
 * has no API for the operating-system primitive behind it.
 */
export const noNodeApi = (kind, primitive) =>
  new IoError(
    'Unsupported',
    `\`${kind}:\` endpoints are not supported by the JavaScript server: Node.js has no API for ${primitive}`,
  );

/**
 * A Node.js-style system error (`code`, negative `errno`, `syscall`), so
 * error-report.js prints it as Rust prints the `io::Error` of that errno.
 */
export function systemError(code, syscall, errnos = constants.errno) {
  const error = new Error(`${syscall} ${code}`);
  error.code = code;
  error.errno = -errnos[code];
  error.syscall = syscall;
  return error;
}

/** Resolves after `ms` milliseconds. */
export const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * Wakes everybody waiting: the condition variable of the Rust transports.
 * `wait(ms)` resolves on the next `notify()` or after `ms` (never when null).
 */
export class Signal {
  #waiters = new Set();

  wait(ms = null) {
    return new Promise((resolve) => {
      let timer;
      const wake = () => {
        clearTimeout(timer);
        this.#waiters.delete(wake);
        resolve();
      };
      this.#waiters.add(wake);
      if (ms !== null) timer = setTimeout(wake, Math.max(0, ms));
    });
  }

  notify() {
    for (const wake of [...this.#waiters]) wake();
  }
}

/** A queue of received bytes. */
export class ByteQueue {
  #chunks = [];
  length = 0;

  push(chunk) {
    if (chunk.length === 0) return;
    this.#chunks.push(chunk);
    this.length += chunk.length;
  }

  /** Takes at most `max` bytes from the front. */
  take(max) {
    const parts = [];
    let count = 0;
    while (count < max && this.#chunks.length > 0) {
      const chunk = this.#chunks[0];
      const part = chunk.subarray(0, max - count);
      parts.push(part);
      count += part.length;
      if (part.length === chunk.length) this.#chunks.shift();
      else this.#chunks[0] = chunk.subarray(part.length);
    }
    this.length -= count;
    return parts.length === 1 ? parts[0] : Buffer.concat(parts, count);
  }

  clear() {
    this.#chunks = [];
    this.length = 0;
  }
}

/** `random_id()`: 20 lowercase hexadecimal digits naming one connection. */
export const randomId = () => randomBytes(10).toString('hex');

/** `random_token()`: 16 random bytes. */
export const randomToken = () => randomBytes(16);

/**
 * `process_alive(pid)` of rust/src/protocol/ipc/unix/mod.rs: `kill(pid, 0)`
 * succeeds or fails with EPERM. Node.js's `process.kill(pid, 0)` does the
 * same on Windows through `OpenProcess`, like windows/mod.rs.
 */
export function processAlive(pid, kill = process.kill) {
  if (!Number.isInteger(pid) || pid <= 0 || pid > 0x7fffffff) return false;
  try {
    kill(pid, 0);
    return true;
  } catch (error) {
    return error.code === 'EPERM';
  }
}

/** `PeerWatch`: whether a peer process lives, checked at most every 100 ms. */
export class PeerWatch {
  #checked = performance.now();
  #alive = true;

  constructor(pid, alive = processAlive) {
    this.pid = pid;
    this.isAlive = alive;
  }

  alive() {
    if (this.#alive && performance.now() - this.#checked >= 100) {
      this.#alive = this.isAlive(this.pid);
      this.#checked = performance.now();
    }
    return this.#alive;
  }
}
