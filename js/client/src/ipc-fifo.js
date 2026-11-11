// `fifo` (and `pipe` on POSIX): two named FIFOs set up through the rendezvous
// directory (the "FIFO" section of docs/protocol/ipc-transports-v1.md; the
// reference is rust/src/protocol/ipc/unix/mod.rs).

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import net from 'node:net';
import { Readable } from 'node:stream';
import { IpcError } from './ipc-endpoint.js';
import { JoinedStream } from './ipc-joined.js';
import { PeerWatch, Rendezvous } from './ipc-rendezvous.js';
import { trace } from './ipc-trace.js';

// Node.js has no mkfifo(3); the POSIX utility does the same.
function mkfifo(file) {
  try {
    execFileSync('mkfifo', ['-m', '600', file], { stdio: ['ignore', 'ignore', 'pipe'] });
  } catch (cause) {
    const detail = cause.stderr?.toString().trim() || cause.message;
    throw new IpcError(`cannot create the FIFO \`${file}\`: ${detail}`, cause.code ?? 'EIO', {
      cause,
    });
  }
}

// The longest wait between two reads of an idle polled FIFO, as the Rust and
// C# readers check a macOS FIFO for its end.
const POLL_LIMIT = 20;

/**
 * Reads the non-blocking FIFO descriptor `fd` by polling. macOS reports a FIFO
 * readable only while it holds bytes, never when its writer closes (XNU
 * `filt_vnode_common`), so libuv never sees the end there. As the
 * specification asks, a read follows every wait, ready or not: bytes are
 * passed on at once, `EAGAIN` waits 1 ms, doubling up to 20 ms while idle, and
 * a read of 0 is the end.
 */
export class PolledFifoReader extends Readable {
  #fd;
  #timer = null;
  #wait = 1;
  #reading = false;
  #closing = null;

  constructor(fd) {
    super();
    this.#fd = fd;
  }

  // Readable calls this again only after a push, so one read runs at a time.
  _read() {
    this.#reading = true;
    const buffer = Buffer.allocUnsafe(65536);
    fs.read(this.#fd, buffer, 0, buffer.length, null, (error, bytes) => {
      this.#reading = false;
      if (this.#closing) {
        this.#closing();
        return;
      }
      if (error?.code === 'EAGAIN') {
        this.#timer = setTimeout(() => {
          this.#timer = null;
          this._read();
        }, this.#wait);
        this.#wait = Math.min(this.#wait * 2, POLL_LIMIT);
      } else if (error) {
        this.destroy(error);
      } else {
        this.#wait = 1;
        this.push(bytes === 0 ? null : buffer.subarray(0, bytes));
      }
    });
  }

  _destroy(error, callback) {
    clearTimeout(this.#timer);
    // The descriptor is closed only once no read uses it any more.
    const close = () => fs.close(this.#fd, () => callback(error));
    if (this.#reading) this.#closing = close;
    else close();
  }
}

/** The incoming side of a FIFO connection: polled on macOS, where libuv misses the end. */
export function fifoReader(fd, platform = process.platform) {
  if (platform === 'darwin') return new PolledFifoReader(fd);
  return new net.Socket({ fd, readable: true, writable: false });
}

/** Connects to a `fifo:` (or POSIX `pipe:`) listener. */
export async function connectFifo(endpoint, { signal } = {}) {
  const kind = endpoint.kind === 'pipe' ? 'pipe' : 'fifo';
  const rendezvous = Rendezvous.begin(endpoint, 'fifo');
  const c2s = rendezvous.path('c2s');
  const s2c = rendezvous.path('s2c');
  const remove = () => {
    fs.rmSync(c2s, { force: true });
    fs.rmSync(s2c, { force: true });
  };
  const { O_RDONLY, O_WRONLY, O_NONBLOCK } = fs.constants;
  let incoming;
  let outgoing;
  try {
    mkfifo(c2s);
    mkfifo(s2c);
    // Opened before the request, so the listener can open its writing end.
    incoming = fs.openSync(s2c, O_RDONLY | O_NONBLOCK);
    const ack = await rendezvous.request({}, signal);
    const peer = new PeerWatch(rendezvous.peerOf(ack));
    outgoing = fs.openSync(c2s, O_WRONLY | O_NONBLOCK);
    remove();
    // Sockets over the descriptors are created only now: with the listener's
    // writer open, end-of-file really is its end.
    const reader = fifoReader(incoming);
    const writer = new net.Socket({ fd: outgoing, readable: false, writable: true });
    const stream = new JoinedStream(reader, writer, {
      greeting: true,
      peerAlive: () => peer.alive(),
    });
    stream.peerPid = peer.pid;
    // The hello byte: tells the peer that this writer is open.
    writer.write(Buffer.of(0));
    trace(`${kind} connected as ${rendezvous.id}`);
    return stream;
  } catch (error) {
    remove();
    for (const fd of [incoming, outgoing]) {
      if (fd !== undefined) {
        try {
          fs.closeSync(fd);
        } catch {
          // Already closed.
        }
      }
    }
    throw error;
  }
}
