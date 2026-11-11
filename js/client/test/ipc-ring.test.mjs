// Unit tests of the ring kinds (`file`, `mmap`, `shm`, `signal`) against a JS
// listener of the rendezvous directory (test/support/ipc-listener.mjs), and of
// the ring stream's failure paths on in-memory regions. They need no `clink`.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import {
  FileRegion,
  MemoryRegion,
  PollNotifier,
  RING_HEADER,
  RingStream,
  SignalNotifier,
  connectRing,
  initialize,
  installSignal,
  peerSignal,
  regionSize,
  ringBase,
  validate,
} from '../src/ipc-ring.js';
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
} from './support/ipc-listener.mjs';

const LINUX = process.platform === 'linux';
const SIGNALS = os.constants.signals;

/** A region that fails every access once `fail` is set. */
class FailingRegion extends MemoryRegion {
  fail = false;

  read(offset, length) {
    if (this.fail) throw new Error('region lost');
    return super.read(offset, length);
  }

  close() {
    if (this.fail) throw new Error('region lost');
  }
}

const side = (region, { client = true, peerPid = process.pid } = {}) =>
  new RingStream({ kind: 'test', region, client, notifier: new PollNotifier(1), peerPid });

const fresh = (capacity = 64, Region = MemoryRegion) => {
  const region = new Region(regionSize(capacity));
  initialize(region, capacity);
  return region;
};

const written = (stream, bytes) =>
  new Promise((resolve, reject) => {
    stream.write(bytes, (error) => (error ? reject(error) : resolve()));
  });

const ended = (stream) =>
  new Promise((resolve, reject) => stream.end((error) => (error ? reject(error) : resolve())));

for (const kind of ['file', 'mmap', 'signal', 'shm']) {
  test(
    `${kind}: connects through the rendezvous and echoes`,
    { skip: kind === 'shm' && !LINUX },
    async (t) => {
      const dir = tempDir(t);
      const listener = new Listener(dir, kind, { onStream: echo });
      t.after(() => listener.close());
      const stream = await connectStream(`${kind}:${dir}?capacity=64&poll=1`);
      assert.equal(stream.kind, kind);
      assert.equal(stream.capacity, 64);
      assert.equal(stream.peerPid, process.pid);
      const sent = payload(5_000);
      const received = collect(stream);
      stream.end(sent);
      assert.deepEqual(await received, sent);
      stream.destroy();
      assert.deepEqual(fs.readdirSync(dir), ['listener.json'], 'the region is removed at once');
    },
  );
}

test('a ring keeps its file until it hangs up on Windows', async (t) => {
  const dir = tempDir(t);
  const listener = new Listener(dir, 'file', { onStream: echo });
  t.after(() => listener.close());
  const stream = await connectRing(parseEndpoint(`file:${dir}`), {}, 'win32');
  assert.equal(fs.readdirSync(dir).filter((name) => name.endsWith('.ring')).length, 1);
  await new Promise((resolve) => stream.destroy().once('close', resolve));
  assert.deepEqual(fs.readdirSync(dir), ['listener.json']);
});

test('a refused ring connection removes its region', async (t) => {
  const dir = tempDir(t);
  let answer = () => ({ error: 'no room' });
  const listener = new Listener(dir, 'signal', { answer: () => answer() });
  t.after(() => listener.close());
  await assert.rejects(connectStream(`signal:${dir}`), rejectsWith('ECONNREFUSED', /no room/));
  answer = () => ({ pid: process.pid });
  await assert.rejects(connectStream(`signal:${dir}`), rejectsWith('EINVAL', /lacks the number/));
  assert.deepEqual(fs.readdirSync(dir), ['listener.json']);
});

test('a ring region that cannot be sized is removed', async (t) => {
  const dir = tempDir(t);
  const listener = new Listener(dir, 'mmap');
  t.after(() => listener.close());
  t.mock.method(fs, 'ftruncateSync', () => {
    throw Object.assign(new Error('disk full'), { code: 'ENOSPC' });
  });
  await assert.rejects(connectStream(`mmap:${dir}`), /disk full/);
  assert.deepEqual(fs.readdirSync(dir), ['listener.json']);
});

test('ring handshakes check the signal numbers', () => {
  assert.equal(peerSignal({ signal: 10 }), 10);
  assert.throws(() => peerSignal({}), rejectsWith('EINVAL', /lacks/));
  for (const signal of [0, 65]) {
    assert.throws(() => peerSignal({ signal }), rejectsWith('EINVAL', /invalid signal/));
  }
  for (const signo of [999, SIGNALS.SIGKILL, SIGNALS.SIGSTOP]) {
    assert.throws(() => installSignal(signo), rejectsWith('EINVAL', /cannot be caught/));
  }
});

test('the signal ring takes the report signal over', (t) => {
  const report = process.report;
  const saved = [report.signal, report.reportOnSignal];
  t.after(() => {
    report.signal = saved[0];
    report.reportOnSignal = saved[1];
  });
  report.signal = 'SIGWINCH';
  report.reportOnSignal = true;
  installSignal(SIGNALS.SIGWINCH);
  assert.equal(report.reportOnSignal, false);
  // Installed once for the life of the process.
  report.reportOnSignal = true;
  installSignal(SIGNALS.SIGWINCH);
  assert.equal(report.reportOnSignal, true);
});

test('a signal the process cannot handle is reported', (t) => {
  t.mock.method(process, 'on', () => {
    throw new Error('uv_signal_start EINVAL');
  });
  assert.throws(() => installSignal(SIGNALS.SIGURG), rejectsWith('EINVAL', /cannot be caught/));
});

test('the signal notifier waits for a signal or its limit', async () => {
  const gone = new SignalNotifier(deadPid(), SIGNALS.SIGWINCH);
  gone.notify();
  const own = new SignalNotifier(process.pid, SIGNALS.SIGWINCH);
  const generation = own.generation();
  await own.wait(generation, 0, 1000);
  const started = Date.now();
  await own.wait(generation, 16, 20);
  assert.ok(Date.now() - started >= 15);
  const woken = own.wait(generation, 16, 10_000);
  own.notify();
  await woken;
  assert.notEqual(own.generation(), generation);
  await own.wait(generation, 16, 10_000);
});

test('ring regions are checked before use', (t) => {
  assert.throws(() => validate(new MemoryRegion(10)), rejectsWith('EINVAL', /too small/));
  const region = fresh();
  assert.equal(validate(region), 64);
  region.bytes.write('XXXX', 0, 'latin1');
  assert.throws(() => validate(region), /magic/);
  const versioned = fresh();
  versioned.bytes.writeUInt32LE(2, 4);
  assert.throws(() => validate(versioned), /another version/);
  for (const capacity of [10, 1 << 20]) {
    const sized = fresh();
    sized.bytes.writeUInt32LE(capacity, 8);
    assert.throws(() => validate(sized), /invalid capacity/);
  }
  const dir = tempDir(t);
  const file = path.join(dir, 'short');
  fs.writeFileSync(file, Buffer.alloc(8));
  const short = new FileRegion(fs.openSync(file, 'r+'), 64);
  assert.throws(() => short.read(0, 64), rejectsWith('EINVAL', /too small/));
  short.write(0, Buffer.from('LIPC'));
  assert.equal(short.read(0, 4).toString(), 'LIPC');
  short.close();
});

test('a torn ring counter fails the reader', async () => {
  const region = fresh();
  region.bytes.writeUInt32LE(5, ringBase(0, 64) + 4);
  const reader = side(region, { client: false });
  await assert.rejects(collect(reader), rejectsWith('EINVAL', /stays torn/));
});

test('a ring reader takes the bytes published with the end', async () => {
  const region = fresh();
  const base = ringBase(0, 64);
  const read = region.read.bind(region);
  let raced = false;
  region.read = (offset, length) => {
    if (offset === base + 8 && !raced) {
      // The writer publishes its last byte and ends right after the positions were read.
      raced = true;
      region.bytes.write('z', base + RING_HEADER);
      region.bytes.writeUInt32LE(1, base);
      region.bytes.writeUInt32LE(~1 >>> 0, base + 4);
      region.bytes.writeUInt32LE(1, base + 8);
    }
    return read(offset, length);
  };
  assert.equal((await collect(side(region, { client: false }))).toString(), 'z');
});

test('a ring wraps around, and a reader held back at its limit resumes', async () => {
  const region = fresh();
  const writer = side(region);
  const reader = side(region, { client: false });
  reader.read(0);
  await written(writer, payload(10));
  while (reader.readableLength < 10) await sleep(1);
  assert.deepEqual(reader.read(), payload(10));
  const limit = reader.readableHighWaterMark;
  const sent = payload(limit + 1_000);
  const writing = written(writer, sent);
  while (reader.readableLength < limit) await sleep(1);
  const received = collect(reader);
  await writing;
  await ended(writer);
  assert.deepEqual(await received, sent);
  writer.destroy();
  reader.destroy();
});

test('a ring notices a peer process that is gone', async () => {
  const pid = deadPid();
  const reader = side(fresh(), { client: false, peerPid: pid });
  await assert.rejects(collect(reader), rejectsWith('ECONNRESET'));
  const writer = side(fresh(), { peerPid: pid });
  writer.on('error', () => {});
  await assert.rejects(written(writer, payload(200)), rejectsWith('EPIPE', /no longer reads/));
});

test('a ring refuses writes once its direction is closed', async () => {
  const state = ringBase(0, 64) + 8;
  const region = fresh();
  const writer = side(region);
  writer.on('error', () => {});
  region.bytes.writeUInt32LE(1, state);
  await assert.rejects(written(writer, Buffer.from('x')), rejectsWith('EPIPE', /outgoing/));
  // Ending a direction that is already ended stores nothing more.
  const aborted = fresh();
  aborted.bytes.writeUInt32LE(2, state);
  await ended(side(aborted));
  assert.equal(aborted.bytes.readUInt32LE(state), 2);

  const full = side(fresh());
  full.on('error', () => {});
  const blocked = written(full, payload(200));
  await sleep(20);
  full.destroy();
  await assert.rejects(blocked, rejectsWith('EPIPE', /outgoing/));
});

test('a ring whose region fails still hangs up', async () => {
  const region = fresh(64, FailingRegion);
  const stream = side(region);
  stream.on('error', () => {});
  region.fail = true;
  const closed = new Promise((resolve) => stream.once('close', resolve));
  await assert.rejects(ended(stream), /region lost/);
  await closed;
  assert.ok(stream.destroyed);
});
