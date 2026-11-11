// Unit tests of the reliable datagram session on recording links, and of the
// `udp` kind against a JS listener (test/support/ipc-listener.mjs). They need
// no `clink` binary.
import assert from 'node:assert/strict';
import dgram from 'node:dgram';
import dns from 'node:dns/promises';
import { test } from 'node:test';
import {
  ABORT,
  ACK,
  DATA,
  DatagramSession,
  EOF,
  PING,
  WELCOME,
  connectUdp,
  display,
  frame,
  parseFrame,
  resolveAddresses,
} from '../src/ipc-datagram.js';
import { connectStream, parseEndpoint } from '../src/ipc.js';
import {
  collect,
  echo,
  payload,
  rejectsWith,
  sleep,
  udpListener,
} from './support/ipc-listener.mjs';

/** A session whose link records the frames it sends. */
const recorded = (settings = {}, link = {}) => {
  const frames = [];
  const session = new DatagramSession(
    'test',
    { send: (bytes) => frames.push(parseFrame(Buffer.from(bytes))), close() {}, ...link },
    { chunk: 16, rto: 5, ...settings },
  );
  session.on('error', () => {});
  return { session, frames, types: () => frames.map((entry) => entry.type) };
};

const closed = (session) =>
  new Promise((resolve) => {
    if (session.closed) resolve();
    else session.once('close', resolve);
  });

const written = (stream, bytes) =>
  new Promise((resolve, reject) => {
    stream.write(bytes, (error) => (error ? reject(error) : resolve()));
  });

const ended = (stream) =>
  new Promise((resolve, reject) => stream.end((error) => (error ? reject(error) : resolve())));

/** Waits until the session has sent `count` frames of `type`. */
const sentOf = async (frames, type, count = 1) => {
  while (frames.filter((entry) => entry.type === type).length < count) await sleep(1);
};

test('a session accepts frames in order and acknowledges repeats', async () => {
  const { session, frames } = recorded();
  const reading = collect(session);
  session.receive(Buffer.alloc(3));
  assert.equal(frames.length, 0, 'a short frame is ignored');
  session.receive(frame(DATA, 1, Buffer.from('ab')));
  session.receive(frame(DATA, 1, Buffer.from('ab')));
  session.receive(frame(DATA, 5, Buffer.from('later')));
  session.receive(frame(DATA, 2, payload(17)));
  session.receive(frame(DATA, 2));
  session.receive(frame(DATA, 3, Buffer.from('c')));
  session.receive(frame(EOF, 4));
  session.receive(frame(DATA, 5, Buffer.from('after the end')));
  session.receive(frame(ACK, 0xfffffff0));
  session.receive(frame(PING, 0));
  assert.equal((await reading).toString(), 'abc');
  assert.deepEqual(
    frames.map(({ type, seq }) => [type, seq]),
    [
      [ACK, 1],
      [ACK, 1],
      [ACK, 2],
      [ACK, 3],
      [ACK, 4],
    ],
  );
  session.destroy();
  await closed(session);
});

test('a session survives a link that fails to send', async () => {
  const { session } = recorded(
    {},
    {
      send() {
        throw new Error('no route');
      },
      close() {
        throw new Error('already closed');
      },
    },
  );
  session.receive(frame(DATA, 1, Buffer.from('x')));
  session.destroy();
  await closed(session);
  session.receive(frame(DATA, 2, Buffer.from('y')));
});

test('a peer abort resets the reader once', async () => {
  const { session, types } = recorded();
  const reading = collect(session);
  session.receive(frame(ABORT, 0));
  session.receive(frame(ABORT, 0));
  await assert.rejects(reading, rejectsWith('ECONNRESET'));
  await closed(session);
  assert.ok(!types().includes(ABORT), 'no abort answers an abort');
});

test('a silent or dead peer hangs the session up', async () => {
  const silent = recorded({ heartbeat: 4, dead: 60 });
  await assert.rejects(collect(silent.session), rejectsWith('ECONNRESET'));
  assert.ok(silent.types().includes(PING), 'heartbeats while waiting');

  let alive = true;
  const watched = recorded({}, { peerAlive: () => alive });
  const reading = collect(watched.session);
  await sleep(120);
  alive = false;
  await assert.rejects(reading, rejectsWith('ECONNRESET'));
});

test('a write fails once the peer hangs up, and the end still completes', async () => {
  const { session, frames } = recorded();
  const writing = written(session, Buffer.from('lost'));
  await sentOf(frames, DATA);
  session.receive(frame(EOF, 1));
  session.receive(frame(ABORT, 0));
  await assert.rejects(writing, rejectsWith('EPIPE', /no longer reads/));

  const ending = recorded();
  ending.session.receive(frame(EOF, 1));
  const finishing = ended(ending.session);
  await sentOf(ending.frames, EOF);
  ending.session.receive(frame(ABORT, 0));
  await finishing;
  ending.session.destroy();
  await closed(ending.session);
  assert.ok(!ending.types().includes(ABORT), 'a peer that hung up gets no abort');
});

test('a write stops between chunks once the session is closed or aborted', async () => {
  const { session, frames } = recorded();
  const writing = written(session, payload(32));
  await sentOf(frames, DATA);
  session.receive(frame(ACK, 1));
  session.destroy();
  await assert.rejects(writing, rejectsWith('EPIPE', /outgoing direction was closed/));

  const aborted = recorded();
  aborted.session.receive(frame(EOF, 1));
  const stopped = written(aborted.session, payload(32));
  await sentOf(aborted.frames, DATA);
  aborted.session.receive(frame(ACK, 1));
  aborted.session.receive(frame(ABORT, 0));
  await assert.rejects(stopped, rejectsWith('EPIPE', /no longer reads/));

  // A peer that hung up after its end needs no end of ours.
  const hungUp = recorded();
  hungUp.session.receive(frame(EOF, 1));
  hungUp.session.receive(frame(ABORT, 0));
  await ended(hungUp.session);
  await sleep(150);
  assert.deepEqual(hungUp.types(), [ACK], 'nor any heartbeat');
  hungUp.session.destroy();
});

test('hanging up fails a pending write and lingers for the end', async () => {
  const { session, frames, types } = recorded();
  const writing = written(session, Buffer.from('unacknowledged'));
  await sentOf(frames, DATA);
  const started = Date.now();
  session.destroy();
  await assert.rejects(writing, rejectsWith('EPIPE', /connection was closed/));
  await closed(session);
  assert.ok(Date.now() - started >= 900, 'the end waits up to a second for its acknowledgement');
  assert.ok(types().filter((type) => type === EOF).length > 2, 'the end is sent again');
  assert.deepEqual(types().slice(-3), [ABORT, ABORT, ABORT]);
});

test('socket addresses resolve as the Rust reference does', async (t) => {
  for (const bad of ['nocolon', 'host:', 'host:12a', 'host:65536']) {
    await assert.rejects(resolveAddresses(bad), rejectsWith('EINVAL', /invalid socket address/));
  }
  assert.deepEqual(await resolveAddresses('127.0.0.1:5'), [
    { host: '127.0.0.1', port: 5, family: 4 },
  ]);
  assert.deepEqual(await resolveAddresses('[::1]:6'), [{ host: '::1', port: 6, family: 6 }]);
  const local = await resolveAddresses('localhost:7');
  assert.ok(local.length > 0 && local.every((entry) => entry.port === 7));
  assert.equal(display('::1', 8), '[::1]:8');
  assert.equal(display('127.0.0.1', 8), '127.0.0.1:8');
  const lookup = t.mock.method(dns, 'lookup', async () => {
    throw Object.assign(new Error('no such host'), { code: 'ENOTFOUND' });
  });
  await assert.rejects(resolveAddresses('nowhere.invalid:1'), rejectsWith('EINVAL', /no address/));
  lookup.mock.mockImplementation(async () => []);
  await assert.rejects(resolveAddresses('nowhere.invalid:1'), rejectsWith('EINVAL', /no address/));
});

/** Patches the next socket `connectUdp` creates with `patch(socket)`. */
const nextSocket = (t, patch) => {
  const original = dgram.createSocket;
  const created = [];
  const mock = t.mock.method(dgram, 'createSocket', (...args) => {
    const socket = original(...args);
    if (created.length === 0) patch(socket);
    created.push(socket);
    return socket;
  });
  return { created, restore: () => mock.mock.restore() };
};

const failure = (message, code) => Object.assign(new Error(message), { code });

for (const host of ['127.0.0.1', '::1']) {
  test(`udp: a session runs on the WELCOME's source (${host})`, async (t) => {
    const listener = await udpListener({ host, early: [frame(PING, 0)], onStream: echo });
    t.after(() => listener.close());
    const address = host.includes(':') ? `[${host}]` : host;
    const stream = await connectStream(`udp:${address}:${listener.port}?rto=5&chunk=64`);
    assert.equal(stream.chunk, 64);
    const sent = payload(3_000);
    const received = collect(stream);
    stream.end(sent);
    assert.deepEqual(await received, sent);
    stream.destroy();
    await closed(stream);
  });
}

test('udp: frames that arrive before the session are kept for it', async (t) => {
  const listener = await udpListener({ early: [frame(DATA, 1, Buffer.from('early'))] });
  t.after(() => listener.close());
  const foreign = dgram.createSocket('udp4');
  t.after(() => foreign.close());
  const patched = nextSocket(t, (socket) => {
    const connect = socket.connect.bind(socket);
    socket.connect = (...args) => {
      foreign.send(frame(DATA, 1, Buffer.from('foreign')), socket.address().port, '127.0.0.1');
      setTimeout(() => connect(...args), 50);
    };
  });
  const stream = await connectStream(`udp:127.0.0.1:${listener.port}`);
  patched.restore();
  const [chunk] = await new Promise((resolve) => stream.once('data', (...args) => resolve(args)));
  assert.equal(chunk.toString(), 'early');
  stream.destroy();
  await closed(stream);
});

test('udp: frames other than its WELCOME are ignored', async (t) => {
  const listener = await udpListener({
    chunk: 40,
    welcome(own, reply) {
      own.send(Buffer.alloc(3));
      own.send(frame(DATA, 1, reply));
      own.send(frame(WELCOME, 0, reply.subarray(0, 19)));
      const foreign = Buffer.from(reply);
      foreign[0] ^= 0xff;
      own.send(frame(WELCOME, 0, foreign));
    },
  });
  t.after(() => listener.close());
  const stream = await connectStream(`udp:127.0.0.1:${listener.port}`);
  assert.equal(stream.chunk, 40);
  stream.destroy();
  await closed(stream);
});

test('udp: a silent listener times out, and an abort stops the handshake', async (t) => {
  let hellos = 0;
  const listener = await udpListener({
    welcome: () => {
      hellos += 1;
      return false;
    },
  });
  t.after(() => listener.close());
  const silent = dgram.createSocket('udp4');
  let resent = 0;
  silent.on('message', () => {
    resent += 1;
  });
  await new Promise((resolve) => silent.bind(0, '127.0.0.1', resolve));
  t.after(() => silent.close());
  await assert.rejects(
    connectStream(`udp:127.0.0.1:${silent.address().port}?timeout=250`),
    rejectsWith('ETIMEDOUT', /no `udp` listener answered/),
  );
  assert.ok(resent >= 2, 'the HELLO is sent again');
  const controller = new AbortController();
  const reason = new Error('give up');
  setTimeout(() => controller.abort(reason), 30);
  await assert.rejects(
    connectUdp(parseEndpoint(`udp:127.0.0.1:${listener.port}`), { signal: controller.signal }),
    (error) => error === reason,
  );
  assert.equal(hellos, 1);
});

test('udp: a port no HELLO can reach fails at once and leaves no resend behind', async () => {
  await assert.rejects(connectStream('udp:127.0.0.1:0?timeout=300'), {
    code: 'ERR_SOCKET_BAD_PORT',
  });
  // Before the fix the resend interval threw ERR_SOCKET_BAD_PORT here, uncaught.
  await sleep(250);
});

test('udp: socket failures during the handshake are reported', async (t) => {
  let patched = nextSocket(t, (socket) => {
    const close = socket.close.bind(socket);
    socket.bind = () => setImmediate(() => socket.emit('error', failure('no port', 'EADDRINUSE')));
    socket.close = () => {
      close();
      throw new Error('not bound');
    };
  });
  await assert.rejects(connectStream('udp:127.0.0.1:9'), /no port/);
  patched.restore();

  patched = nextSocket(t, (socket) => {
    socket.send = (...args) => args.at(-1)(failure('unreachable', 'ENETUNREACH'));
  });
  await assert.rejects(connectStream('udp:127.0.0.1:9'), /unreachable/);
  patched.restore();

  patched = nextSocket(t, (socket) => {
    setTimeout(() => {
      socket.emit('error', failure('refused', 'ECONNREFUSED'));
      socket.emit('error', failure('denied', 'EACCES'));
    }, 30);
  });
  await assert.rejects(connectStream('udp:127.0.0.1:9'), /denied/);
  patched.restore();

  const listener = await udpListener();
  t.after(() => listener.close());
  patched = nextSocket(t, (socket) => {
    socket.connect = () => setImmediate(() => socket.emit('error', failure('gone', 'EINVAL')));
  });
  await assert.rejects(connectStream(`udp:127.0.0.1:${listener.port}`), /gone/);
  patched.restore();
});

test('udp: receive errors of a connected session are only traced', async (t) => {
  const listener = await udpListener({ onStream: echo });
  t.after(() => listener.close());
  const patched = nextSocket(t, () => {});
  const stream = await connectStream(`udp:127.0.0.1:${listener.port}?rto=5`);
  patched.restore();
  const [socket] = patched.created;
  socket.emit('error', failure('refused', 'ECONNREFUSED'));
  socket.emit('error', failure('odd', 'EIO'));
  const received = collect(stream);
  stream.end('still there');
  assert.equal((await received).toString(), 'still there');
  stream.destroy();
  await closed(stream);
});
