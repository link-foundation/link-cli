// `fifo` (and `pipe` on POSIX): two named FIFOs set up through the rendezvous
// directory (the "FIFO" section of docs/protocol/ipc-transports-v1.md; the
// reference is rust/src/protocol/ipc/unix/mod.rs).

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import net from 'node:net';
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
    const reader = new net.Socket({ fd: incoming, readable: true, writable: false });
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
