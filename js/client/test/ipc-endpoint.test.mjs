// Unit tests of the IPC endpoint layer: parsing, options, the registry, the
// ring encoding and the datagram session. They need no `clink` binary; the
// interop tests against the Rust reference are in ipc.test.mjs.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  ACK,
  DATA,
  DatagramSession,
  FRAME_HEADER,
  agreeChunk,
  frame,
  parseFrame,
} from '../src/ipc-datagram.js';
import { commandLine } from '../src/ipc-endpoint.js';
import { randomId } from '../src/ipc-rendezvous.js';
import {
  MemoryRegion,
  PollNotifier,
  RingStream,
  initialize,
  regionSize,
  validate,
} from '../src/ipc-ring.js';
import {
  Endpoint,
  IpcError,
  IpcTransport,
  KINDS,
  connectStream,
  parseEndpoint,
  registerTransport,
  supports,
  supportsNatively,
} from '../src/ipc.js';

const LINUX = process.platform === 'linux';
const WINDOWS = process.platform === 'win32';

const rejectsWith = (code) => (error) => {
  assert.ok(error instanceof IpcError, `an IpcError, got ${error}`);
  assert.equal(error.code, code, error.message);
  return true;
};

const payload = (length, seed) => {
  const bytes = Buffer.alloc(length);
  for (let index = 0; index < length; index += 1) bytes[index] = (index * 31 + seed * 7) & 0xff;
  return bytes;
};

const collect = (stream) =>
  new Promise((resolve, reject) => {
    const chunks = [];
    stream.on('data', (chunk) => chunks.push(chunk));
    stream.once('end', () => resolve(Buffer.concat(chunks)));
    stream.once('error', reject);
  });

test('endpoints parse as the Rust reference does', () => {
  const endpoint = parseEndpoint('shm:/tmp/links?capacity=1048576&timeout=500');
  assert.ok(endpoint instanceof Endpoint);
  assert.equal(endpoint.kind, 'shm');
  assert.equal(endpoint.address, '/tmp/links');
  assert.equal(endpoint.option('capacity'), '1048576');
  assert.equal(endpoint.timeout(), 500);
  assert.equal(parseEndpoint('localhost:80').kind, 'tcp');
  assert.equal(parseEndpoint('[::1]:80').kind, 'tcp');
  assert.equal(parseEndpoint('stdio').kind, 'stdio');
  assert.equal(parseEndpoint('  UNIX:/tmp/s  ').kind, 'unix');
  assert.throws(() => parseEndpoint('shm:'), rejectsWith('EINVAL'));
  assert.throws(() => parseEndpoint('shm:/tmp/x?capacity=1'), rejectsWith('EINVAL'));
  assert.throws(() => parseEndpoint('shm:/tmp/x?nonsense=1'), /no option `nonsense`/);
  assert.throws(() => parseEndpoint('stdio:x'), /takes no address/);
  assert.throws(() => parseEndpoint(''), /must not be empty/);
  assert.throws(() => parseEndpoint('udp:h:1?chunk'), /needs a `=value`/);
  const text = 'mq:/tmp/links?chunk=512';
  assert.equal(parseEndpoint(text).toString(), text);
  assert.equal(parseEndpoint('udp:h:1?rto=5&chunk=64').toString(), 'udp:h:1?chunk=64&rto=5');
  assert.equal(parseEndpoint(parseEndpoint(text)).toString(), text);
});

test('option values are whole numbers within their ranges', () => {
  assert.equal(parseEndpoint('tcp:h:1').timeout(), 10_000);
  assert.equal(parseEndpoint('tcp:h:1?timeout=0').timeout(), 1);
  assert.equal(parseEndpoint('tcp:h:1?timeout=+7').timeout(), 7);
  assert.throws(() => parseEndpoint('tcp:h:1?timeout=-1'), /from 0 to 86400000/);
  assert.throws(() => parseEndpoint('tcp:h:1?timeout=1.5'), /found `1.5`/);
  assert.throws(() => parseEndpoint('tcp:h:1?timeout=86400001'), rejectsWith('EINVAL'));
  assert.throws(() => parseEndpoint('udp:h:1?chunk=15'), /from 16 to 65000/);
  assert.equal(parseEndpoint('udp:h:1?chunk=65000').number('chunk', 4096, 16, 65000), 65000);
  assert.throws(() => parseEndpoint('signal:/tmp/x?signo=65'), /from 1 to 64/);
  assert.throws(() => parseEndpoint('mq:/tmp/x?maxmsg=0'), /from 1 to 65536/);
  assert.throws(() => parseEndpoint('tcp:h:1?capacity=4096'), /no option `capacity`/);
  assert.equal(parseEndpoint('fifo:/tmp/x?poll=3&accept-poll=4').millis('poll', 1), 3);
});

test('exec commands are words or a JSON array', () => {
  assert.equal(parseEndpoint('exec:clink --serve stdio?x').address, 'clink --serve stdio?x');
  assert.deepEqual(commandLine('  clink   --serve stdio '), ['clink', '--serve', 'stdio']);
  assert.deepEqual(commandLine('["a b", "c"]'), ['a b', 'c']);
  assert.throws(() => commandLine('[1]'), /JSON array of strings/);
  assert.throws(() => parseEndpoint('exec:  '), /needs a command/);
});

test('the registry adds custom kinds but keeps the built-in ones', async () => {
  assert.ok(KINDS.includes('shm'));
  assert.throws(() => registerTransport('tcp', () => null), rejectsWith('EEXIST'));
  assert.throws(() => registerTransport('bad kind', () => null), rejectsWith('EINVAL'));
  assert.throws(() => registerTransport('loop', 'nope'), TypeError);
  assert.equal(parseEndpoint('loop:x').kind, 'tcp');
  assert.equal(supports('loop'), false);
  const seen = [];
  registerTransport('Loop', (endpoint, options) => {
    seen.push([endpoint.toString(), typeof options.signal]);
    return Promise.reject(new IpcError('refused on purpose', 'ECONNREFUSED'));
  });
  const endpoint = parseEndpoint('loop:x?anything=1');
  assert.equal(endpoint.kind, 'loop');
  assert.equal(endpoint.option('anything'), '1');
  assert.ok(supportsNatively('LOOP'));
  await assert.rejects(connectStream('loop:x?anything=1'), /refused on purpose/);
  assert.deepEqual(seen, [['loop:x?anything=1', 'object']]);
  await assert.rejects(IpcTransport.connect('loop:x'), (error) => {
    assert.equal(error.name, 'ConnectionClosedError');
    assert.match(error.message, /cannot connect to loop:x: refused on purpose/);
    assert.equal(error.outcome, 'not-sent');
    return true;
  });
});

test('the support sets follow the platform', () => {
  for (const kind of ['tcp', 'udp', 'exec', 'pipe', 'file', 'mmap']) {
    assert.ok(supportsNatively(kind), kind);
  }
  assert.equal(supportsNatively('unix'), !WINDOWS);
  assert.equal(supportsNatively('shm'), LINUX);
  assert.equal(supports('mq'), LINUX);
  assert.equal(supports('mailslot'), WINDOWS);
  assert.equal(supports('stdio'), false);
  assert.equal(supports('nonsense'), false);
});

test('connecting reports unusable endpoints', async () => {
  await assert.rejects(connectStream('stdio'), rejectsWith('ENOTSUP'));
  if (!WINDOWS) {
    await assert.rejects(connectStream('mailslot:x'), rejectsWith('ENOTSUP'));
  }
  if (!LINUX && !WINDOWS) {
    await assert.rejects(connectStream('unix:@name'), /exist only on Linux/);
  }
  await assert.rejects(
    connectStream('exec:/nonexistent/clink-ipc-test --serve stdio'),
    /cannot start/,
  );
  const aborted = AbortSignal.abort();
  await assert.rejects(connectStream('tcp:127.0.0.1:9', { signal: aborted }));
});

test('datagram frames round trip', () => {
  const bytes = frame(DATA, 7, Buffer.from('links'));
  assert.equal(bytes.length, FRAME_HEADER + 5);
  assert.deepEqual([...bytes.subarray(0, FRAME_HEADER)], [DATA, 0, 0, 0, 7, 0, 0, 0]);
  const parsed = parseFrame(bytes);
  assert.equal(parsed.type, DATA);
  assert.equal(parsed.seq, 7);
  assert.equal(parsed.payload.toString(), 'links');
  assert.equal(parseFrame(bytes.subarray(0, 4)), null);
  assert.equal(frame(ACK, 0xffffffff).readUInt32LE(4), 0xffffffff);
  assert.equal(agreeChunk(4096, 1024), 1024);
  assert.equal(agreeChunk(4096, undefined), 4096);
  assert.equal(agreeChunk(1, 1), 16);
  assert.equal(agreeChunk(70_000, 70_000), 65000);
});

test('the ring region layout matches the specification', () => {
  assert.equal(regionSize(4096), 64 + 2 * (128 + 4096));
  const region = new MemoryRegion(regionSize(64));
  initialize(region, 64);
  assert.equal(region.bytes.subarray(0, 4).toString(), 'LIPC');
  assert.equal(region.bytes.readUInt32LE(4), 1);
  assert.equal(validate(region), 64);
  // Both rings start empty: each counter is 0 followed by its complement.
  for (const base of [64, 64 + 128 + 64]) {
    for (const offset of [0, 64]) {
      assert.equal(region.bytes.readUInt32LE(base + offset), 0);
      assert.equal(region.bytes.readUInt32LE(base + offset + 4), 0xffffffff);
    }
  }
  assert.throws(() => validate(new MemoryRegion(8)), /too small/);
  const other = new MemoryRegion(regionSize(64));
  assert.throws(() => validate(other), /magic/);
});

test('random ids are twenty hex digits', () => {
  const [a, b] = [randomId(), randomId()];
  assert.match(a, /^[0-9a-f]{20}$/);
  assert.notEqual(a, b);
});

const ringPair = (capacity) => {
  const region = new MemoryRegion(regionSize(capacity));
  initialize(region, capacity);
  const side = (client) =>
    new RingStream({
      kind: 'test',
      region,
      client,
      notifier: new PollNotifier(1),
      peerPid: process.pid,
    });
  return [side(true), side(false), region];
};

test('a ring carries both directions through wrap-around and ends', async () => {
  const [client, server, region] = ringPair(64);
  const up = payload(5_000, 1);
  const down = payload(3_333, 2);
  const [atServer, atClient] = [collect(server), collect(client)];
  client.end(up);
  server.end(down);
  assert.deepEqual(await atServer, up);
  assert.deepEqual(await atClient, down);
  // Writer state 1 (ended) on both rings.
  assert.equal(region.bytes.readUInt32LE(64 + 8), 1);
  assert.equal(region.bytes.readUInt32LE(64 + 128 + 64 + 8), 1);
  client.destroy();
  server.destroy();
});

test('a ring hang-up ends the peer stream after the data before it', async () => {
  const [client, server, region] = ringPair(64);
  const reading = collect(server);
  client.write(Buffer.from('partial'));
  client.destroy();
  assert.deepEqual(await reading, Buffer.from('partial'));
  // Writer state 1 (ended) on the client's ring, reader state 2 on the other,
  // as the Rust and C# implementations store them.
  assert.equal(region.bytes.readUInt32LE(64 + 8), 1);
  assert.equal(region.bytes.readUInt32LE(64 + 128 + 64 + 64 + 8), 2);
  server.destroy();
});

test('an aborted ring direction resets the reader', async () => {
  const [client, server, region] = ringPair(64);
  const reading = collect(server);
  // Writer state 2 on the client's ring aborts the direction.
  region.bytes.writeUInt32LE(2, 64 + 8);
  await assert.rejects(reading, rejectsWith('ECONNRESET'));
  client.destroy();
  // A writer whose reader is gone gets a broken pipe.
  const [writer, gone] = ringPair(64);
  gone.destroy();
  const writing = new Promise((resolve, reject) => {
    writer.write(Buffer.from('x'), (error) => (error ? reject(error) : resolve()));
  });
  writer.on('error', () => {});
  await assert.rejects(writing, rejectsWith('EPIPE'));
  writer.destroy();
});

/** Two sessions joined by an in-memory link that loses every `loseEvery`-th frame. */
const sessionPair = (loseEvery, chunk) => {
  const sessions = [];
  const link = (index) => {
    let count = 0;
    return {
      send(bytes) {
        count += 1;
        if (loseEvery > 0 && count % loseEvery === 0) return;
        const copy = Buffer.from(bytes);
        setImmediate(() => sessions[1 - index].receive(copy));
      },
      close() {},
    };
  };
  for (const index of [0, 1]) {
    sessions.push(new DatagramSession('test', link(index), { chunk, rto: 5 }));
  }
  return sessions;
};

test('datagram sessions survive loss', { timeout: 60_000 }, async () => {
  for (const loseEvery of [0, 3, 7]) {
    const [a, b] = sessionPair(loseEvery, 64);
    assert.equal(a.chunk, 64);
    const sent = payload(20_000, loseEvery);
    const reply = payload(777, loseEvery + 1);
    const [atB, atA] = [collect(b), collect(a)];
    a.end(sent);
    b.end(reply);
    assert.deepEqual(await atB, sent, `lose every ${loseEvery}`);
    assert.deepEqual(await atA, reply, `lose every ${loseEvery}`);
    await Promise.all(
      [a, b].map((session) => new Promise((resolve) => session.destroy().once('close', resolve))),
    );
  }
});

test('a destroyed datagram session hangs up its peer', { timeout: 10_000 }, async () => {
  const [a, b] = sessionPair(0, 64);
  const reading = collect(b);
  a.write(Buffer.from('before'));
  await new Promise((resolve) => setTimeout(resolve, 50));
  a.destroy();
  // An orderly end-of-stream goes first, so the reader sees the bytes and the end.
  assert.equal((await reading).toString(), 'before');
  await new Promise((resolve) => b.destroy().once('close', resolve));
});
