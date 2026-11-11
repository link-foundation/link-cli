// A minimal listener side of docs/protocol/ipc-transports-v1.md in JS, for
// unit tests of the client connectors without the Rust `clink`: the
// rendezvous directory (fifo and the rings) and the `udp` handshake.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import dgram from 'node:dgram';
import fs from 'node:fs';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { DatagramSession, HELLO, WELCOME, frame, parseFrame } from '../../src/ipc-datagram.js';
import { JoinedStream } from '../../src/ipc-joined.js';
import { IpcError } from '../../src/ipc-endpoint.js';
import { toJson } from '../../src/ipc-rendezvous.js';
import { FileRegion, PollNotifier, RingStream, SignalNotifier } from '../../src/ipc-ring.js';

export const rejectsWith = (code, pattern) => (error) => {
  assert.ok(error instanceof IpcError, `an IpcError, got ${error}`);
  assert.equal(error.code, code, error.message);
  if (pattern) assert.match(error.message, pattern);
  return true;
};

export const tempDir = (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'ipc-test-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  return dir;
};

export const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** The pid of a process that has exited. */
export const deadPid = () => spawnSync(process.execPath, ['-e', '']).pid;

export const collect = (stream) =>
  new Promise((resolve, reject) => {
    const chunks = [];
    stream.on('data', (chunk) => chunks.push(chunk));
    stream.once('end', () => resolve(Buffer.concat(chunks)));
    stream.once('error', reject);
  });

export const payload = (length) =>
  Buffer.from(Array.from({ length }, (_, index) => (index * 7) & 0xff));

/** Echoes every server stream back, ending when its client ends. */
export const echo = (stream) => {
  stream.on('error', () => {});
  stream.pipe(stream);
};

const SUFFIX = { mmap: 'mmap', file: 'ring', signal: 'sig' };

/** Writes `file` as `file.tmp` and renames it, as both sides of the protocol do. */
export function writeAtomically(file, text) {
  fs.writeFileSync(`${file}.tmp`, text);
  fs.renameSync(`${file}.tmp`, file);
}

/** Writes `listener.json` of `dir`; `fields` replace the defaults. */
export function writeListener(dir, fields = {}) {
  fs.mkdirSync(dir, { recursive: true });
  writeAtomically(
    path.join(dir, 'listener.json'),
    toJson({ v: 1, kind: 'fifo', pid: process.pid, ...fields }),
  );
}

/**
 * Listens on the rendezvous directory `dir` for `kind`: every request goes to
 * `answer(id, request)`, which resolves to the ack fields (the default opens
 * the kind's channel and hands the server stream to `onStream`). `null`
 * leaves the request unanswered. A `fifo` or `pipe` greeting waits
 * `greetAfter` ms.
 */
export class Listener {
  #timer;
  #busy = false;
  streams = [];
  requests = [];

  constructor(dir, kind, { answer, onStream = () => {}, fields = {}, greetAfter = 0 } = {}) {
    this.dir = dir;
    this.kind = kind;
    this.greetAfter = greetAfter;
    this.answer = answer ?? ((id, request) => this.#open(id, request));
    this.onStream = onStream;
    writeListener(dir, { kind: kind === 'pipe' ? 'fifo' : kind, ...fields });
    this.#timer = setInterval(() => this.#scan(), 2);
  }

  async #scan() {
    if (this.#busy) return;
    this.#busy = true;
    try {
      for (const name of fs.readdirSync(this.dir)) {
        if (!name.endsWith('.req')) continue;
        const id = name.slice(0, -'.req'.length);
        const file = path.join(this.dir, name);
        const request = JSON.parse(fs.readFileSync(file, 'utf8'));
        fs.rmSync(file);
        this.requests.push(request);
        const ack = await this.answer(id, request);
        if (ack !== null) {
          writeAtomically(path.join(this.dir, `${id}.ack`), toJson({ v: 1, ...ack }));
        }
      }
    } finally {
      this.#busy = false;
    }
  }

  #open(id, request) {
    const file = (suffix) => path.join(this.dir, `${id}.${suffix}`);
    if (this.kind === 'fifo' || this.kind === 'pipe') {
      const { O_RDONLY, O_WRONLY, O_NONBLOCK } = fs.constants;
      const incoming = fs.openSync(file('c2s'), O_RDONLY | O_NONBLOCK);
      const outgoing = fs.openSync(file('s2c'), O_WRONLY | O_NONBLOCK);
      const reader = new net.Socket({ fd: incoming, readable: true, writable: false });
      const writer = new net.Socket({ fd: outgoing, readable: false, writable: true });
      const greet = () => {
        writer.write(Buffer.of(0));
        this.#hand(new JoinedStream(reader, writer, { greeting: true }));
      };
      if (this.greetAfter > 0) setTimeout(greet, this.greetAfter);
      else greet();
      return { pid: process.pid };
    }
    const region = this.kind === 'shm' ? `/dev/shm/lc${id}` : file(SUFFIX[this.kind]);
    const size = fs.statSync(region).size;
    const signal = this.kind === 'signal';
    const notifier = signal ? new SignalNotifier(request.pid, request.signal) : new PollNotifier(1);
    this.#hand(
      new RingStream({
        kind: this.kind,
        region: new FileRegion(fs.openSync(region, 'r+'), size),
        client: false,
        notifier,
        peerPid: request.pid,
      }),
    );
    return signal ? { pid: process.pid, signal: request.signal } : { pid: process.pid };
  }

  #hand(stream) {
    this.streams.push(stream);
    this.onStream(stream);
  }

  close() {
    clearInterval(this.#timer);
    for (const stream of this.streams) stream.destroy();
    fs.rmSync(path.join(this.dir, 'listener.json'), { force: true });
  }
}

/**
 * A `udp:` listener on `host`: answers each HELLO with a WELCOME (its token
 * and `chunk`) from a fresh socket, then runs a session on that socket.
 * `welcome(own, reply)` may send other frames first, or return false to stay
 * silent; `early` frames follow the WELCOME at once.
 */
export async function udpListener({
  host = '127.0.0.1',
  chunk = 1200,
  early = [],
  onStream = () => {},
  welcome,
} = {}) {
  const type = net.isIPv6(host) ? 'udp6' : 'udp4';
  const bound = async () => {
    const socket = dgram.createSocket(type);
    await new Promise((resolve) => socket.bind(0, host, resolve));
    return socket;
  };
  const socket = await bound();
  const sessions = [];
  const seen = new Set();
  socket.on('message', async (bytes, source) => {
    const hello = parseFrame(bytes);
    if (hello?.type !== HELLO || seen.has(source.port)) return;
    seen.add(source.port);
    const own = await bound();
    // A client that is gone makes the kernel refuse what is still sent to it.
    own.on('error', () => {});
    own.connect(source.port, source.address);
    await new Promise((resolve) => own.once('connect', resolve));
    const reply = Buffer.alloc(20);
    hello.payload.copy(reply, 0, 0, 16);
    reply.writeUInt32LE(chunk, 16);
    if ((await welcome?.(own, reply)) === false) {
      own.close();
      return;
    }
    const session = new DatagramSession(
      'udp',
      { send: (frameBytes) => own.send(frameBytes), close: () => own.close() },
      { chunk: Math.min(chunk, hello.payload.readUInt32LE(16)), rto: 5 },
    );
    sessions.push(session);
    own.on('message', (frameBytes) => session.receive(frameBytes));
    own.send(frame(WELCOME, 0, reply));
    for (const bytes of early) own.send(bytes);
    onStream(session);
  });
  return {
    port: socket.address().port,
    sessions,
    close() {
      for (const session of sessions) session.destroy();
      socket.close();
    },
  };
}
