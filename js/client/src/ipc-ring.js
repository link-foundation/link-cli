// The two byte rings of `shm`, `mmap`, `file` and `signal` (the "Ring region"
// section of docs/protocol/ipc-transports-v1.md; the reference is
// rust/src/protocol/ipc/ring.rs). Node.js has no shared mappings, so the
// region is accessed with positioned reads and writes of the file that backs
// it; the counter complements make that safe against concurrent stores.

import fs from 'node:fs';
import os from 'node:os';
import { Duplex } from 'node:stream';
import { brokenPipe, hungUp, IpcError, outgoingClosed } from './ipc-endpoint.js';
import { numberField, PeerWatch, Rendezvous } from './ipc-rendezvous.js';
import { trace } from './ipc-trace.js';

export const MAGIC = Buffer.from('LIPC', 'latin1');
export const VERSION = 1;
export const HEADER = 64;
export const RING_HEADER = 128;
const HEAD = 0;
const WRITER_STATE = 8;
const TAIL = 64;
const READER_STATE = 72;
const OPEN = 0;
const ENDED = 1;
const ABORTED = 2;
const GONE = 2;
const PEER_CHECK = 100;
const READ_CHUNK = 65536;

export const regionSize = (capacity) => HEADER + 2 * (RING_HEADER + capacity);
export const ringBase = (ring, capacity) => HEADER + ring * (RING_HEADER + capacity);

const immediate = () => new Promise((resolve) => setImmediate(resolve));
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** A region of bytes both processes see: any object with `read` and `write` at offsets. */
export class FileRegion {
  constructor(fd, length) {
    this.fd = fd;
    this.length = length;
  }

  read(offset, length) {
    const buffer = Buffer.alloc(length);
    let done = 0;
    while (done < length) {
      const count = fs.readSync(this.fd, buffer, done, length - done, offset + done);
      if (count === 0) throw new IpcError('the ring region is too small', 'EINVAL');
      done += count;
    }
    return buffer;
  }

  write(offset, bytes) {
    let done = 0;
    while (done < bytes.length) {
      done += fs.writeSync(this.fd, bytes, done, bytes.length - done, offset + done);
    }
  }

  close() {
    fs.closeSync(this.fd);
  }
}

/** An in-memory region, for tests of the ring itself. */
export class MemoryRegion {
  constructor(length) {
    this.bytes = Buffer.alloc(length);
    this.length = length;
  }

  read(offset, length) {
    return Buffer.from(this.bytes.subarray(offset, offset + length));
  }

  write(offset, bytes) {
    bytes.copy(this.bytes, offset);
  }

  close() {}
}

const load = (region, offset) => region.read(offset, 4).readUInt32LE(0);

const store = (region, offset, value) => {
  const bytes = Buffer.alloc(4);
  bytes.writeUInt32LE(value >>> 0, 0);
  region.write(offset, bytes);
};

/** Writes the header of a fresh, zero-filled region. */
export function initialize(region, capacity) {
  region.write(0, MAGIC);
  store(region, 4, VERSION);
  store(region, 8, capacity);
  for (const ring of [0, 1]) {
    const base = ringBase(ring, capacity);
    store(region, base + HEAD + 4, 0xffffffff);
    store(region, base + TAIL + 4, 0xffffffff);
  }
}

/** Checks the header and returns the capacity. */
export function validate(region) {
  const fail = (message) => new IpcError(message, 'EINVAL');
  if (region.length < HEADER) throw fail('the ring region is too small');
  if (!region.read(0, 4).equals(MAGIC)) throw fail('the ring region lacks the `LIPC` magic');
  if (load(region, 4) !== VERSION) throw fail('the ring region has another version');
  const capacity = load(region, 8);
  if (capacity < 64 || capacity > 1 << 30 || regionSize(capacity) > region.length) {
    throw fail('the ring region has an invalid capacity');
  }
  return capacity;
}

/** A counter whose complement matches, read again while torn. */
function counter(region, offset) {
  for (let attempt = 0; attempt <= 1_000_000; attempt += 1) {
    const bytes = region.read(offset, 8);
    const value = bytes.readUInt32LE(0);
    if (value === ~bytes.readUInt32LE(4) >>> 0) return value;
  }
  throw new IpcError('a ring counter stays torn', 'EINVAL');
}

/** Stores a counter: the value first, the complement second. */
function publish(region, offset, value) {
  store(region, offset, value);
  store(region, offset + 4, ~value >>> 0);
}

/** `[head, tail]` of a ring, consistent with each other. */
function positions(region, base, capacity) {
  for (;;) {
    const head = counter(region, base + HEAD);
    const tail = counter(region, base + TAIL);
    if ((head - tail) >>> 0 <= capacity) return [head, tail];
  }
}

/** Polling with a backoff from immediate re-checks to sleeps of at most `poll` ms. */
export class PollNotifier {
  constructor(poll) {
    this.poll = poll;
  }

  generation() {
    return 0;
  }

  notify() {}

  wait(_generation, attempt, limit) {
    if (attempt < 128) return immediate();
    const step = 0.05 * 2 ** Math.min(attempt - 128, 10);
    return sleep(Math.min(step, this.poll, limit));
  }
}

// ---------------------------------------------------------------- signals

let signalGeneration = 0;
const signalWaiters = new Set();
const installedSignals = new Set();

function onSignal() {
  signalGeneration += 1;
  for (const wake of [...signalWaiters]) wake();
}

const signalName = (signo) =>
  Object.entries(os.constants.signals).find(([, number]) => number === signo)?.[0];

/** Installs the process-wide handler of `signo` (kept for the life of the process). */
export function installSignal(signo) {
  if (installedSignals.has(signo)) return;
  const name = signalName(signo);
  if (name === undefined || name === 'SIGKILL' || name === 'SIGSTOP') {
    throw new IpcError(`signal ${signo} cannot be caught`, 'EINVAL');
  }
  // Node writes a diagnostic report on its report signal (SIGUSR2, the default
  // `signo`) when `--report-on-signal` is on: the ring needs that signal now.
  const report = process.report;
  if (report?.reportOnSignal && report.signal === name) {
    report.reportOnSignal = false;
    trace(`diagnostic reports on ${name} turned off: the signal ring uses ${name}`);
  }
  try {
    process.on(name, onSignal);
  } catch (cause) {
    throw new IpcError(`signal ${signo} cannot be caught`, 'EINVAL', { cause });
  }
  trace(`signal handler installed for signal ${signo}`);
  installedSignals.add(signo);
}

/** Wakes the peer with the signal it asked for and waits for its signal. */
export class SignalNotifier {
  constructor(peer, peerSignal) {
    this.peer = peer;
    this.peerSignal = peerSignal;
  }

  generation() {
    return signalGeneration;
  }

  notify() {
    try {
      process.kill(this.peer, this.peerSignal);
    } catch {
      // The peer is gone; the liveness check notices.
    }
  }

  wait(generation, attempt, limit) {
    if (attempt < 16) return immediate();
    if (signalGeneration !== generation) return Promise.resolve();
    return new Promise((resolve) => {
      const wake = () => {
        clearTimeout(timer);
        signalWaiters.delete(wake);
        resolve();
      };
      const timer = setTimeout(wake, limit);
      signalWaiters.add(wake);
    });
  }
}

/** The signal number an ack or request asks for. */
export function peerSignal(fields) {
  const signo = numberField(fields, 'signal');
  if (signo === undefined) {
    throw new IpcError('the handshake lacks the number `signal`', 'EINVAL');
  }
  if (signo < 1 || signo > 64) {
    throw new IpcError(`the peer asked for the invalid signal ${signo}`, 'EINVAL');
  }
  return signo;
}

// ---------------------------------------------------------------- the stream

/**
 * One side of a ring connection as a duplex stream. `end()` stores the end of
 * stream; `destroy()` hangs up (writer state 2, reader state 2).
 */
export class RingStream extends Duplex {
  #region;
  #capacity;
  #outgoing;
  #incoming;
  #notifier;
  #peer;
  #cleanup;
  #released = false;
  #wanted = false;
  #looping = false;

  constructor({ kind, region, client = true, notifier, peerPid, cleanup = [] }) {
    super({ allowHalfOpen: true });
    this.kind = kind;
    this.#region = region;
    this.#capacity = validate(region);
    const [outgoing, incoming] = client ? [0, 1] : [1, 0];
    this.#outgoing = ringBase(outgoing, this.#capacity);
    this.#incoming = ringBase(incoming, this.#capacity);
    this.#notifier = notifier;
    this.#peer = new PeerWatch(peerPid);
    this.#cleanup = cleanup;
    this.peerPid = peerPid;
  }

  get capacity() {
    return this.#capacity;
  }

  _read() {
    this.#wanted = true;
    if (this.#looping) return;
    this.#looping = true;
    this.#readLoop()
      .catch((error) => this.destroy(error))
      .finally(() => {
        this.#looping = false;
      });
  }

  async #readLoop() {
    const region = this.#region;
    const base = this.#incoming;
    const capacity = this.#capacity;
    let attempt = 0;
    let lastCheck = Date.now();
    while (this.#wanted && !this.#released) {
      const generation = this.#notifier.generation();
      const [head, tail] = positions(region, base, capacity);
      const used = (head - tail) >>> 0;
      if (used > 0) {
        const count = Math.min(used, READ_CHUNK);
        const start = tail % capacity;
        const first = Math.min(count, capacity - start);
        const data = base + RING_HEADER;
        let bytes = region.read(data + start, first);
        if (first < count) bytes = Buffer.concat([bytes, region.read(data, count - first)]);
        publish(region, base + TAIL, (tail + count) >>> 0);
        this.#notifier.notify();
        attempt = 0;
        if (!this.push(bytes)) this.#wanted = false;
        continue;
      }
      const state = load(region, base + WRITER_STATE);
      if (state === ABORTED) throw hungUp();
      if (state !== OPEN) {
        // The writer may have published its last bytes after the positions
        // above were read: the end counts only once the ring is still empty.
        const [lastHead, lastTail] = positions(region, base, capacity);
        if (lastHead !== lastTail) continue;
        this.#wanted = false;
        this.push(null);
        return;
      }
      const now = Date.now();
      if (now - lastCheck >= PEER_CHECK) {
        lastCheck = now;
        if (!this.#peer.alive()) throw hungUp();
      }
      await this.#notifier.wait(generation, attempt, PEER_CHECK);
      attempt += 1;
    }
  }

  _write(chunk, _encoding, callback) {
    this.#writeAll(chunk).then(() => callback(), callback);
  }

  async #writeAll(chunk) {
    const region = this.#region;
    const base = this.#outgoing;
    const capacity = this.#capacity;
    let offset = 0;
    let attempt = 0;
    let lastCheck = Date.now();
    while (offset < chunk.length) {
      if (this.#released || load(region, base + WRITER_STATE) !== OPEN) throw outgoingClosed();
      if (load(region, base + READER_STATE) === GONE) throw brokenPipe();
      const generation = this.#notifier.generation();
      const [head, tail] = positions(region, base, capacity);
      const free = capacity - ((head - tail) >>> 0);
      if (free > 0) {
        const count = Math.min(free, chunk.length - offset);
        const start = head % capacity;
        const first = Math.min(count, capacity - start);
        const data = base + RING_HEADER;
        region.write(data + start, chunk.subarray(offset, offset + first));
        if (first < count) region.write(data, chunk.subarray(offset + first, offset + count));
        publish(region, base + HEAD, (head + count) >>> 0);
        this.#notifier.notify();
        offset += count;
        attempt = 0;
        continue;
      }
      const now = Date.now();
      if (now - lastCheck >= PEER_CHECK) {
        lastCheck = now;
        if (!this.#peer.alive()) throw brokenPipe();
      }
      await this.#notifier.wait(generation, attempt, PEER_CHECK);
      attempt += 1;
    }
  }

  _final(callback) {
    try {
      if (!this.#released && load(this.#region, this.#outgoing + WRITER_STATE) === OPEN) {
        store(this.#region, this.#outgoing + WRITER_STATE, ENDED);
        this.#notifier.notify();
      }
      callback();
    } catch (error) {
      callback(error);
    }
  }

  _destroy(error, callback) {
    if (!this.#released) {
      this.#released = true;
      trace(`${this.kind} ring to process ${this.peerPid} hung up`);
      try {
        if (load(this.#region, this.#outgoing + WRITER_STATE) === OPEN) {
          store(this.#region, this.#outgoing + WRITER_STATE, ENDED);
        }
        store(this.#region, this.#incoming + READER_STATE, GONE);
        this.#notifier.notify();
      } catch {
        // The region is unusable; the peer's liveness check notices.
      }
      try {
        this.#region.close();
      } catch {
        // Already closed.
      }
      for (const file of this.#cleanup) fs.rmSync(file, { force: true });
    }
    callback(error);
  }
}

const SUFFIX = { mmap: 'mmap', file: 'ring', signal: 'sig' };

function createFile(file, size) {
  const { O_RDWR, O_CREAT, O_EXCL } = fs.constants;
  const fd = fs.openSync(file, O_RDWR | O_CREAT | O_EXCL, 0o600);
  try {
    fs.ftruncateSync(fd, size);
  } catch (error) {
    fs.closeSync(fd);
    fs.rmSync(file, { force: true });
    throw error;
  }
  return fd;
}

/**
 * Connects to a `shm`, `mmap`, `file` or `signal` listener; `platform`
 * (default: this one) decides when the files are removed.
 */
export async function connectRing(endpoint, { signal } = {}, platform = process.platform) {
  const kind = endpoint.kind;
  const rendezvous = Rendezvous.begin(endpoint, kind);
  const capacity = endpoint.number('capacity', 65536, 64, 1 << 30);
  const size = regionSize(capacity);
  // `shm` is the POSIX object `/lcID`, which Linux keeps in /dev/shm.
  const file = kind === 'shm' ? `/dev/shm/lc${rendezvous.id}` : rendezvous.path(SUFFIX[kind]);
  const region = new FileRegion(createFile(file, size), size);
  const forget = () => fs.rmSync(file, { force: true });
  try {
    initialize(region, capacity);
    const poll = endpoint.millis('poll', 5);
    let fields = {};
    if (kind === 'signal') {
      const own = endpoint.number('signo', os.constants.signals.SIGUSR2 ?? 12, 1, 64);
      installSignal(own);
      fields = { signal: own };
    }
    const ack = await rendezvous.request(fields, signal);
    const peer = rendezvous.peerOf(ack);
    const notifier =
      kind === 'signal' ? new SignalNotifier(peer, peerSignal(ack)) : new PollNotifier(poll);
    // Both sides hold the region open now; nothing should outlive a crash.
    const windows = platform === 'win32';
    const cleanup = windows ? [file] : [];
    if (!windows) forget();
    const stream = new RingStream({ kind, region, notifier, peerPid: peer, cleanup });
    trace(`${kind} connected as ${rendezvous.id} to process ${peer}, ${capacity} bytes per ring`);
    return stream;
  } catch (error) {
    region.close();
    forget();
    throw error;
  }
}
