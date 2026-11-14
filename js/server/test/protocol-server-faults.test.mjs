// The failures of a links server (src/protocol-server.js) that a real socket
// cannot be made to produce on demand, reached with fake listeners and
// streams: failing and exhausted listeners, streams accepted while the server
// stops, failing reads and writes, and a store that cannot be marked for
// triplets. Trace lines are compared with the texts of the Rust `eprintln!`s.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { LinkStorage } from '../src/link-storage.js';
import { IoError } from '../src/protocol-errors.js';
import { ACCEPT_RETRY_MS, LinksServer, ServerOptions } from '../src/protocol-server.js';
import { pause, traced, until } from './helpers/protocol-server.mjs';

test('a server in triplet mode that cannot mark its store does not serve', async () => {
  const listener = new FakeListener([]);
  const server = LinksServer.fromListener(listener, new ServerOptions({ triplets: true }));
  const sessions = {
    ensureTriplets() {
      throw new Error('the store is read-only', { cause: new Error('disk full') });
    },
  };
  // Rust maps the error to `io::Error::other(format!("{error:#}"))`.
  await assert.rejects(server.serveSessions(sessions), {
    name: 'ProtocolError',
    message: 'I/O error: the store is read-only: disk full',
  });
  assert.equal(listener.closed, true);
});

/**
 * A listener answering `accept` with the next of `script` (a function of the
 * listener); once the script is over, it waits until closed and accepts no more.
 */
class FakeListener {
  constructor(script) {
    this.script = script;
    this.closed = false;
    this.closing = new Promise((resolve) => {
      this.wake = resolve;
    });
  }

  async accept() {
    const next = this.script.shift();
    if (next !== undefined) return next(this);
    await this.closing;
    return null;
  }

  endpoint() {
    return 'fake:1';
  }

  endpoints() {
    return ['fake:1'];
  }

  localAddr() {
    return null;
  }

  close() {
    this.closed = true;
    this.wake();
  }
}

/**
 * A stream reading `input` (Buffers, or Errors to fail with), then waiting
 * until shut down: then it ends, or fails with `failOnShutdown`. Its writes
 * fail with `writeError` when given.
 */
class FakeStream {
  constructor({ input = [], writeError = null, failOnShutdown = null } = {}) {
    this.input = input;
    this.writeError = writeError;
    this.failOnShutdown = failOnShutdown;
    this.written = [];
    this.reads = 0;
    this.shutdowns = 0;
    this.down = new Promise((resolve) => {
      this.wake = resolve;
    });
  }

  get kind() {
    return 'fake';
  }

  async read() {
    this.reads += 1;
    const next = this.input.shift();
    if (next instanceof Error) throw next;
    if (next !== undefined) return next;
    await this.down;
    if (this.failOnShutdown !== null) throw this.failOnShutdown;
    return Buffer.alloc(0);
  }

  async write(bytes) {
    if (this.writeError !== null) throw this.writeError;
    this.written.push(Buffer.from(bytes).toString('utf8'));
  }

  async flush() {}

  shutdown() {
    this.shutdowns += 1;
    this.wake();
  }
}

/** Serves an in-memory store from `listener` with `options` until `body` is done. */
async function serveFake(listener, options, body) {
  const server = LinksServer.fromListener(listener, new ServerOptions(options));
  const serving = server.serve(LinkStorage.inMemory());
  try {
    await body(server, serving);
  } finally {
    server.shutdownHandle().shutdown();
    await serving;
    server.disconnectAll();
  }
}

test('a failing accept is retried, and a listener that accepts no more stops the server', async () => {
  const listener = new FakeListener([
    () => {
      throw new IoError('Other', 'too many open files');
    },
    () => null,
  ]);
  const lines = await traced(async () => {
    const server = LinksServer.fromListener(listener, new ServerOptions({ trace: true }));
    const started = Date.now();
    // The listener ends the serving loop by itself.
    await server.serve(LinkStorage.inMemory());
    assert.ok(Date.now() - started >= ACCEPT_RETRY_MS - 1);
  });
  assert.deepEqual(lines, [
    '[server] accept failed: too many open files',
    '[server] fake:1 accepts no more connections',
  ]);
  assert.equal(listener.closed, true);
});

test('a server stopped while a failed accept waits accepts no more', async () => {
  let server = null;
  const listener = new FakeListener([
    () => {
      server.shutdownHandle().shutdown();
      throw new IoError('Other', 'interrupted');
    },
  ]);
  const accept = listener.accept.bind(listener);
  let accepts = 0;
  listener.accept = () => {
    accepts += 1;
    return accept();
  };
  server = LinksServer.fromListener(listener);
  await server.serve(LinkStorage.inMemory());
  assert.equal(accepts, 1);
});

test('a listener that accepts no more is not traced when tracing is off', async () => {
  const lines = await traced(async () => {
    await LinksServer.fromListener(new FakeListener([() => null])).serve(LinkStorage.inMemory());
  });
  assert.deepEqual(lines, []);
});

test('a stream accepted while the server stops is hung up', async () => {
  const stream = new FakeStream();
  const listener = new FakeListener([
    async (self) => {
      await self.closing;
      return stream;
    },
  ]);
  const lines = await traced(() => serveFake(listener, { trace: true }, async () => {}));
  assert.equal(stream.shutdowns, 1);
  assert.equal(stream.reads, 0);
  // Nor is a listener that closes because the server stops traced.
  assert.deepEqual(lines, []);
});

test('a read failure is answered in text and closes the connection', async () => {
  const stream = new FakeStream({ input: [new IoError('ConnectionReset', 'connection reset')] });
  const listener = new FakeListener([() => stream]);
  const lines = await traced(async (lines) => {
    await serveFake(listener, { trace: true }, async () => {
      await until(() => lines.length === 2);
    });
  });
  assert.deepEqual(stream.written, ["(error: 'I/O error: connection reset')\n.\n"]);
  assert.deepEqual(lines, [
    '[server] fake connection accepted',
    '[server] connection closed: I/O error: connection reset',
  ]);
});

test('a failing write closes the connection', async () => {
  const stream = new FakeStream({
    input: [Buffer.from('() ((1 1))\n.\n')],
    writeError: new IoError('BrokenPipe', 'broken pipe'),
  });
  const listener = new FakeListener([() => stream]);
  const lines = await traced(async (lines) => {
    await serveFake(listener, { trace: true }, async () => {
      await until(() => lines.length === 5);
    });
  });
  assert.equal(stream.shutdowns >= 1, true);
  // The writer and the reader of a Rust connection run on threads of their own.
  assert.deepEqual(lines.slice(0, 3), [
    '[server] fake connection accepted',
    '[server] request: () ((1 1))',
    '[server] reply: () ((1: 1 1))',
  ]);
  assert.deepEqual(lines.slice(3).sort(), [
    '[server] connection closed',
    '[server] connection closed: I/O error: broken pipe',
  ]);
});

test('a read that fails because the server hung up ends the connection quietly', async () => {
  const stream = new FakeStream({ failOnShutdown: new IoError('NotConnected', 'gone') });
  const listener = new FakeListener([() => stream]);
  const lines = await traced(async (lines) => {
    await serveFake(listener, { trace: true }, async (server) => {
      await until(() => stream.reads === 1);
      server.disconnectAll();
      await until(() => stream.shutdowns === 1);
      await pause(20);
      assert.deepEqual(lines, ['[server] fake connection accepted']);
    });
  });
  assert.deepEqual(stream.written, []);
  assert.deepEqual(lines, ['[server] fake connection accepted']);
});

test('a request that arrives as the server stops is told so', async () => {
  // The request is queued after the shutdown: the serving loop stops first
  // and the request is answered without touching the store.
  let release;
  const held = new Promise((resolve) => {
    release = resolve;
  });
  const stream = new FakeStream({ input: [] });
  stream.input.push(held);
  const original = stream.read.bind(stream);
  stream.read = async () => {
    const next = stream.input[0];
    if (next === held) {
      stream.input.shift();
      return held;
    }
    return original();
  };
  const listener = new FakeListener([() => stream]);
  const server = LinksServer.fromListener(listener);
  const serving = server.serve(LinkStorage.inMemory());
  await until(() => listener.script.length === 0 && stream.input.length === 0);
  server.shutdownHandle().shutdown();
  release(Buffer.from('() ((1 1))\n.\n'));
  await serving;
  await until(() => stream.written.length === 1);
  assert.deepEqual(stream.written, ["(error: 'server is shutting down')\n.\n"]);
  server.disconnectAll();
});
