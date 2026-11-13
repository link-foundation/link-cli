// One duplex stream over a separate readable and writable, as FIFOs and the
// standard input and output of a child process come.

import { Duplex } from 'node:stream';
import { hungUp } from './ipc-endpoint.js';

/**
 * Joins `readable` (incoming bytes) and `writable` (outgoing bytes). `end()`
 * ends the writable; the readable's end is this stream's end.
 *
 * - `greeting`: the first incoming byte is a greeting to discard; the
 *   readable ending before it is a hang-up.
 * - `peerAlive`: checked every 100 ms until the greeting arrives.
 * - `release(error)`: called once on destroy after both sides are destroyed;
 *   may return a promise the destroy waits for.
 */
export class JoinedStream extends Duplex {
  #readable;
  #writable;
  #greeted;
  #release;
  #watch = null;

  constructor(readable, writable, { greeting = false, peerAlive, release } = {}) {
    super({ allowHalfOpen: true });
    this.#readable = readable;
    this.#writable = writable;
    this.#greeted = !greeting;
    this.#release = release;
    readable.on('data', (chunk) => this.#receive(chunk));
    readable.on('end', () => {
      if (this.#greeted) this.push(null);
      else this.destroy(hungUp());
    });
    readable.on('error', (error) => this.destroy(error));
    writable.on('error', (error) => this.destroy(error));
    readable.pause();
    if (!this.#greeted && peerAlive) {
      this.#watch = setInterval(() => {
        if (!peerAlive()) this.destroy(hungUp());
      }, 100);
    }
  }

  #receive(chunk) {
    let bytes = chunk;
    if (!this.#greeted) {
      this.#greeted = true;
      clearInterval(this.#watch);
      bytes = bytes.subarray(1);
      if (bytes.length === 0) return;
    }
    if (!this.push(bytes)) this.#readable.pause();
  }

  _read() {
    this.#readable.resume();
  }

  _write(chunk, encoding, callback) {
    this.#writable.write(chunk, encoding, (error) => callback(error ?? undefined));
  }

  _final(callback) {
    const writable = this.#writable;
    if (writable.writableFinished || writable.destroyed) {
      callback();
      return;
    }
    const done = (error) => {
      writable.off('finish', onFinish);
      writable.off('error', done);
      callback(error);
    };
    const onFinish = () => done();
    writable.once('finish', onFinish);
    writable.once('error', done);
    writable.end();
  }

  _destroy(error, callback) {
    clearInterval(this.#watch);
    this.#readable.destroy();
    this.#writable.destroy();
    let released;
    try {
      released = this.#release?.(error);
    } catch {
      released = undefined;
    }
    Promise.resolve(released).then(
      () => callback(error),
      () => callback(error),
    );
  }
}
