// Unit tests of ipc-stream.js: `IpcStream` over fake transports (the
// serialised reads and writes, `Interrupted` retries, `WriteZero`), the
// `ThreadedReader` queue of rust/src/protocol/ipc/stdio.rs and the
// `SocketTransport` of rust/src/protocol/ipc/net.rs, with and without
// half-close (the Windows named pipe mode), over fake sockets.
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
import { describe, it } from 'node:test';
import { IpcStream, ReadQueue, SocketTransport } from '../src/ipc-stream.js';
import { IoError } from '../src/protocol-errors.js';
import { setTrace } from '../src/ipc-trace.js';
import { diagnostics } from '../src/protocol-errors.js';

const interrupted = () => new IoError('Interrupted', 'interrupted');

describe('IpcStream', () => {
  it('retries interrupted calls and writes everything', async () => {
    const written = [];
    let reads = 0;
    let writes = 0;
    let flushed = 0;
    let closed = 0;
    let shut = 0;
    const transport = {
      kind: 'fake',
      async read(max, timeout) {
        reads += 1;
        if (reads === 1) throw interrupted();
        return Buffer.from(`${max}/${timeout}`);
      },
      async write(bytes) {
        writes += 1;
        if (writes === 1) throw interrupted();
        written.push(bytes.subarray(0, 2).toString());
        return Math.min(bytes.length, 2);
      },
      flush: () => flushed++,
      closeWrite: () => closed++,
      shutdown: () => shut++,
    };
    const stream = new IpcStream(transport);
    assert.equal(stream.kind, 'fake');
    assert.equal(stream.transport, transport);
    assert.equal(stream.readTimeout, null);
    stream.setReadTimeout(25);
    assert.equal(stream.readTimeout, 25);
    assert.equal((await stream.read(5)).toString(), '5/25');
    stream.setReadTimeout(null);
    assert.equal((await stream.read()).toString(), '65536/null');
    assert.equal((await stream.read(0)).length, 0);
    await stream.write(new Uint8Array([97, 98, 99, 100, 101]));
    assert.deepEqual(written, ['ab', 'cd', 'e']);
    await stream.flush();
    await stream.closeWrite();
    stream.shutdown();
    assert.deepEqual([flushed, closed, shut], [1, 1, 1]);
    for (const bad of [-1, Number.NaN, Infinity, '5']) {
      assert.throws(() => stream.setReadTimeout(bad), {
        kind: 'InvalidInput',
        message: `a read timeout must be null or milliseconds, found ${bad}`,
      });
    }
  });

  it('fails a write the transport takes nothing of, and passes errors on', async () => {
    const stream = new IpcStream({
      kind: 'fake',
      read: async () => {
        throw new IoError('ConnectionReset', 'gone');
      },
      write: async () => 0,
      closeWrite: async () => {},
      shutdown() {},
    });
    await assert.rejects(stream.write(Buffer.from('x')), {
      kind: 'WriteZero',
      message: 'failed to write whole buffer',
    });
    await assert.rejects(stream.read(1), { message: 'gone' });
    // A failed operation does not stall the next one; flush is optional.
    await assert.rejects(stream.read(1), { message: 'gone' });
    await stream.flush();
  });
});

describe('ReadQueue', () => {
  it('pauses a full source and resumes it', async () => {
    const source = new PassThrough();
    const queue = new ReadQueue(source, { limit: 4 });
    source.write('abcdef');
    await new Promise((resolve) => setImmediate(resolve));
    assert.equal(source.isPaused(), true);
    assert.equal((await queue.read(2, null)).toString(), 'ab');
    assert.equal(source.isPaused(), true);
    assert.equal((await queue.read(3, null)).toString(), 'cde');
    assert.equal(source.isPaused(), false);
    source.end();
    assert.equal((await queue.read(9, null)).toString(), 'f');
    assert.equal((await queue.read(9, null)).length, 0);
  });

  it('times out, closes and discards', async () => {
    const source = new PassThrough();
    const queue = new ReadQueue(source);
    await assert.rejects(queue.read(1, 10), { kind: 'TimedOut' });
    source.write('x');
    await new Promise((resolve) => setImmediate(resolve));
    queue.close();
    assert.equal((await queue.read(1, null)).toString(), 'x');
    assert.equal((await queue.read(1, null)).length, 0);
    queue.discard();
    source.write('ignored');
    await new Promise((resolve) => setImmediate(resolve));
    assert.equal((await queue.read(1, null)).length, 0);
  });

  it('reports a source error once or on every read', async () => {
    const once = new PassThrough();
    const onceQueue = new ReadQueue(once);
    const pending = onceQueue.read(1, null);
    once.destroy(new Error('boom'));
    await assert.rejects(pending, { message: 'boom' });
    assert.equal((await onceQueue.read(1, null)).length, 0);
    const kept = new EventEmitter();
    const keptQueue = new ReadQueue(kept, { keepError: true });
    kept.emit('error', new Error('again'));
    await assert.rejects(keptQueue.read(1, null), { message: 'again' });
    await assert.rejects(keptQueue.read(1, null), { message: 'again' });
    const mapped = new EventEmitter();
    const mappedQueue = new ReadQueue(mapped);
    mapped.emit('error', new Error('quiet'));
    assert.equal((await mappedQueue.read(1, null, () => null)).length, 0);
  });
});

/** A `net.Socket` stand-in: records writes and ends, fails writes on demand. */
class FakeSocket extends EventEmitter {
  destroyed = false;
  writableEnded = false;
  failure = null;
  writes = [];
  ended = 0;
  soon = 0;

  pause() {}

  resume() {}

  write(buffer, callback) {
    this.writes.push(buffer);
    queueMicrotask(() => callback(this.failure));
  }

  end() {
    this.ended += 1;
  }

  destroy() {
    this.destroyed = true;
  }

  destroySoon() {
    this.soon += 1;
  }
}

const systemFailure = (code, errno) => Object.assign(new Error(code), { code, errno });

describe('SocketTransport', () => {
  it('maps socket errors as net.rs does', async () => {
    const cases = [
      [systemFailure('ENOTCONN', -107), null],
      [new IoError('ConnectionAborted', 'aborted'), null],
      [systemFailure('ETIMEDOUT', -110), 'timed out'],
      [new IoError('WouldBlock', 'later'), 'timed out'],
      [systemFailure('ECONNRESET', -104), 'ECONNRESET'],
    ];
    for (const [error, expected] of cases) {
      const socket = new FakeSocket();
      const transport = new SocketTransport('tcp', socket);
      socket.emit('error', error);
      if (expected === null) assert.equal((await transport.read(1, null)).length, 0);
      else await assert.rejects(transport.read(1, null), { message: expected });
    }
  });

  it('writes, half-closes and shuts down a socket', async () => {
    const socket = new FakeSocket();
    const transport = new SocketTransport('unix', socket);
    assert.equal(transport.socket, socket);
    assert.equal(await transport.write(Buffer.from('ab')), 2);
    socket.failure = systemFailure('ECONNRESET', -104);
    await assert.rejects(transport.write(Buffer.from('c')), { code: 'ECONNRESET' });
    socket.failure = new Error('no errno');
    await assert.rejects(transport.write(Buffer.from('c')), { code: 'EPIPE' });
    socket.failure = null;
    await transport.closeWrite();
    assert.equal(socket.ended, 1);
    socket.writableEnded = true;
    await assert.rejects(transport.write(Buffer.from('d')), { code: 'EPIPE', syscall: 'write' });
    socket.writableEnded = false;
    socket.destroyed = true;
    await assert.rejects(transport.write(Buffer.from('d')), { code: 'EPIPE' });
    socket.destroyed = false;
    transport.shutdown();
    transport.shutdown();
    assert.equal(socket.ended, 2);
    await assert.rejects(transport.write(Buffer.from('d')), { code: 'EPIPE' });
    assert.equal((await transport.read(1, null)).length, 0);
  });

  it('closes a Windows named pipe as a whole', async () => {
    const lines = [];
    const original = diagnostics.stderr;
    diagnostics.stderr = (line) => lines.push(line);
    setTrace(true);
    try {
      const socket = new FakeSocket();
      const transport = new SocketTransport('pipe', socket, { halfClose: false });
      socket.failure = systemFailure('EPIPE', -32);
      await assert.rejects(transport.write(Buffer.from('a')), {
        kind: 'BrokenPipe',
        message: 'the peer no longer reads',
      });
      socket.failure = systemFailure('EACCES', -13);
      await assert.rejects(transport.write(Buffer.from('a')), { code: 'EACCES' });
      socket.failure = null;
      socket.emit('error', systemFailure('EOF', -4095));
      assert.equal((await transport.read(1, null)).length, 0);
      await transport.closeWrite();
      await transport.closeWrite();
      assert.equal(socket.soon, 1);
      assert.equal((await transport.read(1, null)).length, 0);
      await assert.rejects(transport.write(Buffer.from('a')), { kind: 'BrokenPipe' });
      transport.shutdown();
      assert.equal(socket.soon, 1);

      const other = new FakeSocket();
      const second = new SocketTransport('pipe', other, { halfClose: false });
      other.emit('error', systemFailure('EACCES', -13));
      await assert.rejects(second.read(1, null), { code: 'EACCES' });
      second.shutdown();
      assert.equal(other.soon, 1);
      await assert.rejects(second.write(Buffer.from('a')), { kind: 'BrokenPipe' });
    } finally {
      setTrace(false);
      diagnostics.stderr = original;
    }
    assert.deepEqual(lines, ['[ipc] pipe closed', '[ipc] pipe closed']);
  });
});
