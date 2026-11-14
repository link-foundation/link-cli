// The reliable byte session over datagrams of `udp` (the "Datagram session"
// section of docs/protocol/ipc-transports-v1.md): a port of
// rust/src/protocol/ipc/datagram.rs, adapted from
// js/client/src/ipc-datagram.js.
//
// Rust runs a receiving thread that waits on the link; here the link calls
// `onFrame(bytes)` for every datagram and `onError(error)` when receiving
// failed, and a timer runs the housekeeping every tick. A link is an object
// with `send(frame)` (a promise of whether the datagram left), `close()` and
// an optional `peerAlive()`. Rust's `Drop` of a session hangs up; JavaScript
// has no destructors, so `shutdown()` does, and `stopped` settles once the
// session lingered for the end of the stream and stopped receiving.
import { IoError } from './protocol-errors.js';
import { trace } from './ipc-trace.js';
import { brokenPipe, ByteQueue, hungUp, outgoingClosed, Signal, timedOut } from './ipc-util.js';

export const HEADER = 8;
export const DATA = 1;
export const ACK = 2;
export const EOF = 3;
export const PING = 4;
export const ABORT = 5;
export const HELLO = 6;
export const WELCOME = 7;

const WINDOW = 1 << 20;
const MAX_RTO = 1000;
const TICK = 100;
const LINGER = 1000;
const HALF = 0x7fffffff;
const EMPTY = Buffer.alloc(0);

/** `frame(kind, seq, payload)`: `[kind, 0, 0, 0, seq (u32 LE), payload]`. */
export function frame(kind, seq, payload = EMPTY) {
  const bytes = Buffer.alloc(HEADER + payload.length);
  bytes[0] = kind;
  bytes.writeUInt32LE(seq >>> 0, 4);
  Buffer.from(payload.buffer, payload.byteOffset, payload.byteLength).copy(bytes, HEADER);
  return bytes;
}

/** `parse(frame)`: `{kind, seq, payload}`, or `null` below eight bytes. */
export function parseFrame(bytes) {
  if (bytes.length < HEADER) return null;
  return { kind: bytes[0], seq: bytes.readUInt32LE(4), payload: bytes.subarray(HEADER) };
}

/** `agree_chunk(own, peer)`: the smaller chunk, within 16..=65000. */
export const agreeChunk = (own, peer = null) =>
  Math.min(Math.max(peer === null ? own : Math.min(own, peer), 16), 65000);

const now = () => performance.now();
const ack = (seq) => frame(ACK, seq);

/**
 * `Session`: a reliable, ordered byte stream over a datagram link, with a
 * stop-and-wait sender, cumulative acknowledgements, heartbeats and an end
 * of stream. `settings`: `{chunk, rto, heartbeat, dead}` in milliseconds;
 * `heartbeat` and `dead` may be `null`.
 */
export class Session {
  #link;
  #settings;
  #changed = new Signal();
  #inbound = new ByteQueue();
  #received = 0;
  #ended = false;
  #aborted = false;
  #sent = 0;
  #acknowledged = 0;
  #writeClosed = false;
  #closed = false;
  #stopping = false;
  #lastHeard = now();
  #lastSent = now();
  #sending = Promise.resolve();
  #timer;

  constructor(kind, link, settings) {
    this.kind = kind;
    this.#link = link;
    this.#settings = settings;
    /** Settles once a shut-down session stopped receiving. */
    this.stopped = new Promise((resolve) => {
      this.resolveStopped = resolve;
    });
    const tick =
      settings.heartbeat === null ? TICK : Math.max(Math.min(TICK, settings.heartbeat / 2), 1);
    this.#timer = setInterval(() => this.#housekeeping(now()), tick);
    this.#timer.unref?.();
    link.onFrame = (bytes) => this.#handle(bytes);
    link.onError = (error) => {
      trace(`${kind} receive failed: ${error.message}`);
      this.#hangUp();
    };
  }

  /** `Session::start`. */
  static start(kind, link, settings) {
    const session = new Session(kind, link, settings);
    trace(`${kind} session started, chunk ${settings.chunk}`);
    return session;
  }

  get chunk() {
    return this.#settings.chunk;
  }

  async #send(bytes) {
    try {
      return await this.#link.send(bytes);
    } finally {
      this.#lastSent = now();
    }
  }

  #handle(bytes) {
    if (this.#stopping) return;
    const parsed = parseFrame(bytes);
    if (parsed === null) return;
    const { kind, seq, payload } = parsed;
    this.#lastHeard = now();
    let reply = null;
    let deliver = false;
    if (kind === DATA || kind === EOF) {
      if (
        seq === (this.#received + 1) >>> 0 &&
        !this.#ended &&
        this.#inbound.length <= WINDOW &&
        payload.length <= this.#settings.chunk
      ) {
        this.#received = seq;
        deliver = true;
        reply = seq;
      } else if ((seq - this.#received) >>> 0 > HALF || seq === this.#received) {
        reply = this.#received;
      }
    } else if (kind === ACK) {
      if ((seq - this.#acknowledged) >>> 0 <= HALF) {
        this.#acknowledged = seq;
        this.#changed.notify();
      }
    } else if (kind === ABORT) {
      if (!this.#aborted) trace(`${this.kind} peer hung up`);
      this.#aborted = true;
      this.#changed.notify();
    }
    // The acknowledgement leaves before the bytes are delivered, as in Rust.
    if (reply !== null) this.#send(ack(reply)).catch(() => {});
    if (deliver) {
      if (kind === EOF) this.#ended = true;
      else this.#inbound.push(Buffer.from(payload));
      this.#changed.notify();
    }
  }

  #housekeeping(time) {
    if (this.#aborted || this.#stopping) return;
    const { heartbeat, dead } = this.#settings;
    const ping = heartbeat !== null && time - this.#lastSent >= heartbeat;
    const silent = dead !== null && time - this.#lastHeard >= dead;
    if (silent || !(this.#link.peerAlive?.() ?? true)) {
      trace(`${this.kind} peer is gone`);
      this.#hangUp();
    } else if (ping) {
      this.#send(frame(PING, 0)).catch(() => {});
    }
  }

  #hangUp() {
    this.#aborted = true;
    this.#changed.notify();
  }

  async #sendReliably(kind, payload, linger) {
    this.#sent = (this.#sent + 1) >>> 0;
    const seq = this.#sent;
    const bytes = frame(kind, seq, payload);
    let rto = Math.max(this.#settings.rto, 1);
    for (;;) {
      await this.#send(bytes);
      const deadline = now() + rto;
      for (;;) {
        if ((this.#acknowledged - seq) >>> 0 <= HALF) return;
        if (this.#aborted) {
          if (kind === EOF) {
            trace(`${this.kind} peer hung up before acknowledging the end`);
            return;
          }
          throw brokenPipe();
        }
        if (this.#closed && linger === null) {
          throw new IoError('BrokenPipe', 'the connection was closed');
        }
        const time = now();
        if (linger !== null && time >= linger) throw timedOut();
        if (time >= deadline) break;
        await this.#changed.wait(deadline - time);
      }
      rto = Math.min(rto * 2, MAX_RTO);
    }
  }

  /** Runs `job` once the earlier sends finished (the `sending` lock). */
  #exclusive(job) {
    const result = this.#sending.then(job);
    this.#sending = result.then(
      () => undefined,
      () => undefined,
    );
    return result;
  }

  async read(max, timeoutMs) {
    const deadline = timeoutMs === null ? null : now() + timeoutMs;
    for (;;) {
      if (this.#inbound.length > 0) return this.#inbound.take(max);
      if (this.#ended || this.#closed) return EMPTY;
      if (this.#aborted) throw hungUp();
      const remaining = deadline === null ? null : deadline - now();
      if (remaining !== null && remaining <= 0) throw timedOut();
      await this.#changed.wait(remaining);
    }
  }

  write(buffer) {
    return this.#exclusive(async () => {
      if (this.#writeClosed || this.#closed) throw outgoingClosed();
      if (this.#aborted) throw brokenPipe();
      const count = Math.min(buffer.length, this.#settings.chunk);
      await this.#sendReliably(DATA, buffer.subarray(0, count), null);
      return count;
    });
  }

  closeWrite() {
    return this.#exclusive(async () => {
      if (this.#writeClosed || this.#aborted || this.#closed) return;
      this.#writeClosed = true;
      await this.#sendReliably(EOF, EMPTY, null);
    });
  }

  shutdown() {
    if (this.#closed) return;
    this.#closed = true;
    this.#changed.notify();
    trace(`${this.kind} session hanging up`);
    this.#exclusive(async () => {
      const end = !this.#writeClosed && !this.#aborted;
      this.#writeClosed = true;
      if (end) await this.#sendReliably(EOF, EMPTY, now() + LINGER).catch(() => {});
      if (!this.#aborted) {
        for (let count = 0; count < 3; count += 1)
          await this.#send(frame(ABORT, 0)).catch(() => {});
      }
      this.#stopping = true;
      this.#changed.notify();
      clearInterval(this.#timer);
      this.#link.close();
      trace(`${this.kind} session stopped`);
      this.resolveStopped();
    });
  }
}
