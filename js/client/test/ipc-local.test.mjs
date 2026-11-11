// Unit tests of the local IPC kinds against a JS listener of the rendezvous
// directory (test/support/ipc-listener.mjs): the rendezvous itself, `fifo`
// and POSIX `pipe`, the rings of `file`, `mmap`, `shm` and `signal`, and the
// joined stream under them. They need no `clink` binary.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { PassThrough, Writable } from 'node:stream';
import { test } from 'node:test';
import { JoinedStream } from '../src/ipc-joined.js';
import { PeerWatch, processAlive, Rendezvous } from '../src/ipc-rendezvous.js';
import { connectStream, parseEndpoint } from '../src/ipc.js';
import {
  Listener,
  collect,
  deadPid,
  echo,
  payload,
  rejectsWith,
  sleep,
  tempDir,
  writeAtomically,
  writeListener,
} from './support/ipc-listener.mjs';

const POSIX = process.platform !== 'win32';

test('processAlive and PeerWatch tell running processes from gone ones', async (t) => {
  for (const pid of [0, -1, 1.5, 0x80000000, '12']) assert.equal(processAlive(pid), false);
  assert.equal(processAlive(process.pid), true);
  const dead = deadPid();
  assert.equal(processAlive(dead), false);
  const kill = t.mock.method(process, 'kill', () => {
    throw Object.assign(new Error('not permitted'), { code: 'EPERM' });
  });
  assert.equal(processAlive(4242), true);
  kill.mock.restore();
  const watch = new PeerWatch(dead);
  assert.equal(watch.alive(), true);
  await sleep(120);
  assert.equal(watch.alive(), false);
  assert.equal(watch.alive(), false);
});

test('the rendezvous refuses a missing, foreign or gone listener', (t) => {
  const dir = tempDir(t);
  const endpoint = parseEndpoint(`fifo:${dir}`);
  const begin = () => Rendezvous.begin(endpoint, 'fifo');
  const file = path.join(dir, 'listener.json');
  assert.throws(begin, rejectsWith('ECONNREFUSED', /nobody listens/));
  for (const text of ['[]', 'null', '7']) {
    writeAtomically(file, text);
    assert.throws(begin, rejectsWith('ECONNREFUSED', /JSON object/));
  }
  writeListener(dir, { v: 2 });
  assert.throws(begin, rejectsWith('ECONNREFUSED', /another rendezvous version/));
  writeListener(dir, { kind: 'mmap' });
  assert.throws(begin, rejectsWith('ECONNREFUSED', /listens for `mmap:` connections/));
  writeAtomically(file, `{"pid":${process.pid},"v":1}`);
  assert.throws(begin, rejectsWith('ECONNREFUSED', /listens for `:` connections/));
  writeAtomically(file, '{"kind":"fifo","v":1}');
  assert.throws(begin, rejectsWith('ECONNREFUSED', /\(process 0\) is gone/));
  writeListener(dir, { pid: deadPid() });
  assert.throws(begin, rejectsWith('ECONNREFUSED', /is gone/));
  writeListener(dir);
  const rendezvous = Rendezvous.begin(parseEndpoint(`pipe:${dir}?poll=3`), 'pipe');
  assert.equal(rendezvous.poll, 3);
  assert.equal(rendezvous.serverPid, process.pid);
  assert.match(rendezvous.path('req'), /[0-9a-f]{20}\.req$/);
  assert.equal(rendezvous.peerOf({}), process.pid);
  assert.equal(rendezvous.peerOf({ pid: 7 }), 7);
});

test('a rendezvous request is answered, refused, aborted or timed out', async (t) => {
  const dir = tempDir(t);
  let answer = () => ({ pid: 77 });
  const listener = new Listener(dir, 'fifo', { answer: (...args) => answer(...args) });
  t.after(() => listener.close());
  const begin = (options = '') => Rendezvous.begin(parseEndpoint(`fifo:${dir}${options}`), 'fifo');

  const answered = await begin().request({ extra: true });
  assert.equal(answered.pid, 77);
  assert.deepEqual(Object.keys(listener.requests[0]), ['extra', 'kind', 'pid', 'v']);
  assert.equal(listener.requests[0].kind, 'fifo');

  answer = () => ({ error: 'too busy' });
  await assert.rejects(
    begin().request(),
    rejectsWith('ECONNREFUSED', /the listener refused: too busy/),
  );

  answer = () => null;
  const controller = new AbortController();
  const reason = new Error('stop');
  setTimeout(() => controller.abort(reason), 30);
  await assert.rejects(begin().request({}, controller.signal), (error) => error === reason);
  await assert.rejects(
    begin('?timeout=40&poll=0').request(),
    rejectsWith('ETIMEDOUT', /no answer from the listener/),
  );
  await sleep(10);
  assert.deepEqual(
    fs.readdirSync(dir).filter((name) => /\.(req|ack|tmp)$/.test(name)),
    [],
    'nothing is left behind',
  );
});

test('a rendezvous request notices the listener exit', async (t) => {
  const dir = tempDir(t);
  const gone = deadPid();
  writeListener(dir);
  const rendezvous = Rendezvous.begin(parseEndpoint(`fifo:${dir}?poll=20`), 'fifo');
  rendezvous.serverPid = gone;
  await assert.rejects(
    rendezvous.request(),
    rejectsWith('ECONNREFUSED', new RegExp(`process ${gone}\\) is gone`)),
  );
});

test('a request that cannot be renamed into place leaves nothing behind', async (t) => {
  const dir = tempDir(t);
  writeListener(dir);
  const rendezvous = Rendezvous.begin(parseEndpoint(`fifo:${dir}`), 'fifo');
  t.mock.method(fs, 'renameSync', () => {
    throw Object.assign(new Error('read-only'), { code: 'EROFS' });
  });
  await assert.rejects(rendezvous.request(), /read-only/);
  assert.deepEqual(fs.readdirSync(dir), ['listener.json']);
});

test('a joined stream discards the greeting and passes the rest', async () => {
  const [incoming, outgoing] = [new PassThrough(), new PassThrough()];
  const stream = new JoinedStream(incoming, outgoing, { greeting: true });
  incoming.write(Buffer.of(0));
  incoming.write(Buffer.from('ab'));
  incoming.end(Buffer.from('c'));
  const sent = collect(outgoing);
  stream.end('out');
  assert.equal((await collect(stream)).toString(), 'abc');
  assert.equal((await sent).toString(), 'out');

  const [first, second] = [new PassThrough(), new PassThrough()];
  const joined = new JoinedStream(first, second, { greeting: true });
  first.end(Buffer.from('\0xy'));
  assert.equal((await collect(joined)).toString(), 'xy');
});

test('a joined stream hangs up without a greeting or a live peer', async () => {
  const silent = new JoinedStream(new PassThrough().end(), new PassThrough(), { greeting: true });
  await assert.rejects(collect(silent), rejectsWith('ECONNRESET'));

  let alive = true;
  const watched = new JoinedStream(new PassThrough(), new PassThrough(), {
    greeting: true,
    peerAlive: () => alive,
  });
  const closed = collect(watched);
  await sleep(150);
  alive = false;
  await assert.rejects(closed, rejectsWith('ECONNRESET'));
});

test('a joined stream passes errors and waits for its release', async () => {
  for (const side of [0, 1]) {
    const parts = [new PassThrough(), new PassThrough()];
    const stream = new JoinedStream(parts[0], parts[1]);
    const failed = new Promise((resolve) => stream.once('error', resolve));
    parts[side].destroy(new Error(`side ${side}`));
    assert.equal((await failed).message, `side ${side}`);
  }
  const refusing = new Writable({
    write: (_chunk, _encoding, callback) => callback(new Error('full')),
  });
  const writer = new JoinedStream(new PassThrough(), refusing);
  writer.on('error', () => {});
  await assert.rejects(
    new Promise((resolve, reject) =>
      writer.write('x', (error) => (error ? reject(error) : resolve())),
    ),
    /full/,
  );

  const releases = [];
  for (const release of [
    () => {
      releases.push('throw');
      throw new Error('ignored');
    },
    async () => {
      releases.push('reject');
      throw new Error('ignored');
    },
  ]) {
    const stream = new JoinedStream(new PassThrough(), new PassThrough(), { release });
    await new Promise((resolve) => stream.destroy().once('close', resolve));
  }
  assert.deepEqual(releases, ['throw', 'reject']);
});

test('a joined stream ends its writable once, and reports a failed end', async () => {
  const done = new PassThrough();
  done.end();
  await new Promise((resolve) => done.once('finish', resolve));
  const finished = new JoinedStream(new PassThrough(), done);
  await new Promise((resolve, reject) =>
    finished.end((error) => (error ? reject(error) : resolve())),
  );

  const destroyed = new PassThrough();
  destroyed.destroy();
  const gone = new JoinedStream(new PassThrough(), destroyed);
  await new Promise((resolve, reject) => gone.end((error) => (error ? reject(error) : resolve())));

  const failing = new Writable({
    write: (_chunk, _encoding, callback) => callback(),
    final: (callback) => callback(new Error('cannot end')),
  });
  const ending = new JoinedStream(new PassThrough(), failing);
  ending.on('error', () => {});
  await assert.rejects(
    new Promise((resolve, reject) => ending.end((error) => (error ? reject(error) : resolve()))),
    /cannot end/,
  );
});

test('a joined stream pauses its readable while nobody reads', async () => {
  const incoming = new PassThrough();
  const stream = new JoinedStream(incoming, new PassThrough());
  const big = payload(100_000);
  incoming.end(big);
  await sleep(20);
  assert.ok(incoming.isPaused());
  // Asked for data but not draining it: the stream holds the source back.
  stream.read(0);
  await sleep(20);
  assert.ok(incoming.isPaused());
  assert.ok(stream.readableLength > 0);
  assert.deepEqual(await collect(stream), big);
});

for (const kind of ['fifo', 'pipe']) {
  test(`${kind}: connects through the rendezvous and half-closes`, { skip: !POSIX }, async (t) => {
    const dir = tempDir(t);
    const listener = new Listener(dir, kind, { onStream: echo });
    t.after(() => listener.close());
    const stream = await connectStream(`${kind}:${dir}`);
    assert.equal(stream.peerPid, process.pid);
    const sent = payload(200_000);
    const received = collect(stream);
    stream.end(sent);
    assert.deepEqual(await received, sent);
    stream.destroy();
    assert.deepEqual(fs.readdirSync(dir), ['listener.json']);
  });
}

test('fifo: the peer is watched until its late greeting', { skip: !POSIX }, async (t) => {
  const dir = tempDir(t);
  const listener = new Listener(dir, 'fifo', { onStream: echo, greetAfter: 250 });
  t.after(() => listener.close());
  const stream = await connectStream(`fifo:${dir}`);
  const received = collect(stream);
  stream.end('late');
  assert.equal((await received).toString(), 'late');
  stream.destroy();
});

test('fifo: a refused request closes and removes the FIFOs', { skip: !POSIX }, async (t) => {
  const dir = tempDir(t);
  const listener = new Listener(dir, 'fifo', { answer: () => ({ error: 'full' }) });
  t.after(() => listener.close());
  await assert.rejects(connectStream(`fifo:${dir}`), rejectsWith('ECONNREFUSED', /full/));
  // A descriptor that is already closed is no further error.
  const close = t.mock.method(fs, 'closeSync', (fd) => {
    close.mock.original(fd);
    throw new Error('closed twice');
  });
  await assert.rejects(connectStream(`fifo:${dir}`), rejectsWith('ECONNREFUSED', /full/));
  close.mock.restore();
  assert.deepEqual(fs.readdirSync(dir), ['listener.json']);
});

test('fifo: a FIFO that cannot be created is reported', { skip: !POSIX }, async (t) => {
  const dir = tempDir(t);
  writeListener(dir);
  const bin = path.join(dir, 'bin');
  fs.mkdirSync(bin);
  const saved = process.env.PATH;
  t.after(() => {
    process.env.PATH = saved;
  });
  process.env.PATH = bin;
  await assert.rejects(connectStream(`fifo:${dir}`), rejectsWith('ENOENT', /cannot create/));
  const fake = path.join(bin, 'mkfifo');
  fs.writeFileSync(fake, '#!/bin/sh\necho "mkfifo: no room" >&2\nexit 1\n', { mode: 0o755 });
  await assert.rejects(connectStream(`fifo:${dir}`), rejectsWith('EIO', /: mkfifo: no room$/));
  fs.writeFileSync(fake, '#!/bin/sh\nexit 1\n', { mode: 0o755 });
  await assert.rejects(connectStream(`fifo:${dir}`), rejectsWith('EIO', /Command failed/));
});
