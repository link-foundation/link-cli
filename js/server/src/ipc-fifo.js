// `fifo:DIR` (and `pipe:DIR` on POSIX): two named FIFOs set up through the
// rendezvous directory (the "FIFO" section of
// docs/protocol/ipc-transports-v1.md). A port of the `fifo` part of
// rust/src/protocol/ipc/unix/mod.rs (`FifoTransport`, `connect_fifo`,
// `listen_fifo`), adapted from js/client/src/ipc-fifo.js.
//
// Node.js has neither `mkfifo(3)` nor `poll(2)`: the POSIX `mkfifo` utility
// creates the FIFOs, and the descriptors are opened non-blocking and read and
// written synchronously, waiting 1 ms, doubling up to 20 ms, after every
// `EAGAIN` (where Rust polls). A read follows every wait, so the end of the
// writer is seen on macOS too, as the specification asks. Windows has no
// FIFOs: there `fifo:` fails with Rust's text, and `pipe:` is a named pipe
// (ipc-net.js).
import { execFileSync } from 'node:child_process';
import * as fs from 'node:fs';
import { dirname } from 'node:path';
import { errorText } from './error-report.js';
import { IoError, ioErrorKind } from './protocol-errors.js';
import { trace } from './ipc-trace.js';
import { IpcStream } from './ipc-stream.js';
import { JsonObject } from './ipc-json.js';
import { acceptWith, Rendezvous, RendezvousListener } from './ipc-rendezvous.js';
import {
  hungUp,
  outgoingClosed,
  PeerWatch,
  processAlive,
  sleep,
  systemError,
  timedOut,
  unsupported,
} from './ipc-util.js';

const WAIT_LIMIT = 20;
// POSIX guarantees that writes of up to `PIPE_BUF` (at least 512 bytes) are
// atomic on a pipe: written whole or failing with `EAGAIN`. Deno's `writeSync`
// loops over a partial write and then throws `EAGAIN`, losing the count, so
// every write stays within it.
const ATOMIC_WRITE = 512;
const EMPTY = Buffer.alloc(0);
const HELLO = Buffer.of(0);

/**
 * `mkfifo(path)` with mode 0600, through the `mkfifo` utility; an existing
 * path or a missing directory fails with the errno Rust reports.
 */
export function mkfifo(path, run = execFileSync) {
  if (fs.existsSync(path)) throw systemError('EEXIST', 'mkfifo');
  if (!fs.existsSync(dirname(path))) throw systemError('ENOENT', 'mkfifo');
  try {
    run('mkfifo', ['-m', '600', path], { stdio: ['ignore', 'ignore', 'pipe'] });
  } catch (error) {
    const detail = error.stderr?.toString().trim() || errorText(error);
    throw new IoError('Other', `cannot create the FIFO \`${path}\`: ${detail}`);
  }
}

/** `open_fifo`: a non-blocking descriptor of one end. */
export function openFifo(path, write) {
  const { O_RDONLY, O_WRONLY, O_NONBLOCK } = fs.constants;
  return fs.openSync(path, (write ? O_WRONLY : O_RDONLY) | O_NONBLOCK);
}

const wouldBlock = (error) => ['WouldBlock', 'Interrupted'].includes(ioErrorKind(error));

/** `FifoTransport`: one FIFO per direction; `api` stands for `fs` in tests. */
export class FifoTransport {
  #incoming;
  #outgoing;
  #greeted = false;
  #closed = false;

  constructor(kind, incoming, outgoing, peer, { alive = processAlive, api = fs } = {}) {
    this.kind = kind;
    this.#incoming = incoming;
    this.#outgoing = outgoing;
    this.peer = new PeerWatch(peer, alive);
    this.api = api;
  }

  /** `FifoTransport::new`: the hello byte tells the peer that this writer is open. */
  static async open(kind, incoming, outgoing, peer, options) {
    const transport = new FifoTransport(kind, incoming, outgoing, peer, options);
    try {
      await transport.write(HELLO);
    } catch (error) {
      transport.release();
      throw error;
    }
    return transport;
  }

  async read(max, timeoutMs) {
    const deadline = timeoutMs === null ? null : performance.now() + timeoutMs;
    const buffer = Buffer.alloc(max + 1);
    let wait = 1;
    for (;;) {
      if (this.#closed) return EMPTY;
      let count;
      try {
        count = this.api.readSync(this.#incoming, buffer, 0, this.#greeted ? max : max + 1, null);
      } catch (error) {
        if (!wouldBlock(error)) throw error;
        if (!this.#greeted && !this.peer.alive()) throw hungUp();
        count = null;
      }
      if (count === 0) {
        if (this.#greeted) return EMPTY;
        // The writer has not opened yet.
        if (!this.peer.alive()) throw hungUp();
      } else if (count !== null) {
        if (this.#greeted) return Buffer.from(buffer.subarray(0, count));
        this.#greeted = true;
        if (count > 1) return Buffer.from(buffer.subarray(1, count));
        continue;
      }
      let pause = wait;
      if (deadline !== null) {
        const remaining = deadline - performance.now();
        if (remaining <= 0) throw timedOut();
        pause = Math.min(pause, remaining);
      }
      await sleep(pause);
      wait = Math.min(wait * 2, WAIT_LIMIT);
    }
  }

  async write(buffer) {
    if (this.#outgoing === null) throw outgoingClosed();
    let wait = 1;
    for (;;) {
      if (this.#closed) throw new IoError('BrokenPipe', 'the connection was closed');
      try {
        const length = Math.min(buffer.length, ATOMIC_WRITE);
        return this.api.writeSync(this.#outgoing, buffer, 0, length, null);
      } catch (error) {
        if (!wouldBlock(error)) throw error;
      }
      await sleep(wait);
      wait = Math.min(wait * 2, WAIT_LIMIT);
    }
  }

  async closeWrite() {
    const outgoing = this.#outgoing;
    this.#outgoing = null;
    if (outgoing !== null) this.api.closeSync(outgoing);
  }

  shutdown() {
    if (this.#closed) return;
    this.#closed = true;
    trace(`${this.kind} to process ${this.peer.pid} hung up`);
    this.release();
  }

  /** Rust's `Drop`: closes both descriptors. */
  release() {
    this.#closed = true;
    this.closeWrite();
    if (this.#incoming !== null) this.api.closeSync(this.#incoming);
    this.#incoming = null;
  }
}

/** `fifo_kind`. */
const fifoKind = (endpoint) => (endpoint.kind === 'pipe' ? 'pipe' : 'fifo');

const onWindows = (platform, what) => {
  if (platform === 'win32') throw unsupported('fifo', what);
};

/** `connect_fifo`. `options`: `{platform, alive, mkfifo}` for tests. */
export async function connectFifo(endpoint, options = {}) {
  onWindows(options.platform ?? process.platform, 'connecting');
  const kind = fifoKind(endpoint);
  const alive = options.alive ?? processAlive;
  const make = options.mkfifo ?? mkfifo;
  const rendezvous = Rendezvous.begin(endpoint, 'fifo', alive);
  const c2s = rendezvous.path('c2s');
  const s2c = rendezvous.path('s2c');
  const remove = () => {
    fs.rmSync(c2s, { force: true });
    fs.rmSync(s2c, { force: true });
  };
  let incoming;
  try {
    make(c2s);
    make(s2c);
    incoming = openFifo(s2c, false);
  } catch (error) {
    remove();
    throw error;
  }
  let transport;
  try {
    const ack = await rendezvous.request(new JsonObject(), alive);
    const peer = Number(BigInt.asUintN(32, ack.number('pid') ?? BigInt(rendezvous.serverPid)));
    const outgoing = openFifo(c2s, true);
    transport = await FifoTransport.open(kind, incoming, outgoing, peer, { alive });
  } catch (error) {
    fs.closeSync(incoming);
    throw error;
  } finally {
    remove();
  }
  trace(`${kind} connected as ${rendezvous.id}`);
  return new IpcStream(transport);
}

/** `FifoListener`. */
class FifoListener {
  constructor(rendezvous, kind, alive) {
    this.rendezvous = rendezvous;
    this.kind = kind;
    this.alive = alive;
  }

  async #open(request) {
    const outgoing = openFifo(request.path('s2c'), true);
    let incoming;
    try {
      incoming = openFifo(request.path('c2s'), false);
    } catch (error) {
      fs.closeSync(outgoing);
      throw error;
    }
    const transport = await FifoTransport.open(this.kind, incoming, outgoing, request.pid(), {
      alive: this.alive,
    });
    return { fields: new JsonObject(), stream: new IpcStream(transport) };
  }

  accept() {
    return acceptWith(this.rendezvous, this.kind, (request) => this.#open(request));
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

/** `listen_fifo`. `options`: `{platform, alive}` for tests. */
export async function listenFifo(endpoint, options = {}) {
  onWindows(options.platform ?? process.platform, 'listening');
  const rendezvous = RendezvousListener.bind(endpoint, 'fifo');
  return new FifoListener(rendezvous, fifoKind(endpoint), options.alive ?? processAlive);
}
