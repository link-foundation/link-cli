// Unit tests of the connectors of src/ipc.js that need no `clink` binary:
// TCP, `unix:`, a Windows named pipe (its path is a plain socket file here),
// `exec:`, the bridge (a fake `clink` script), custom kinds, the transport,
// and the endpoint parsing and trace corners. They run on any POSIX system.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import dns from 'node:dns/promises';
import net from 'node:net';
import path from 'node:path';
import { PassThrough } from 'node:stream';
import { test } from 'node:test';
import { AbortError, ConnectionClosedError, ProtocolMismatchError } from '../src/errors.js';
import { encodeMessage, MessageReader } from '../src/framing.js';
import { commandLine, Endpoint, optionsOf } from '../src/ipc-endpoint.js';
import { trace } from '../src/ipc-trace.js';
import {
  bridgeableKinds,
  bridgeCommand,
  connectIpc,
  connectStream,
  IpcError,
  IpcTransport,
  nativeConnect,
  parseEndpoint,
  pipeName,
  registerTransport,
  setTrace,
  supportsNatively,
} from '../src/ipc.js';
import { collect, rejectsWith, sleep, tempDir } from './support/ipc-listener.mjs';

const POSIX = process.platform !== 'win32';
const LINUX = process.platform === 'linux';

/** Turns the `[ipc]` log on for the test and returns the lines it writes. */
const traced = (t) => {
  const lines = [];
  t.mock.method(process.stderr, 'write', (text) => {
    lines.push(String(text));
    return true;
  });
  setTrace(true);
  t.after(() => setTrace(false));
  return lines;
};

/** Shortens the one timer of `ms` the code under test arms to a few milliseconds. */
const shortenTimer = (t, ms) => {
  const original = globalThis.setTimeout;
  t.mock.method(globalThis, 'setTimeout', (callback, delay, ...rest) =>
    original(callback, delay === ms ? 20 : delay, ...rest),
  );
};

/** A server on `where` that echoes, or hands each socket to `onSocket`. */
const server = async (t, where, onSocket = (socket) => socket.pipe(socket)) => {
  const sockets = new Set();
  const created = net.createServer({ allowHalfOpen: true }, (socket) => {
    sockets.add(socket);
    socket.on('error', () => {});
    socket.on('end', () => socket.end());
    onSocket(socket);
  });
  await new Promise((resolve) => created.listen(where, resolve));
  t.after(() => {
    for (const socket of sockets) socket.destroy();
    return new Promise((resolve) => created.close(resolve));
  });
  return created;
};

const freePort = async () => {
  const probe = net.createServer();
  await new Promise((resolve) => probe.listen(0, '127.0.0.1', resolve));
  const { port } = probe.address();
  await new Promise((resolve) => probe.close(resolve));
  return port;
};

const echoed = async (stream, text = 'hello') => {
  const received = collect(stream);
  stream.end(text);
  assert.equal((await received).toString(), text);
};

test('the bridge kinds of every platform', () => {
  const kinds = (platform, arch) => [...bridgeableKinds(platform, arch)].slice(9);
  assert.deepEqual(kinds('linux'), ['fifo', 'sysvmq', 'mq']);
  assert.deepEqual(kinds('darwin'), ['fifo', 'sysvmq']);
  assert.deepEqual(kinds('freebsd'), ['fifo']);
  assert.deepEqual(kinds('win32', 'x64'), ['mailslot', 'copydata', 'alpc']);
  assert.deepEqual(kinds('win32', 'ia32'), ['mailslot', 'copydata']);
  // Kinds are matched in ASCII lowercase.
  assert.equal(supportsNatively('TCP', 'linux'), true);
  assert.equal(supportsNatively('MailSlot', 'linux'), false);
  assert.equal(bridgeCommand({ clink: '/opt/clink' }), '/opt/clink');
  const saved = process.env.CLINK_BIN;
  try {
    process.env.CLINK_BIN = '/env/clink';
    assert.equal(bridgeCommand(), '/env/clink');
    delete process.env.CLINK_BIN;
    assert.equal(bridgeCommand(), 'clink');
  } finally {
    if (saved !== undefined) process.env.CLINK_BIN = saved;
  }
});

test('endpoint corners: options, numbers, commands and kinds', () => {
  for (const kind of ['sysvmq', 'mailslot', 'copydata', 'alpc']) {
    assert.deepEqual(optionsOf(kind), ['timeout', 'poll', 'accept-poll', 'chunk', 'rto']);
  }
  assert.deepEqual(optionsOf('pipe', 'win32'), ['timeout']);
  registerTransport('unit-options', () => null);
  assert.equal(parseEndpoint('unit-options:x?any=1').options.any, '1');
  assert.equal(parseEndpoint('fifo:/x?poll=1&&timeout=2').toString(), 'fifo:/x?poll=1&timeout=2');
  assert.throws(
    () => parseEndpoint('fifo:/x?capacity=64'),
    rejectsWith('EINVAL', /no option `capacity` \(options: timeout, poll, accept-poll\)/),
  );
  assert.throws(() => parseEndpoint('fifo:/x?timeout=18446744073709551616'), rejectsWith('EINVAL'));
  assert.equal(new Endpoint('fifo', '/x', [['timeout', 'soon']]).timeout(), 10_000);
  const endpoint = parseEndpoint('fifo:/x');
  assert.equal(parseEndpoint(endpoint), endpoint);
  assert.throws(() => parseEndpoint(42), TypeError);
  assert.throws(() => commandLine('[bad'), rejectsWith('EINVAL', /JSON array of strings: /));
  assert.throws(() => commandLine('  '), rejectsWith('EINVAL', /needs a command/));
  assert.throws(() => commandLine('[""]'), rejectsWith('EINVAL', /needs a command/));
  assert.throws(() => registerTransport(5, () => {}), TypeError);
  assert.equal(pipeName('\\\\host\\pipe\\name'), '\\\\host\\pipe\\name');
  assert.equal(pipeName('name'), '\\\\.\\pipe\\name');
  assert.throws(() => pipeName(''), rejectsWith('EINVAL', /needs a name/));
});

test('the [ipc] log goes to standard error when on', (t) => {
  const lines = traced(t);
  trace('one');
  setTrace(false);
  trace('two');
  assert.deepEqual(lines, ['[ipc] one\n']);
});

test('tcp: connects to the first address that answers', async (t) => {
  const listening = await server(t, { port: 0, host: '127.0.0.1' });
  const { port } = listening.address();
  await echoed(await connectStream(`tcp:127.0.0.1:${port}?timeout=2000`));
  t.mock.method(dns, 'lookup', async () => [
    { address: '::1', family: 6 },
    { address: '127.0.0.1', family: 4 },
  ]);
  await echoed(await connectStream(`tcp:two.test:${port}`));
  await assert.rejects(connectStream(`tcp:127.0.0.1:${await freePort()}`), {
    code: 'ECONNREFUSED',
  });
});

test('tcp: a connect that hangs times out or is aborted', async (t) => {
  t.mock.method(net, 'connect', () => new net.Socket());
  await assert.rejects(connectStream('tcp:127.0.0.1:1?timeout=30'), rejectsWith('ETIMEDOUT'));
  const controller = new AbortController();
  setTimeout(() => controller.abort(new Error('enough')), 30);
  await assert.rejects(connectStream('tcp:127.0.0.1:1', { signal: controller.signal }), AbortError);
});

test('unix: connects to a socket file, and to an abstract name on Linux', async (t) => {
  const file = path.join(tempDir(t), 'links.sock');
  await server(t, file);
  const stream = await connectStream(`unix:${file}`);
  await echoed(stream);
  const elsewhere = nativeConnect('unix', 'darwin');
  await assert.rejects(
    elsewhere(parseEndpoint('unix:@name'), {}),
    rejectsWith('ENOTSUP', /only on Linux/),
  );
  if (!LINUX) return;
  const name = `link-cli-unit-${process.pid}`;
  await server(t, `\0${name}`);
  await echoed(await connectStream(`unix:@${name}`));
});

test('pipe: a Windows named pipe cannot half-close', { skip: !POSIX }, async (t) => {
  // On POSIX the pipe path is a relative socket file name.
  const dir = tempDir(t);
  const cwd = process.cwd();
  process.chdir(dir);
  t.after(() => process.chdir(cwd));
  let received;
  await server(t, pipeName('links'), (socket) => {
    received = collect(socket);
  });
  const connect = nativeConnect('pipe', 'win32');
  const stream = await connect(parseEndpoint('pipe:links'), {});
  const closed = new Promise((resolve) => stream.once('close', resolve));
  stream.end('bye');
  await closed;
  assert.equal((await received).toString(), 'bye');
  await assert.rejects(
    connect(parseEndpoint('pipe:nobody'), {}),
    rejectsWith('ECONNREFUSED', /nobody listens on `\\\\\.\\pipe\\nobody`/),
  );
  fs.writeFileSync(pipeName('plain'), '');
  await assert.rejects(connect(parseEndpoint('pipe:plain'), {}), (error) => {
    assert.ok(!(error instanceof IpcError));
    assert.equal(error.code, 'ECONNREFUSED');
    return true;
  });
});

const node = (...args) => `exec:${JSON.stringify([process.execPath, ...args])}`;

test('exec: speaks to a child and reaps it', { skip: !POSIX }, async (t) => {
  const lines = traced(t);
  const stream = await connectStream(node('-e', 'process.stdin.pipe(process.stdout)'));
  assert.ok(stream.pid > 0);
  await echoed(stream);
  await new Promise((resolve) => stream.destroy().once('close', resolve));
  const quick = await connectStream(node('-e', ''));
  await collect(quick);
  await sleep(100);
  await new Promise((resolve) => quick.destroy().once('close', resolve));
  // The second child had exited before it was released.
  assert.equal(lines.filter((line) => /exited: exit status: 0/.test(line)).length, 1);
});

test('exec: a child that stays is killed after 5 s', { skip: !POSIX }, async (t) => {
  const lines = traced(t);
  shortenTimer(t, 5000);
  const stream = await connectStream(node('-e', 'setInterval(() => {}, 1000)'));
  await new Promise((resolve) => stream.destroy().once('close', resolve));
  assert.ok(lines.some((line) => /killed after 5 s/.test(line)));
  assert.ok(lines.some((line) => /exited: signal: SIGKILL/.test(line)));
});

test('exec: a command that cannot start, or a connect given up', async () => {
  await assert.rejects(
    connectStream('exec:/nonexistent/program --flag'),
    rejectsWith('ENOENT', /cannot start `\/nonexistent\/program`/),
  );
  const controller = new AbortController();
  const reason = new Error('stop');
  const connecting = nativeConnect('exec')(parseEndpoint(node('-e', '')), {
    signal: controller.signal,
  });
  controller.abort(reason);
  await assert.rejects(connecting, (error) => error === reason);
});

/** A fake `clink` whose behaviour `FAKE_CLINK` picks. */
const FAKE = `#!${process.execPath}
const say = (text) => process.stderr.write(text);
const stay = () => setInterval(() => {}, 1000);
switch (process.env.FAKE_CLINK) {
  case 'connect':
    say('clink server listening on stdio\\n[ipc] conn');
    setTimeout(() => {
      say(\`ected \${process.argv.slice(2).join(' ')}\\r\\nnot a log line\\n\`);
      process.stdin.pipe(process.stdout);
    }, 20);
    break;
  case 'refuse':
    say('clink: cannot forward to sysvmq:/q: no such queue\\n');
    stay();
    break;
  case 'refuse-bare':
    say('clink: cannot forward to sysvmq:/q: \\n');
    stay();
    break;
  case 'fail':
    say('clink server listening on stdio\\nclink: opening failed\\nclink server listening on x\\n');
    process.exit(3);
  case 'quiet':
    say('clink server listening on stdio\\n');
    process.exit(1);
  default:
    stay();
}
`;

const fakeClink = (t, mode) => {
  const file = path.join(tempDir(t), 'clink');
  fs.writeFileSync(file, FAKE, { mode: 0o755 });
  const saved = process.env.FAKE_CLINK;
  process.env.FAKE_CLINK = mode;
  t.after(() => {
    if (saved === undefined) delete process.env.FAKE_CLINK;
    else process.env.FAKE_CLINK = saved;
  });
  return file;
};

test('bridge: connects through clink and passes its log on', { skip: !POSIX }, async (t) => {
  const clink = fakeClink(t, 'connect');
  const lines = traced(t);
  const stream = await connectStream('sysvmq:/q?rto=5', { clink });
  assert.equal(stream.bridged, true);
  assert.ok(stream.pid > 0);
  await echoed(stream, 'through the bridge');
  await new Promise((resolve) => stream.destroy().once('close', resolve));
  assert.ok(lines.includes('[ipc] bridge: connected --serve stdio --forward sysvmq:/q?rto=5\n'));
  assert.ok(lines.includes('[ipc] bridge: not a log line\n'));
});

test('bridge: a refused or failed forward is reported', { skip: !POSIX }, async (t) => {
  const cases = {
    refuse: /failed: no such queue$/,
    'refuse-bare': /failed: sysvmq:\/q: $/,
    fail: /failed: clink: opening failed$/,
    quiet: /failed: it exited$/,
  };
  for (const [mode, pattern] of Object.entries(cases)) {
    const clink = fakeClink(t, mode);
    await assert.rejects(
      connectStream('sysvmq:/q', { clink }),
      rejectsWith('ECONNREFUSED', pattern),
      mode,
    );
  }
  await assert.rejects(
    connectStream('sysvmq:/q', { clink: '/nonexistent/clink' }),
    rejectsWith('ENOENT', /cannot start the bridge `\/nonexistent\/clink` for `sysvmq:`/),
  );
});

test(
  'bridge: a bridge that never connects times out or is aborted',
  { skip: !POSIX },
  async (t) => {
    const clink = fakeClink(t, 'hang');
    shortenTimer(t, 10_007);
    await assert.rejects(
      connectStream('sysvmq:/q?timeout=7', { clink }),
      rejectsWith('ETIMEDOUT', /did not connect in time/),
    );
    const controller = new AbortController();
    setTimeout(() => controller.abort(), 50);
    await assert.rejects(
      connectStream('sysvmq:/q', { clink, signal: controller.signal }),
      AbortError,
    );
  },
);

test('open: custom kinds, stdio and kinds without a connector', async (t) => {
  const stream = new PassThrough();
  registerTransport('unit-now', () => stream);
  assert.equal(await connectStream('unit-now:x'), stream);
  await assert.rejects(connectStream('stdio'), rejectsWith('ENOTSUP', /only be listened on/));
  if (!process.platform.startsWith('win')) {
    await assert.rejects(connectStream('mailslot:x'), rejectsWith('ENOTSUP', /`mailslot:`/));
  }
  await assert.rejects(connectStream('unit-now:x', { signal: AbortSignal.abort() }), AbortError);
  // A stream that opens after the connect gave up is closed again.
  const late = [
    new PassThrough(),
    {
      destroy: () => {
        throw new Error('gone');
      },
    },
    null,
  ];
  let index = 0;
  registerTransport('unit-late', () => sleep(40).then(() => late[index]));
  for (index = 0; index < late.length; index += 1) {
    await assert.rejects(connectStream('unit-late:x', { timeout: 10 }), {
      name: 'DeadlineExceededError',
    });
    await sleep(60);
  }
  assert.ok(late[0].destroyed);
  t.after(() => registerTransport('unit-late', () => null));
});

/** A links server that answers every request with `reply` (raw bytes when a Buffer). */
const linksServer = (t, reply) =>
  server(t, { port: 0, host: '127.0.0.1' }, (socket) => {
    const reader = new MessageReader();
    socket.on('data', (chunk) => {
      const requests = reader.push(chunk).length;
      for (let i = 0; i < requests; i++) {
        socket.write(Buffer.isBuffer(reply) ? reply : encodeMessage(reply));
      }
    });
  });

test('connectIpc verifies the protocol and takes a TCP address', async (t) => {
  const good = (await linksServer(t, '(count: 0)')).address().port;
  for (const endpoint of [
    { port: good },
    { host: '127.0.0.1', port: good },
    parseEndpoint(`127.0.0.1:${good}`),
    `tcp:127.0.0.1:${good}`,
  ]) {
    const client = await connectIpc(endpoint);
    await client.close();
  }
  const bad = (await linksServer(t, Buffer.of(0x20, 0, 0, 0, 0xff, 10, 46, 10))).address().port;
  await assert.rejects(connectIpc(`127.0.0.1:${bad}`), ProtocolMismatchError);
  const unverified = await connectIpc(`127.0.0.1:${bad}`, { verify: false });
  await unverified.close();
  await assert.rejects(connectIpc({ host: '::1', port: await freePort() }), (error) => {
    assert.ok(error instanceof ConnectionClosedError);
    assert.match(error.message, /cannot connect to tcp:\[::1\]:\d+/);
    return true;
  });
  await assert.rejects(connectIpc(null), TypeError);
  await assert.rejects(
    IpcTransport.connect('tcp:127.0.0.1:1', { signal: AbortSignal.abort() }),
    AbortError,
  );
});

test('an IpcTransport on a stream it does not own only closes itself', async (t) => {
  const port = (await linksServer(t, '(count: 0)')).address().port;
  const ended = [];
  const listening = await server(t, { port: 0, host: '127.0.0.1' }, (socket) => socket.end());
  for (const [owned, kind] of [
    [false, 'test'],
    [true, undefined],
  ]) {
    const socket = net.connect({
      port: listening.address().port,
      host: '127.0.0.1',
      allowHalfOpen: true,
    });
    socket.on('error', () => {});
    const transport = new IpcTransport(socket, { owned, kind });
    assert.equal(transport.kind, kind ?? 'ipc');
    await new Promise((resolve) => socket.once('end', resolve));
    await sleep(10);
    ended.push([owned ? 'owned' : transport.closed, socket.writableEnded]);
    transport.destroy();
    socket.destroy();
  }
  assert.deepEqual(ended, [
    [true, false],
    ['owned', true],
  ]);
  const transport = await IpcTransport.connect(`127.0.0.1:${port}`);
  assert.equal(transport.endpoint, `tcp:127.0.0.1:${port}`);
  await transport.close();
});
