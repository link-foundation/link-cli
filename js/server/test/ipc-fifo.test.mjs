// Unit tests of ipc-fifo.js, the `fifo` part of rust/src/protocol/ipc/unix/mod.rs:
// `mkfifo`'s failures, `FifoTransport` over a stand-in `fs` (the hello byte,
// `EAGAIN`, a writer that never opens, a dead peer, timeouts, closed
// directions, other errors), and the rendezvous of `connect`/`listen` (Windows,
// a failing `mkfifo`, an unanswered request, an ack without a pid, a listener
// that cannot open the client's FIFO).
import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as os from 'node:os';
import { join } from 'node:path';
import { after, describe, it } from 'node:test';
import { Endpoint } from '../src/ipc-endpoint.js';
import { connectFifo, FifoTransport, listenFifo, mkfifo, openFifo } from '../src/ipc-fifo.js';
import { RendezvousListener, writeAtomically } from '../src/ipc-rendezvous.js';
import { setTrace } from '../src/ipc-trace.js';
import { systemError } from '../src/ipc-util.js';
import { diagnostics } from '../src/protocol-errors.js';

const root = fs.mkdtempSync(join(os.tmpdir(), 'ipc-fifo-'));
after(() => fs.rmSync(root, { recursive: true, force: true }));
let dirs = 0;
const freshDir = () => join(root, `d${(dirs += 1)}`);
const WINDOWS = process.platform === 'win32';
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** Collects the `[ipc]` traces of `body`. */
async function traced(body) {
  const lines = [];
  const original = diagnostics.stderr;
  diagnostics.stderr = (line) => lines.push(line);
  setTrace(true);
  try {
    await body();
  } finally {
    setTrace(false);
    diagnostics.stderr = original;
  }
  return lines;
}

describe('mkfifo', () => {
  it('fails like Rust on an existing path or a missing directory', () => {
    const run = () => assert.fail('the utility must not run');
    assert.throws(() => mkfifo(root, run), { code: 'EEXIST', syscall: 'mkfifo' });
    assert.throws(() => mkfifo(join(root, 'missing', 'f'), run), { code: 'ENOENT' });
  });

  it("reports the utility's error output, or its failure", () => {
    const path = join(root, 'f');
    const calls = [];
    const failing =
      (stderr) =>
      (...args) => {
        calls.push(args);
        throw Object.assign(new Error('Command failed: mkfifo'), { stderr });
      };
    assert.throws(() => mkfifo(path, failing(Buffer.from('mkfifo: no room\n'))), {
      kind: 'Other',
      message: `cannot create the FIFO \`${path}\`: mkfifo: no room`,
    });
    for (const stderr of [Buffer.from(' \n'), undefined]) {
      assert.throws(() => mkfifo(path, failing(stderr)), {
        message: `cannot create the FIFO \`${path}\`: Command failed: mkfifo`,
      });
    }
    assert.deepEqual(calls[0], [
      'mkfifo',
      ['-m', '600', path],
      { stdio: ['ignore', 'ignore', 'pipe'] },
    ]);
  });
});

/**
 * An `fs` stand-in: `reads` and `writes` hold the outcome of each call in
 * turn (bytes, a count or an error); once empty they fail with `EAGAIN`.
 */
function fakeFs(reads = [], writes = []) {
  const api = { closed: [], written: [] };
  api.readSync = (_fd, buffer, offset, length) => {
    const next = reads.length > 0 ? reads.shift() : systemError('EAGAIN', 'read');
    if (next instanceof Error) throw next;
    const count = Math.min(next.length, length);
    next.copy(buffer, offset, 0, count);
    return count;
  };
  api.writeSync = (_fd, buffer, offset, length) => {
    const next = writes.length > 0 ? writes.shift() : systemError('EAGAIN', 'write');
    if (next instanceof Error) throw next;
    api.written.push(Buffer.from(buffer.subarray(offset, offset + length)));
    return length;
  };
  api.closeSync = (fd) => api.closed.push(fd);
  return api;
}

const hello = Buffer.of(0);
const transport = (api, alive = () => true) => new FifoTransport('fifo', 3, 4, 77, { alive, api });

describe('FifoTransport', { timeout: 30_000 }, () => {
  it('opens with the hello byte, waiting while the FIFO is full', async () => {
    const api = fakeFs([], [systemError('EINTR', 'write'), systemError('EAGAIN', 'write'), 1, 1]);
    const opened = await FifoTransport.open('fifo', 3, 4, 77, { api });
    assert.deepEqual(api.written, [hello]);
    await opened.write(Buffer.alloc(600));
    assert.equal(api.written[1].length, 512, 'writes stay atomic');
  });

  it('closes both ends when the hello byte fails', async () => {
    const api = fakeFs([], [systemError('EPIPE', 'write')]);
    await assert.rejects(FifoTransport.open('fifo', 3, 4, 77, { api }), { code: 'EPIPE' });
    assert.deepEqual(api.closed, [4, 3]);
  });

  it('skips the hello byte, alone or before the first bytes', async () => {
    const api = fakeFs([hello, Buffer.from('ab'), Buffer.alloc(0)]);
    const fifo = transport(api);
    assert.equal((await fifo.read(10, null)).toString(), 'ab');
    assert.equal((await fifo.read(10, null)).length, 0);
    const again = transport(fakeFs([Buffer.from('\0cd')]));
    assert.equal((await again.read(10, 1000)).toString(), 'cd');
  });

  it('waits for a writer that has not opened yet, but not for a dead one', async () => {
    const opening = transport(fakeFs([Buffer.alloc(0), Buffer.from('\0x')]));
    assert.equal((await opening.read(10, null)).toString(), 'x');
    const ended = transport(fakeFs(new Array(1000).fill(Buffer.alloc(0))), () => false);
    await assert.rejects(ended.read(10, null), { kind: 'ConnectionReset' });
    await assert.rejects(transport(fakeFs(), () => false).read(10, null), {
      kind: 'ConnectionReset',
      message: 'the peer hung up',
    });
  });

  it('times out and passes other errors on', async () => {
    await assert.rejects(transport(fakeFs()).read(10, 20), { kind: 'TimedOut' });
    await assert.rejects(transport(fakeFs([systemError('EIO', 'read')])).read(10, null), {
      code: 'EIO',
    });
    await assert.rejects(transport(fakeFs([], [systemError('EPIPE', 'write')])).write(hello), {
      code: 'EPIPE',
    });
  });

  it('refuses writes once a direction or the connection is closed', async () => {
    const api = fakeFs();
    const fifo = transport(api);
    await fifo.closeWrite();
    await fifo.closeWrite();
    await assert.rejects(fifo.write(hello), {
      kind: 'BrokenPipe',
      message: 'the outgoing direction was closed',
    });
    const waiting = transport(api);
    const writing = waiting.write(hello);
    await delay(5);
    const lines = await traced(async () => {
      waiting.shutdown();
      waiting.shutdown();
    });
    assert.deepEqual(lines, ['[ipc] fifo to process 77 hung up']);
    await assert.rejects(writing, { kind: 'BrokenPipe', message: 'the connection was closed' });
    assert.equal((await waiting.read(10, null)).length, 0);
    assert.deepEqual(api.closed, [4, 4, 3]);
  });
});

describe('connect and listen', { timeout: 30_000 }, () => {
  it("fails on Windows with Rust's text", async () => {
    const endpoint = Endpoint.parse(`fifo:${root}`);
    await assert.rejects(connectFifo(endpoint, { platform: 'win32' }), {
      kind: 'Unsupported',
      message: 'connecting on `fifo:` endpoints is not supported on this platform',
    });
    await assert.rejects(listenFifo(endpoint, { platform: 'win32' }), {
      kind: 'Unsupported',
      message: 'listening on `fifo:` endpoints is not supported on this platform',
    });
  });

  const posix = { skip: WINDOWS && 'Windows has no FIFOs' };

  it('cleans up after a failing mkfifo or an unanswered request', posix, async () => {
    const dir = freshDir();
    const listener = await listenFifo(Endpoint.parse(`fifo:${dir}`));
    assert.deepEqual([listener.endpoints(), listener.localAddr()], [[`fifo:${dir}`], null]);
    let made = 0;
    const failing = (path) => {
      made += 1;
      if (made === 2) throw new Error('no room for the FIFO');
      mkfifo(path);
    };
    await assert.rejects(connectFifo(Endpoint.parse(`fifo:${dir}`), { mkfifo: failing }), {
      message: 'no room for the FIFO',
    });
    const unanswered = connectFifo(Endpoint.parse(`fifo:${dir}?timeout=200`));
    await assert.rejects(unanswered, { kind: 'TimedOut' });
    assert.deepEqual(
      fs.readdirSync(dir).filter((name) => /\.(c2s|s2c)$/.test(name)),
      [],
    );
    listener.close();
  });

  it("takes the listener's process id when the ack lacks one", posix, async () => {
    const endpoint = Endpoint.parse(`fifo:${freshDir()}`);
    const rendezvous = RendezvousListener.bind(endpoint, 'fifo');
    const connecting = connectFifo(endpoint);
    const request = await rendezvous.next();
    const incoming = openFifo(request.path('c2s'), false);
    writeAtomically(request.path('ack'), '{"v":1}');
    const stream = await connecting;
    assert.equal(stream.transport.peer.pid, process.pid);
    stream.shutdown();
    fs.closeSync(incoming);
    rendezvous.close();
  });

  it('refuses a client whose FIFO the listener cannot open', posix, async () => {
    const dir = freshDir();
    const listener = await listenFifo(Endpoint.parse(`pipe:${dir}`));
    const accepting = listener.accept();
    // Without the client-to-server FIFO, the listener's second open fails.
    const halfway = (path) => (path.endsWith('.c2s') ? undefined : mkfifo(path));
    await assert.rejects(connectFifo(Endpoint.parse(`pipe:${dir}`), { mkfifo: halfway }), {
      kind: 'ConnectionRefused',
      message: /^the listener refused: /,
    });
    listener.close();
    assert.equal(await accepting, null);
  });
});
