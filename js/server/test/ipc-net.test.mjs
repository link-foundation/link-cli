// Unit tests of ipc-net.js, the socket transports of
// rust/src/protocol/ipc/net.rs and the named pipes of
// rust/src/protocol/ipc/windows/mod.rs: Rust's `ToSocketAddrs` failures,
// trying every resolved address, the listener queue, stale and live Unix
// socket files, and the named pipe errors over an injected `net` stand-in.
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import * as fs from 'node:fs';
import * as net from 'node:net';
import * as os from 'node:os';
import * as path from 'node:path';
import { describe, it } from 'node:test';
import { Endpoint } from '../src/ipc-endpoint.js';
import {
  connectPipe,
  connectTcp,
  fromTcpSocket,
  connectUnix,
  display,
  listenPipe,
  listenTcp,
  listenUnix,
  lookupAll,
  pipeName,
  resolve,
  resolveAll,
  SocketListener,
  tcpStream,
} from '../src/ipc-net.js';
import { setTrace } from '../src/ipc-trace.js';
import { diagnostics } from '../src/protocol-errors.js';

const windows = process.platform === 'win32';
const failure = (code) => Object.assign(new Error(code), { code });

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

/** A `net.Socket` stand-in that connects, fails or never answers. */
class FakeSocket extends EventEmitter {
  destroyed = false;
  writableEnded = false;

  constructor(outcome) {
    super();
    if (outcome === 'connect') setImmediate(() => this.emit('connect'));
    else if (outcome !== 'hang') setImmediate(() => this.emit('error', outcome));
  }

  pause() {}

  resume() {}

  setNoDelay() {
    throw new Error('not a TCP socket');
  }

  end() {}

  destroy() {
    this.destroyed = true;
  }
}

/** A `net.Server` stand-in whose `listen` fails with `outcome` or succeeds. */
class FakeServer extends EventEmitter {
  closed = 0;

  constructor(outcome) {
    super();
    this.outcome = outcome;
  }

  listen(options, callback) {
    this.options = options;
    if (this.outcome === null) setImmediate(callback);
    else setImmediate(() => this.emit('error', this.outcome));
  }

  close(callback) {
    this.closed += 1;
    callback();
  }
}

const fakeNet = (outcome) => ({
  connected: [],
  connect(options) {
    this.connected.push(options);
    return new FakeSocket(outcome);
  },
  createServer() {
    this.server = new FakeServer(outcome === 'connect' ? null : outcome);
    return this.server;
  },
});

describe('address resolution', () => {
  it('resolves literals and names as Rust does', async () => {
    assert.equal(display('::1', 80), '[::1]:80');
    assert.equal(display('127.0.0.1', 80), '127.0.0.1:80');
    const found = await lookupAll('localhost');
    assert.ok(found.length > 0 && found.every(({ address }) => net.isIP(address) !== 0));
    await assert.rejects(lookupAll('no-such-host.invalid'));
    assert.deepEqual(await resolve('127.0.0.1:7'), [{ address: '127.0.0.1', port: 7, family: 4 }]);
    assert.deepEqual(await resolve('[::1]:7'), [{ address: '::1', port: 7, family: 6 }]);
    assert.deepEqual(
      await resolve('10.0.0.1:+8', async () => [{ address: '10.0.0.1', family: 4 }]),
      [{ address: '10.0.0.1', port: 8, family: 4 }],
    );
    const asked = [];
    const lookup = async (host) => {
      asked.push(host);
      return [{ address: '::1', family: 6 }];
    };
    assert.deepEqual(
      await resolve('[::1]:70000', lookup).catch((error) => error.message),
      'invalid port value',
    );
    assert.deepEqual(await resolve('[nope]:7', lookup), [{ address: '::1', port: 7, family: 6 }]);
    assert.deepEqual(await resolve('1.2.3:7', lookup), [{ address: '::1', port: 7, family: 6 }]);
    assert.deepEqual(asked, ['[nope]', '1.2.3']);
  });

  it('fails as getaddrinfo does', async () => {
    const cases = [
      ['no-port', null, 'invalid socket address'],
      ['host:http', null, 'invalid port value'],
      ['host:65536', null, 'invalid port value'],
      [':80', null, 'failed to lookup address information: Name or service not known'],
      [
        'host:80',
        failure('ENOTFOUND'),
        'failed to lookup address information: Name or service not known',
      ],
      [
        'host:80',
        failure('EAI_AGAIN'),
        'failed to lookup address information: Temporary failure in name resolution',
      ],
      ['host:80', [], '`host:80` resolves to no address'],
    ];
    for (const [text, outcome, message] of cases) {
      const lookup = async () => {
        if (outcome instanceof Error) throw outcome;
        return outcome;
      };
      await assert.rejects(resolve(text, lookup), { message }, text);
    }
    await assert.rejects(
      resolveAll('host:1', 'nothing', async () => []),
      {
        kind: 'InvalidInput',
        message: 'nothing',
      },
    );
  });
});

describe('TCP', { timeout: 30_000 }, () => {
  it('traces a socket that keeps Nagle on', async () => {
    const lines = await traced(async () => {
      const stream = tcpStream(new FakeSocket('hang'));
      assert.equal(stream.kind, 'tcp');
    });
    assert.deepEqual(lines, ["[ipc] could not disable Nagle's algorithm: not a TCP socket"]);
  });

  it('tries every address and reports the last failure', async () => {
    const closed = net.createServer();
    await new Promise((done) => closed.listen(0, '127.0.0.1', done));
    const { port } = closed.address();
    await new Promise((done) => closed.close(done));
    const twice = async () => [
      { address: '127.0.0.1', family: 4 },
      { address: '127.0.0.1', family: 4 },
    ];
    await assert.rejects(connectTcp(Endpoint.parse(`tcp:here:${port}`), twice), {
      code: 'ECONNREFUSED',
    });
    const api = fakeNet('hang');
    await assert.rejects(connectTcp(Endpoint.parse('tcp:here:9?timeout=20'), twice, api), {
      kind: 'TimedOut',
      message: 'connection timed out',
    });
    assert.equal(api.connected.length, 2);
  });

  it('binds the first address that accepts the bind', async () => {
    // 192.0.2.1 (TEST-NET-1) is no local address: its bind fails, with
    // `EADDRNOTAVAIL` (Deno's Node.js layer reports `EINVAL`).
    const foreign = { address: '192.0.2.1', family: 4 };
    await assert.rejects(
      listenTcp(Endpoint.parse('tcp:here:0'), async () => [foreign]),
      (error) => ['EADDRNOTAVAIL', 'EINVAL'].includes(error.code),
    );
    const listener = await listenTcp(Endpoint.parse('tcp:here:0'), async () => [
      foreign,
      { address: '127.0.0.1', family: 4 },
    ]);
    const { address, port } = listener.localAddr();
    assert.equal(address, '127.0.0.1');
    assert.deepEqual(listener.endpoints(), [`127.0.0.1:${port}`]);
    const client = await connectTcp(Endpoint.parse(`tcp:127.0.0.1:${port}`));
    let served;
    const lines = await traced(async () => {
      served = await listener.accept();
    });
    assert.match(lines[0], /^\[ipc\] tcp accepted 127\.0\.0\.1:[0-9]+$/);
    await served.write(Buffer.from('hi'));
    assert.equal((await client.read(2)).toString(), 'hi');
    client.shutdown();
    served.shutdown();
    listener.close();
    // An already connected socket wraps as it is.
    const socket = new FakeSocket('hang');
    assert.equal((await fromTcpSocket(socket)).transport.socket, socket);
  });
});

describe('SocketListener', () => {
  it('queues server errors and refuses connections after close', async () => {
    const server = new FakeServer(null);
    let cleaned = 0;
    const listener = new SocketListener(server, {
      endpoint: 'fake',
      accepted: (socket) => socket,
      cleanup: () => cleaned++,
    });
    assert.equal(listener.localAddr(), null);
    server.emit('error', failure('EMFILE'));
    await assert.rejects(listener.accept(), { code: 'EMFILE' });
    const queued = new FakeSocket('hang');
    server.emit('connection', queued);
    server.emit('error', failure('EMFILE'));
    assert.equal(await listener.accept(), queued);
    await assert.rejects(listener.accept(), { code: 'EMFILE' });
    const waiting = listener.accept();
    const first = new FakeSocket('hang');
    server.emit('connection', first);
    assert.equal(await waiting, first);
    const left = new FakeSocket('hang');
    server.emit('connection', left);
    assert.equal(listener.closed, false);
    listener.close();
    listener.close();
    assert.equal(listener.closed, true);
    assert.deepEqual([server.closed, cleaned, left.destroyed], [1, 1, true]);
    const late = new FakeSocket('hang');
    server.emit('connection', late);
    server.emit('error', failure('EMFILE'));
    assert.equal(late.destroyed, true);
    assert.equal(await listener.accept(), null);
  });
});

describe('Unix domain sockets', { skip: windows && 'Node.js has no AF_UNIX on Windows' }, () => {
  it('refuses a live socket file and removes a stale one', async () => {
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'ipc-net-'));
    const file = path.join(directory, 's');
    const endpoint = Endpoint.parse(`unix:${file}`);
    try {
      fs.writeFileSync(file, '');
      let listener;
      const lines = await traced(async () => {
        listener = await listenUnix(endpoint);
      });
      assert.ok(
        lines.includes(`[ipc] unix removes the stale socket \`${file}\``),
        lines.join('\n'),
      );
      await assert.rejects(listenUnix(endpoint), {
        kind: 'AddrInUse',
        message: `a server already listens on \`${file}\``,
      });
      // The probe of the refused listen is a connection of its own.
      const probe = await listener.accept();
      assert.equal((await probe.read(2)).length, 0);
      probe.shutdown();
      const client = await connectUnix(endpoint);
      const served = await listener.accept();
      await client.write(Buffer.from('hi'));
      assert.equal((await served.read(2)).toString(), 'hi');
      client.shutdown();
      served.shutdown();
      listener.close();
      assert.equal(fs.existsSync(file), false);
    } finally {
      fs.rmSync(directory, { recursive: true, force: true });
    }
  });

  it(
    'listens on an abstract name on Linux and Android',
    { skip: process.platform !== 'linux' && 'abstract sockets exist only on Linux' },
    async () => {
      const endpoint = Endpoint.parse(`unix:@ipc-net-${process.pid}`);
      const listener = await listenUnix(endpoint, 'android');
      assert.deepEqual(listener.endpoints(), [`unix:@ipc-net-${process.pid}`]);
      const client = await connectUnix(endpoint, 'linux');
      const served = await listener.accept();
      client.shutdown();
      served.shutdown();
      listener.close();
    },
  );

  it('keeps abstract names to Linux and reports Windows', async () => {
    for (const run of [listenUnix, connectUnix]) {
      await assert.rejects(run(Endpoint.parse('unix:links'), 'win32'), {
        kind: 'Unsupported',
        message:
          '`unix:` endpoints are not supported by the JavaScript server: Node.js has no API for `AF_UNIX` sockets on Windows',
      });
    }
    for (const run of [listenUnix, connectUnix]) {
      await assert.rejects(run(Endpoint.parse('unix:@links'), 'darwin'), {
        kind: 'Unsupported',
        message: 'abstract `unix:@NAME` sockets exist only on Linux',
      });
    }
  });
});

describe('Windows named pipes', () => {
  it('names pipes as Rust does', () => {
    assert.equal(pipeName('links'), '\\\\.\\pipe\\links');
    assert.equal(pipeName('\\\\server\\pipe\\x'), '\\\\server\\pipe\\x');
    assert.throws(() => pipeName(''), { kind: 'InvalidInput', message: '`pipe:` needs a name' });
  });

  it('connects, or reports nobody listening', async () => {
    const endpoint = Endpoint.parse('pipe:links');
    await assert.rejects(connectPipe(endpoint, fakeNet(failure('ENOENT'))), {
      kind: 'ConnectionRefused',
      message: 'nobody listens on `\\\\.\\pipe\\links`',
    });
    await assert.rejects(connectPipe(endpoint, fakeNet(failure('EACCES'))), { code: 'EACCES' });
    await assert.rejects(connectPipe(Endpoint.parse('pipe:links?timeout=10'), fakeNet('hang')), {
      kind: 'TimedOut',
    });
    let stream;
    const lines = await traced(async () => {
      const api = fakeNet('connect');
      stream = await connectPipe(endpoint, api);
      assert.equal(api.connected[0].path, '\\\\.\\pipe\\links');
    });
    assert.equal(stream.kind, 'pipe');
    assert.deepEqual(lines, ['[ipc] pipe connected to \\\\.\\pipe\\links']);
  });

  it('listens, or reports the name taken', async () => {
    const endpoint = Endpoint.parse('pipe:links');
    await assert.rejects(listenPipe(endpoint, fakeNet(failure('EADDRINUSE'))), {
      kind: 'AddrInUse',
      message: 'a server already listens on `\\\\.\\pipe\\links`',
    });
    await assert.rejects(listenPipe(endpoint, fakeNet(failure('EACCES'))), { code: 'EACCES' });
    const api = fakeNet('connect');
    const listener = await listenPipe(endpoint, api);
    assert.deepEqual(listener.endpoints(), ['pipe:links']);
    assert.equal(api.server.options.path, '\\\\.\\pipe\\links');
    let stream;
    const lines = await traced(async () => {
      api.server.emit('connection', new FakeSocket('hang'));
      stream = await listener.accept();
    });
    assert.equal(stream.kind, 'pipe');
    assert.deepEqual(lines, ['[ipc] pipe accepted on links']);
    listener.close();
  });
});
