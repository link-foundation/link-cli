// The two byte rings of `shm`, `mmap`, `file` and `signal` (the "Ring region"
// section of docs/protocol/ipc-transports-v1.md): a port of
// rust/src/protocol/ipc/ring.rs and of the POSIX signal notifier and shared
// memory names of rust/src/protocol/ipc/unix/mod.rs, adapted from
// js/client/src/ipc-ring.js.
//
// Node.js cannot map memory, so every kind reaches its region with positioned
// reads and writes of the file behind it (Rust's `FileRegion`): the page cache
// makes them coherent with the mappings of a Rust peer, and the complement of
// every counter makes them safe against torn stores. `shm` is the POSIX object
// `/lcID`, which Linux keeps as `/dev/shm/lcID`; elsewhere Node.js has no
// `shm_open`. `signal` wakes the peer with `process.kill` and is woken by a
// `process.on` handler; on Windows Rust uses named events, which Node.js lacks.
// The kinds Rust supports but this port cannot fail with `noNodeApi`.
//
// Rust's `Drop` releases the region; here `shutdown()` hangs up and closes the
// file at once, so a later read ends at once instead of draining the ring.
import * as fs from 'node:fs';
import { constants } from 'node:os';
import { IoError } from './protocol-errors.js';
import { trace } from './ipc-trace.js';
import { IpcStream } from './ipc-stream.js';
import { JsonObject } from './ipc-json.js';
import { acceptWith, Rendezvous, RendezvousListener } from './ipc-rendezvous.js';
import {
  brokenPipe,
  hungUp,
  noNodeApi,
  outgoingClosed,
  PeerWatch,
  processAlive,
  Signal,
  timedOut,
} from './ipc-util.js';

export const MAGIC = Buffer.from('LIPC', 'latin1');
export const VERSION = 1;
export const HEADER = 64;
export const RING_HEADER = 128;
const HEAD = 0;
const WRITER_STATE = 8;
const TAIL = 64;
const READER_STATE = 72;
export const OPEN = 0;
export const ENDED = 1;
export const ABORTED = 2;
export const GONE = 2;
const PEER_CHECK = 100;
const EMPTY = Buffer.alloc(0);

/** `region_size(capacity)`. */
export const regionSize = (capacity) => HEADER + 2 * (RING_HEADER + capacity);

/** `ring_base(ring, capacity)`. */
export const ringBase = (ring, capacity) => HEADER + ring * (RING_HEADER + capacity);

const immediate = () => new Promise((resolve) => setImmediate(resolve));

/** Little-endian u32 loads and stores over `readBytes` and `writeBytes`. */
class Words {
  load(offset) {
    return this.readBytes(offset, 4).readUInt32LE(0);
  }

  store(offset, value) {
    const bytes = Buffer.alloc(4);
    bytes.writeUInt32LE(value >>> 0, 0);
    this.writeBytes(offset, bytes);
  }
}

/** `FileRegion`: the bytes of an open file, reached with positioned I/O. */
export class FileRegion extends Words {
  constructor(fd, length, api = fs) {
    super();
    this.fd = fd;
    this.length = length;
    this.api = api;
  }

  readBytes(offset, length) {
    const buffer = Buffer.alloc(length);
    for (let done = 0; done < length;) {
      const count = this.api.readSync(this.fd, buffer, done, length - done, offset + done);
      if (count === 0) {
        throw new IoError('UnexpectedEof', 'the ring file is shorter than its header says');
      }
      done += count;
    }
    return buffer;
  }

  writeBytes(offset, bytes) {
    for (let done = 0; done < bytes.length;) {
      const count = this.api.writeSync(this.fd, bytes, done, bytes.length - done, offset + done);
      if (count === 0) throw new IoError('WriteZero', 'the ring file is full');
      done += count;
    }
  }

  close() {
    this.api.closeSync(this.fd);
  }
}

/** A region in this process's memory, for tests of the ring itself. */
export class MemoryRegion extends Words {
  constructor(length) {
    super();
    this.bytes = Buffer.alloc(length);
    this.length = length;
  }

  readBytes(offset, length) {
    return Buffer.from(this.bytes.subarray(offset, offset + length));
  }

  writeBytes(offset, bytes) {
    bytes.copy(this.bytes, offset);
  }

  close() {}
}

/** `validate`: checks the header and returns the capacity. */
export function validate(region) {
  const invalid = (message) => new IoError('InvalidData', message);
  if (region.length < HEADER) throw invalid('the ring region is too small');
  if (!region.readBytes(0, 4).equals(MAGIC)) {
    throw invalid('the ring region lacks the `LIPC` magic');
  }
  if (region.load(4) !== VERSION) throw invalid('the ring region has another version');
  const capacity = region.load(8);
  if (capacity < 64 || capacity > 1 << 30 || regionSize(capacity) > region.length) {
    throw invalid('the ring region has an invalid capacity');
  }
  return capacity;
}

/** `initialize`: the header of a fresh, zero-filled region. */
export function initialize(region, capacity) {
  region.writeBytes(0, MAGIC);
  region.store(4, VERSION);
  region.store(8, capacity);
  for (const ring of [0, 1]) {
    const base = ringBase(ring, capacity);
    region.store(base + HEAD + 4, 0xffffffff);
    region.store(base + TAIL + 4, 0xffffffff);
  }
}

/**
 * `PollNotifier`: immediate re-checks first (Rust spins and yields), then
 * sleeps from 50 µs doubling up to `poll` ms.
 */
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
    return new Promise((resolve) => setTimeout(resolve, Math.min(step, this.poll, limit)));
  }
}

// ---------------------------------------------------------------- signals

let generation = 0;
const caught = new Signal();
const installed = new Set();

function onSignal() {
  generation += 1;
  caught.notify();
}

const signalName = (signo) =>
  Object.keys(constants.signals).find((name) => constants.signals[name] === signo);

/**
 * `install(signo)`: catches `signo` for the rest of the process's life.
 * Node.js names no real-time signal, so those cannot be caught here.
 */
export function installSignal(signo, api = process) {
  if (installed.has(signo)) return;
  const name = signalName(signo);
  if (name === 'SIGKILL' || name === 'SIGSTOP') {
    throw new IoError('InvalidInput', `signal ${signo} cannot be caught`);
  }
  if (name === undefined) throw noNodeApi('signal', `catching signal ${signo}`);
  // With `--report-on-signal`, Node.js writes a diagnostic report on its
  // report signal (SIGUSR2 by default): the ring needs that signal now.
  const report = api.report;
  if (report?.reportOnSignal && report.signal === name) {
    report.reportOnSignal = false;
    trace(`diagnostic reports on ${name} turned off: the signal ring uses ${name}`);
  }
  api.on(name, onSignal);
  trace(`signal handler installed for signal ${signo}`);
  installed.add(signo);
}

/** `SignalNotifier`: wakes the peer with its signal and waits for ours. */
export class SignalNotifier {
  constructor(peer, peerSignal, kill = process.kill) {
    this.peer = peer;
    this.peerSignal = peerSignal;
    this.kill = kill;
  }

  generation() {
    return generation;
  }

  notify() {
    try {
      this.kill(this.peer, this.peerSignal);
    } catch {
      // The peer is gone; its liveness check notices.
    }
  }

  wait(seen, attempt, limit) {
    if (attempt < 16) return immediate();
    if (generation !== seen) return Promise.resolve();
    return caught.wait(limit);
  }
}

/** `own_signal`: the `signo` option (SIGUSR2 by default), installed. */
export function ownSignal(endpoint, api = process) {
  const signo = endpoint.number('signo', constants.signals.SIGUSR2, 1, 64);
  installSignal(signo, api);
  return signo;
}

/** `peer_signal`: the signal number a request or an ack asks for. */
export function peerSignal(fields) {
  const signo = fields.requireNumber('signal');
  if (signo < 1n || signo > 64n) {
    throw new IoError('InvalidData', `the peer asked for the invalid signal ${signo}`);
  }
  return Number(signo);
}

// ---------------------------------------------------------------- the transport

/** `RingTransport`: one side of two rings in a shared region. */
export class RingTransport {
  #closed = false;

  constructor(kind, region, client, notifier, peerPid, { cleanup = [], alive } = {}) {
    this.kind = kind;
    this.region = region;
    this.capacity = validate(region);
    const [outgoing, incoming] = client ? [0, 1] : [1, 0];
    this.outgoing = ringBase(outgoing, this.capacity);
    this.incoming = ringBase(incoming, this.capacity);
    this.notifier = notifier;
    this.peer = new PeerWatch(peerPid, alive);
    this.cleanup = cleanup;
  }

  /** `counter`: a value whose complement matches, read again while torn. */
  #counter(offset) {
    for (let attempt = 0; attempt <= 1_000_000; attempt += 1) {
      const bytes = this.region.readBytes(offset, 8);
      const value = bytes.readUInt32LE(0);
      if (value === ~bytes.readUInt32LE(4) >>> 0) return value;
    }
    throw new IoError('InvalidData', 'a ring counter stays torn');
  }

  #publish(offset, value) {
    this.region.store(offset, value);
    this.region.store(offset + 4, ~value >>> 0);
  }

  /** `[head, tail]`, or `null` while they disagree (Rust yields and retries). */
  #positions(base) {
    const head = this.#counter(base + HEAD);
    const tail = this.#counter(base + TAIL);
    return (head - tail) >>> 0 <= this.capacity ? [head, tail] : null;
  }

  #copyOut(base, position, count) {
    const data = base + RING_HEADER;
    const start = position % this.capacity;
    const first = Math.min(count, this.capacity - start);
    const bytes = this.region.readBytes(data + start, first);
    return first < count
      ? Buffer.concat([bytes, this.region.readBytes(data, count - first)])
      : bytes;
  }

  #copyIn(base, position, bytes) {
    const data = base + RING_HEADER;
    const start = position % this.capacity;
    const first = Math.min(bytes.length, this.capacity - start);
    this.region.writeBytes(data + start, bytes.subarray(0, first));
    if (first < bytes.length) this.region.writeBytes(data, bytes.subarray(first));
  }

  async read(max, timeoutMs) {
    const deadline = timeoutMs === null ? null : performance.now() + timeoutMs;
    let attempt = 0;
    let lastCheck = performance.now();
    for (;;) {
      if (this.#closed) return EMPTY;
      const seen = this.notifier.generation();
      const positions = this.#positions(this.incoming);
      if (positions === null) {
        await immediate();
        continue;
      }
      const [head, tail] = positions;
      const used = (head - tail) >>> 0;
      if (used > 0) {
        const count = Math.min(used, max);
        const bytes = this.#copyOut(this.incoming, tail, count);
        this.#publish(this.incoming + TAIL, (tail + count) >>> 0);
        this.notifier.notify();
        return bytes;
      }
      const state = this.region.load(this.incoming + WRITER_STATE);
      if (state === ABORTED) throw hungUp();
      // The writer may have published its last bytes after the positions
      // above were read: the end counts only once the ring is still empty.
      if (state !== OPEN) {
        const again = this.#positions(this.incoming);
        if (again === null || again[0] !== again[1]) continue;
        return EMPTY;
      }
      const now = performance.now();
      if (now - lastCheck >= PEER_CHECK) {
        lastCheck = now;
        if (!this.peer.alive()) throw hungUp();
      }
      let limit = PEER_CHECK;
      if (deadline !== null) {
        if (now >= deadline) throw timedOut();
        limit = Math.min(deadline - now, PEER_CHECK);
      }
      await this.notifier.wait(seen, attempt, limit);
      attempt += 1;
    }
  }

  async write(buffer) {
    let attempt = 0;
    let lastCheck = performance.now();
    for (;;) {
      if (this.#closed || this.region.load(this.outgoing + WRITER_STATE) !== OPEN) {
        throw outgoingClosed();
      }
      if (this.region.load(this.outgoing + READER_STATE) === GONE) throw brokenPipe();
      const seen = this.notifier.generation();
      const positions = this.#positions(this.outgoing);
      if (positions === null) {
        await immediate();
        continue;
      }
      const [head, tail] = positions;
      const free = this.capacity - ((head - tail) >>> 0);
      if (free > 0) {
        const count = Math.min(free, buffer.length);
        this.#copyIn(this.outgoing, head, buffer.subarray(0, count));
        this.#publish(this.outgoing + HEAD, (head + count) >>> 0);
        this.notifier.notify();
        return count;
      }
      const now = performance.now();
      if (now - lastCheck >= PEER_CHECK) {
        lastCheck = now;
        if (!this.peer.alive()) throw brokenPipe();
      }
      await this.notifier.wait(seen, attempt, PEER_CHECK);
      attempt += 1;
    }
  }

  async closeWrite() {
    if (this.#closed) return;
    if (this.region.load(this.outgoing + WRITER_STATE) === OPEN) {
      this.region.store(this.outgoing + WRITER_STATE, ENDED);
      this.notifier.notify();
    }
  }

  shutdown() {
    if (this.#closed) return;
    this.#closed = true;
    trace(`${this.kind} ring to process ${this.peer.pid} hung up`);
    quietly(() => {
      if (this.region.load(this.outgoing + WRITER_STATE) === OPEN) {
        this.region.store(this.outgoing + WRITER_STATE, ENDED);
      }
    });
    quietly(() => this.region.store(this.incoming + READER_STATE, GONE));
    this.notifier.notify();
    quietly(() => this.region.close());
    for (const path of this.cleanup) fs.rmSync(path, { force: true });
  }
}

function quietly(operation) {
  try {
    operation();
  } catch {
    // Rust ignores these errors too; the peer's liveness check notices.
  }
}

// ---------------------------------------------------------------- connect and listen

const SUFFIX = { mmap: 'mmap', file: 'ring', signal: 'sig' };

/** The file of POSIX shared memory object `/lcID` on Linux. */
export const shmPath = (id) => `/dev/shm/lc${id}`;

/**
 * Whether this port can use `kind` on `platform`: `shm` needs Linux's
 * `/dev/shm`, `signal` needs POSIX signals; otherwise the failure.
 */
export function ringUnsupported(kind, platform) {
  if (kind === 'shm' && platform !== 'linux') {
    return noNodeApi(
      'shm',
      platform === 'win32' ? 'named file mappings' : 'POSIX shared memory (`shm_open`)',
    );
  }
  if (kind === 'signal' && platform === 'win32') return noNodeApi('signal', 'Windows named events');
  return null;
}

/** `create_file`: a new file of `size` zero bytes, mode 0600 (`api`: `fs`). */
export function createFile(path, size, api = fs) {
  const fd = api.openSync(path, 'wx+', 0o600);
  try {
    api.ftruncateSync(fd, size);
  } catch (error) {
    api.closeSync(fd);
    api.rmSync(path, { force: true });
    throw error;
  }
  return fd;
}

/** `open_file`: an existing ring file and its length (`api`: `fs`). */
export function openFile(path, api = fs) {
  const fd = api.openSync(path, 'r+');
  try {
    return new FileRegion(fd, api.fstatSync(fd).size, api);
  } catch (error) {
    api.closeSync(fd);
    throw error;
  }
}

/**
 * `ring::connect`. `options`: `{platform, alive, api}` for tests (the
 * platform decides when the files are removed; `api` stands for `process`).
 */
export async function connectRing(endpoint, options = {}) {
  const platform = options.platform ?? process.platform;
  const kind = endpoint.kind;
  const failure = ringUnsupported(kind, platform);
  if (failure !== null) throw failure;
  const alive = options.alive ?? processAlive;
  const rendezvous = Rendezvous.begin(endpoint, kind, alive);
  const capacity = endpoint.number('capacity', 65536, 64, 1 << 30);
  const size = regionSize(capacity);
  const path = kind === 'shm' ? shmPath(rendezvous.id) : rendezvous.path(SUFFIX[kind]);
  const region = new FileRegion(createFile(path, size), size);
  const forget = () => fs.rmSync(path, { force: true });
  let transport;
  try {
    initialize(region, capacity);
    const poll = endpoint.millis('poll', 5);
    let fields = new JsonObject();
    if (kind === 'signal') fields = fields.with('signal', ownSignal(endpoint, options.api));
    const ack = await rendezvous.request(fields, alive);
    const peer = Number(BigInt.asUintN(32, ack.number('pid') ?? BigInt(rendezvous.serverPid)));
    const notifier =
      kind === 'signal' ? new SignalNotifier(peer, peerSignal(ack)) : new PollNotifier(poll);
    // Both sides hold the region open now: on POSIX nothing need outlive a
    // crash, while Windows deletes a file only once nobody has it open.
    const cleanup = platform === 'win32' ? [path] : [];
    transport = new RingTransport(kind, region, true, notifier, peer, { cleanup, alive });
    if (platform !== 'win32') forget();
  } catch (error) {
    region.close();
    forget();
    throw error;
  }
  trace(
    `${kind} connected as ${rendezvous.id} to process ${transport.peer.pid}, ${capacity} bytes per ring`,
  );
  return new IpcStream(transport);
}

/** `RingListener`. */
class RingListener {
  constructor(rendezvous, endpoint, kind, options) {
    this.rendezvous = rendezvous;
    this.endpointOption = endpoint;
    this.kind = kind;
    this.platform = options.platform ?? process.platform;
    this.alive = options.alive;
    this.api = options.api;
  }

  #open(request) {
    if (this.kind === 'shm') return { region: openFile(shmPath(request.id)), cleanup: [] };
    const path = request.path(SUFFIX[this.kind]);
    return { region: openFile(path), cleanup: this.platform === 'win32' ? [path] : [] };
  }

  #setUp(request) {
    const peer = request.pid();
    const { region, cleanup } = this.#open(request);
    try {
      let fields = new JsonObject();
      let notifier;
      if (this.kind === 'signal') {
        fields = fields.with('signal', ownSignal(this.endpointOption, this.api));
        notifier = new SignalNotifier(peer, peerSignal(request.fields));
      } else {
        notifier = new PollNotifier(this.endpointOption.millis('poll', 5));
      }
      const transport = new RingTransport(this.kind, region, false, notifier, peer, {
        cleanup,
        alive: this.alive,
      });
      return { fields, stream: new IpcStream(transport) };
    } catch (error) {
      region.close();
      throw error;
    }
  }

  accept() {
    return acceptWith(this.rendezvous, this.kind, (request) => this.#setUp(request));
  }

  endpoint() {
    return this.rendezvous.endpoint();
  }

  endpoints() {
    return [this.endpoint()];
  }

  localAddr() {
    return null;
  }

  close() {
    this.rendezvous.close();
  }
}

/** `ring::listen`. */
export async function listenRing(endpoint, options = {}) {
  const kind = endpoint.kind;
  const failure = ringUnsupported(kind, options.platform ?? process.platform);
  if (failure !== null) throw failure;
  return new RingListener(RendezvousListener.bind(endpoint, kind), endpoint, kind, options);
}
