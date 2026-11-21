// The transport tests of rust/src/protocol/ipc/tests.rs, run against the
// JavaScript ports: every kind echoes two consecutive and two concurrent
// connections, the unsupported kinds fail cleanly, listeners close, relays
// stack transports, `exec` runs a child and read timeouts expire. Payloads
// are smaller than Rust's to keep the CPU use modest.
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import * as net from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { PassThrough } from 'node:stream';
import { after, describe, it } from 'node:test';
import {
  connect,
  connectEndpoint,
  Endpoint,
  forward,
  IpcStream,
  KINDS,
  listen,
  listenAll,
  listenEndpoint,
  MultiListener,
  register,
  relay,
  supports,
} from '../src/ipc.js';
import { unregisterAll } from '../src/ipc-endpoint.js';
import { randomId } from '../src/ipc-util.js';
import { diagnostics } from '../src/protocol-errors.js';

const POSIX = process.platform !== 'win32';
const LINUX = process.platform === 'linux';

const directories = [];
after(() => {
  unregisterAll();
  for (const dir of directories) rmSync(dir, { recursive: true, force: true });
});

function tempDir() {
  const dir = mkdtempSync(join(tmpdir(), 'clink-ipc-'));
  directories.push(dir);
  return dir;
}

const rendezvous = (kind, options = '') => `${kind}:${join(tempDir(), 'links')}${options}`;

function payload(length, seed) {
  const bytes = Buffer.alloc(length);
  for (let index = 0; index < length; index += 1) bytes[index] = ((index * 31 + seed) >>> 0) % 251;
  return bytes;
}

async function readToEnd(stream) {
  const parts = [];
  for (;;) {
    const part = await stream.read();
    if (part.length === 0) return Buffer.concat(parts);
    parts.push(part);
  }
}

/** `read_exact`: the next `length` bytes of `stream`. */
async function readExact(stream, length) {
  const parts = [];
  let read = 0;
  while (read < length) {
    const part = await stream.read(length - read);
    assert.notEqual(part.length, 0, `the stream ended after ${read} of ${length} bytes`);
    parts.push(part);
    read += part.length;
  }
  return Buffer.concat(parts);
}

/**
 * How a test marks the end of a payload (`Framing` of rust/src/protocol/ipc/tests.rs):
 * the writer ends its outgoing direction, or an 8-byte little-endian length
 * comes first, for kinds that cannot half-close (a Windows named pipe closes
 * both directions at once).
 */
const HALF_CLOSE = 'half-close';
const LENGTH_PREFIX = 'length-prefix';

/** `serve`: echoes every connection until the listener closes; resolves to the count. */
async function echoServer(listener, framing = HALF_CLOSE) {
  let served = 0;
  const connections = [];
  for (;;) {
    const stream = await listener.accept();
    if (stream === null) break;
    served += 1;
    connections.push(
      (async () => {
        if (framing === LENGTH_PREFIX) {
          const length = Number((await readExact(stream, 8)).readBigUInt64LE());
          await stream.write(await readExact(stream, length));
          await stream.flush();
        } else {
          await stream.write(await readToEnd(stream));
          await stream.flush();
          await stream.closeWrite();
        }
        // Wait for the client to hang up.
        await stream.read(1).catch(() => {});
        stream.shutdown();
      })(),
    );
  }
  await Promise.all(connections);
  return served;
}

async function roundTrip(endpoint, length, seed, framing = HALF_CLOSE) {
  const client = await connect(endpoint);
  const sent = payload(length, seed);
  const writing = (async () => {
    if (framing === LENGTH_PREFIX) {
      const prefix = Buffer.alloc(8);
      prefix.writeBigUInt64LE(BigInt(sent.length));
      await client.write(prefix);
    }
    await client.write(sent);
    await client.flush();
    if (framing === HALF_CLOSE) await client.closeWrite();
  })();
  const echoed =
    framing === LENGTH_PREFIX ? await readExact(client, length) : await readToEnd(client);
  await writing;
  assert.equal(echoed.length, sent.length, `${endpoint}: echoed length`);
  assert.ok(echoed.equals(sent), `${endpoint}: echoed bytes differ`);
  client.shutdown();
}

async function checkKind(endpoint, length, framing = HALF_CLOSE) {
  const listener = await listen(endpoint);
  const address = listener.endpoint();
  const server = echoServer(listener, framing);
  await roundTrip(address, length, 1, framing);
  await roundTrip(address, 1, 2, framing);
  await Promise.all(
    [0, 1].map((index) => roundTrip(address, length / 2 + index, 3 + index, framing)),
  );
  listener.close();
  assert.equal(await server, 4);
}

describe('every kind echoes', { timeout: 120_000 }, () => {
  it('tcp', () => checkKind('tcp:127.0.0.1:0', 100_000));
  // The framing the Windows named pipe echoes with, checked on every platform.
  it('tcp with length prefixes', () => checkKind('tcp:127.0.0.1:0', 20_000, LENGTH_PREFIX));

  it('a bare address is tcp', async () => {
    const listener = await listen('127.0.0.1:0');
    assert.notEqual(listener.localAddr(), null);
    assert.equal(Endpoint.parse(listener.endpoint()).kind, 'tcp');
    listener.close();
  });

  it('udp', () => checkKind('udp:127.0.0.1:0', 20_000));
  it('file', () => checkKind(rendezvous('file', '?capacity=4096'), 50_000));
  it('mmap', () => checkKind(rendezvous('mmap', '?capacity=4096'), 50_000));
  it('shm', { skip: !LINUX && 'shm needs /dev/shm (Linux)' }, () =>
    checkKind(rendezvous('shm'), 50_000),
  );
  it('signal', { skip: !POSIX && 'signal needs POSIX signals' }, () =>
    checkKind(rendezvous('signal', '?capacity=4096'), 50_000),
  );
  it('unix', { skip: !POSIX && 'Node.js has no AF_UNIX on Windows' }, () =>
    checkKind(`unix:${join(tempDir(), 'links.sock')}`, 100_000),
  );
  it('abstract unix', { skip: !LINUX && 'abstract sockets exist only on Linux' }, () =>
    checkKind(`unix:@clink-test-${randomId()}`, 10_000),
  );
  it('fifo', { skip: !POSIX && 'Windows has no FIFOs' }, () =>
    checkKind(rendezvous('fifo'), 100_000),
  );
  it('pipe', () =>
    POSIX
      ? checkKind(rendezvous('pipe'), 50_000)
      : checkKind(`pipe:clink-test-${randomId()}`, 50_000, LENGTH_PREFIX));
});

describe('kinds', () => {
  it('every kind is reported', () => {
    for (const kind of KINDS) {
      const either = supports(kind, true) || supports(kind, false);
      const expected = {
        unix: POSIX,
        fifo: POSIX,
        signal: POSIX,
        shm: LINUX,
        mq: false,
        sysvmq: false,
        mailslot: false,
        copydata: false,
        alpc: false,
      }[kind];
      assert.equal(either, expected ?? true, kind);
    }
    assert.ok(supports('stdio', true) && !supports('stdio', false));
    assert.ok(supports('exec', false) && !supports('exec', true));
    assert.ok(supports('TCP', true));
    assert.ok(!supports('unknown', true));
    assert.ok(!supports('unix', true, 'win32') && !supports('shm', false, 'darwin'));
  });

  it('unsupported kinds fail cleanly', async () => {
    for (const kind of KINDS) {
      const endpoint = `${kind}:/nonexistent/clink`;
      if (!supports(kind, false) && kind !== 'stdio') {
        await assert.rejects(connect(endpoint), { kind: 'Unsupported' }, kind);
      }
      if (!supports(kind, true) && kind !== 'exec') {
        await assert.rejects(listen(endpoint), { kind: 'Unsupported' }, kind);
      }
    }
    await assert.rejects(connect('stdio'), {
      kind: 'Unsupported',
      message: '`stdio` can only be listened on; connect with `exec:COMMAND` instead',
    });
    await assert.rejects(listen('exec:cat'), {
      kind: 'Unsupported',
      message: '`exec` can only be connected to; listen on `stdio` in the started command',
    });
  });

  it('unsupported texts follow Rust where Rust lacks the kind too', async () => {
    const text = (what, kind) =>
      `${what} on \`${kind}:\` endpoints is not supported on this platform`;
    const cases = [
      ['mq', 'darwin', text('connecting', 'mq')],
      ['sysvmq', 'win32', text('connecting', 'sysvmq')],
      ['mailslot', 'linux', text('connecting', 'mailslot')],
      ['copydata', 'darwin', text('connecting', 'copydata')],
      ['fifo', 'win32', text('connecting', 'fifo')],
    ];
    for (const [kind, platform, message] of cases) {
      await assert.rejects(connect(`${kind}:x`, { platform }), { kind: 'Unsupported', message });
    }
    await assert.rejects(listen('alpc:x', { platform: 'win32', arch: 'ia32' }), {
      message: text('listening', 'alpc'),
    });
    const node = (kind, primitive) =>
      `\`${kind}:\` endpoints are not supported by the JavaScript server: Node.js has no API for ${primitive}`;
    const missing = [
      ['mq', 'linux', 'x64', 'POSIX message queues'],
      ['sysvmq', 'darwin', 'arm64', 'System V message queues'],
      ['sysvmq', 'linux', 'x64', 'System V message queues'],
      ['mailslot', 'win32', 'x64', 'mailslots'],
      ['copydata', 'win32', 'x64', '`WM_COPYDATA` window messages'],
      ['alpc', 'win32', 'arm64', 'ALPC ports'],
      ['unix', 'win32', 'x64', '`AF_UNIX` sockets on Windows'],
      ['shm', 'darwin', 'x64', 'POSIX shared memory (`shm_open`)'],
      ['shm', 'win32', 'x64', 'named file mappings'],
      ['signal', 'win32', 'x64', 'Windows named events'],
    ];
    for (const [kind, platform, arch, primitive] of missing) {
      const message = node(kind, primitive);
      await assert.rejects(connect(`${kind}:x`, { platform, arch }), { message }, kind);
      await assert.rejects(listen(`${kind}:x`, { platform, arch }), { message }, kind);
    }
  });

  it('a Windows pipe that nobody serves is refused', async () => {
    // On POSIX the injected platform still asks libuv for a named pipe path.
    const name = `clink-test-${randomId()}`;
    await assert.rejects(connect(`pipe:${name}?timeout=200`, { platform: 'win32' }), {
      kind: 'ConnectionRefused',
    });
  });

  it('a Windows pipe is served and reached through `node:net`', { skip: !POSIX }, async () => {
    // A stand-in `node:net` that serves the pipe's name on a POSIX socket.
    const socketPath = join(tempDir(), 'pipe.sock');
    const names = [];
    const redirect = (options) => {
      names.push(options.path);
      return { ...options, path: socketPath };
    };
    const api = {
      createServer: (options) => {
        const server = net.createServer(options);
        const listen = server.listen.bind(server);
        server.listen = (options, ...rest) => listen(redirect(options), ...rest);
        return server;
      },
      connect: (options) => net.connect(redirect(options)),
    };
    const options = { platform: 'win32', net: api };
    const listener = await listen('pipe:clink-test', options);
    const accepting = listener.accept();
    const client = await connect('pipe:clink-test', options);
    const server = await accepting;
    await client.write(Buffer.from('hi'));
    assert.equal((await server.read(2)).toString(), 'hi');
    assert.deepEqual(names, ['\\\\.\\pipe\\clink-test', '\\\\.\\pipe\\clink-test']);
    client.shutdown();
    server.shutdown();
    listener.close();
  });

  it('custom kinds can be registered', async () => {
    register('test-alias', (endpoint) => connect(`tcp:${endpoint.address}`), null);
    assert.ok(supports('test-alias', false));
    assert.ok(!supports('test-alias', true));
    assert.throws(() => register('tcp', null, null), { kind: 'AlreadyExists' });
    assert.throws(() => register('Bad Kind', null, null), { kind: 'InvalidInput' });
    await assert.rejects(listen('test-alias:x'), {
      kind: 'Unsupported',
      message: 'listening on `test-alias:` endpoints is not supported on this platform',
    });
    const listener = await listen('tcp:127.0.0.1:0');
    const server = echoServer(listener);
    await roundTrip(`test-alias:${listener.endpoint()}`, 1000, 5);
    listener.close();
    assert.equal(await server, 1);

    register('test-listen', null, (endpoint) => listen(`tcp:${endpoint.address}`));
    assert.ok(supports('test-listen', true) && !supports('test-listen', false));
    await assert.rejects(connect('test-listen:x'), {
      message: 'connecting on `test-listen:` endpoints is not supported on this platform',
    });
    const custom = await listenEndpoint(Endpoint.parse('test-listen:127.0.0.1:0'));
    assert.match(custom.endpoint(), /^127\.0\.0\.1:\d+$/);
    custom.close();
  });

  it('connecting without a listener fails quickly', { timeout: 30_000 }, async () => {
    await assert.rejects(connect(rendezvous('mmap', '?timeout=300')), (error) =>
      ['NotFound', 'ConnectionRefused', 'TimedOut'].includes(error.kind),
    );
  });
});

describe('listeners', { timeout: 60_000 }, () => {
  it('closing a listener ends accept', async () => {
    const listener = await listen(rendezvous('file'));
    const accepting = listener.accept();
    setTimeout(() => listener.close(), 50);
    assert.equal(await accepting, null);
  });

  it('listenAll accepts on every endpoint', async () => {
    const mmap = rendezvous('mmap');
    const listener = await listenAll(['tcp:127.0.0.1:0', mmap]);
    assert.ok(listener instanceof MultiListener);
    const { address, port } = listener.localAddr();
    assert.equal(listener.endpoints().length, 2);
    assert.equal(listener.endpoint(), listener.endpoints().join(' '));
    const server = echoServer(listener);
    await roundTrip(`${address}:${port}`, 5000, 7);
    await roundTrip(mmap, 5000, 8);
    listener.close();
    assert.equal(await server, 2);
  });

  it('listenAll of nothing, of one and of a failing endpoint', async () => {
    await assert.rejects(listenAll([]), {
      kind: 'InvalidInput',
      message: 'no endpoint to listen on',
    });
    const one = await listenAll(['tcp:127.0.0.1:0']);
    assert.ok(!(one instanceof MultiListener));
    const taken = `tcp:127.0.0.1:${one.localAddr().port}`;
    const first = rendezvous('file');
    await assert.rejects(listenAll([first, taken]), { code: 'EADDRINUSE' });
    // The listener opened before the failure was closed: the directory is free.
    (await listen(first)).close();
    one.close();
  });

  it('a MultiListener ends once every listener ended, and passes errors on', async () => {
    const fake = (results) => ({
      accept: async () => {
        const next = results.shift();
        if (next instanceof Error) throw next;
        return next;
      },
      endpoints: () => ['fake'],
      localAddr: () => null,
      close: () => {},
    });
    const stream = new IpcStream({ kind: 'fake', shutdown() {} });
    const multi = new MultiListener([fake([new Error('broken'), stream, null]), fake([null])]);
    assert.equal(multi.localAddr(), null);
    await assert.rejects(multi.accept(), /broken/);
    assert.equal(await multi.accept(), stream);
    assert.equal(await multi.accept(), null);
    assert.equal(await multi.accept(), null);
  });
});

describe('relays and forwarding', { timeout: 60_000 }, () => {
  it('relays stack transports', async () => {
    // client --mmap--> relay --udp--> relay --tcp--> echo server
    const tcp = await listen('tcp:127.0.0.1:0');
    const tcpEndpoint = Endpoint.parse(tcp.endpoint());
    const server = echoServer(tcp);
    const udp = await listen('udp:127.0.0.1:0');
    const udpRelay = relay(udp, tcpEndpoint);
    const mmap = rendezvous('mmap');
    const ring = await listen(mmap);
    const ringRelay = relay(ring, Endpoint.parse(udp.endpoint()));
    await roundTrip(mmap, 20_000, 9);
    ring.close();
    await ringRelay;
    udp.close();
    await udpRelay;
    tcp.close();
    assert.equal(await server, 1);
  });

  it('a relay reports a target it cannot reach and survives failing accepts', async () => {
    const lines = [];
    const stderr = diagnostics.stderr;
    diagnostics.stderr = (line) => lines.push(line);
    try {
      let shut = 0;
      const stream = new IpcStream({ kind: 'fake', shutdown: () => (shut += 1) });
      const results = [new Error('accept failed'), stream, null];
      const listener = {
        accept: async () => {
          const next = results.shift();
          if (next instanceof Error) throw next;
          return next;
        },
        endpoint: () => 'fake',
      };
      await relay(listener, Endpoint.parse('stdio'));
      assert.deepEqual(lines, [
        'clink: cannot forward to stdio: `stdio` can only be listened on; connect with `exec:COMMAND` instead',
      ]);
      assert.equal(shut, 1);
    } finally {
      diagnostics.stderr = stderr;
    }
  });

  it('a relay whose forwarding fails goes on', async () => {
    const tcp = await listen('tcp:127.0.0.1:0');
    const accepting = tcp.accept();
    const failing = {
      kind: 'fake',
      read: async () => {
        throw new Error('read failed');
      },
      closeWrite: async () => {},
      shutdown() {},
    };
    const results = [new IpcStream(failing), null];
    await relay({ accept: async () => results.shift() }, Endpoint.parse(tcp.endpoint()));
    (await accepting).shutdown();
    tcp.close();
  });

  it('forward copies both ways and reports the first failure', async () => {
    const source = (chunks, onWrite = () => 1) => ({
      kind: 'mem',
      written: [],
      ended: false,
      read: async () => chunks.shift() ?? Buffer.alloc(0),
      async write(buffer) {
        this.written.push(Buffer.from(buffer));
        return onWrite(buffer) ?? buffer.length;
      },
      async closeWrite() {
        this.ended = true;
        throw new Error('ignored');
      },
      shutdown() {},
    });
    const a = source([Buffer.from('ab'), Buffer.from('c')], (buffer) => buffer.length);
    const b = source([Buffer.from('xyz')], (buffer) => buffer.length);
    assert.deepEqual(await forward(new IpcStream(a), new IpcStream(b)), [3, 3]);
    assert.equal(Buffer.concat(b.written).toString(), 'abc');
    assert.equal(Buffer.concat(a.written).toString(), 'xyz');
    assert.ok(a.ended && b.ended);

    const failingWrite = source([], () => {
      throw new Error('write failed');
    });
    await assert.rejects(
      forward(new IpcStream(failingWrite), new IpcStream(source([Buffer.from('q')]))),
      /write failed/,
    );
    const failingRead = { ...source([]), read: async () => Promise.reject(new Error('gone')) };
    await assert.rejects(forward(new IpcStream(failingRead), new IpcStream(source([]))), /gone/);
  });
});

describe('exec and stdio', { timeout: 30_000 }, () => {
  it('exec runs a child over anonymous pipes', { skip: !POSIX && 'needs cat' }, async () => {
    await roundTrip('exec:cat', 200_000, 10);
    await roundTrip('exec:["sh", "-c", "cat"]', 1000, 11);
  });

  it('stdio serves one connection over the given streams', async () => {
    const stdin = new PassThrough();
    const stdout = new PassThrough();
    let closed = 0;
    const listener = await listen('stdio', { io: { stdin, stdout, closeOutput: () => closed++ } });
    assert.equal(listener.endpoint(), 'stdio');
    const stream = await listener.accept();
    stdin.end('ping');
    assert.equal((await readToEnd(stream)).toString(), 'ping');
    await stream.write(Buffer.from('pong'));
    await stream.closeWrite();
    assert.equal(stdout.read().toString(), 'pong');
    assert.equal(closed, 1);
    const later = listener.accept();
    stream.shutdown();
    assert.equal(await later, null);
    listener.close();
  });

  it('a missing command fails to start', async () => {
    await assert.rejects(connect('exec:clink-no-such-command-anywhere'), {
      message: /^cannot start `clink-no-such-command-anywhere`: /,
    });
  });
});

describe('read timeouts', { timeout: 30_000 }, () => {
  it('read timeouts expire', async () => {
    const endpoint = rendezvous('mmap');
    const listener = await listen(endpoint);
    const accepting = listener.accept();
    const client = await connect(listener.endpoint());
    const server = await accepting;
    client.setReadTimeout(50);
    await assert.rejects(client.read(8), { kind: 'TimedOut', message: 'timed out' });
    server.shutdown();
    client.setReadTimeout(10_000);
    assert.equal((await client.read(8)).length, 0);
    client.shutdown();
    listener.close();
  });

  it(
    'a socket shut both ways still reads its end',
    { skip: !POSIX && 'Node.js has no AF_UNIX on Windows' },
    async () => {
      const listener = await listen(`unix:${join(tempDir(), 'links.sock')}`);
      const accepting = listener.accept();
      const client = await connectEndpoint(Endpoint.parse(listener.endpoint()));
      (await accepting).shutdown();
      await client.closeWrite();
      client.setReadTimeout(10_000);
      assert.equal((await client.read(8)).length, 0);
      client.setReadTimeout(50);
      assert.equal((await client.read(8)).length, 0);
      client.shutdown();
      listener.close();
    },
  );
});
