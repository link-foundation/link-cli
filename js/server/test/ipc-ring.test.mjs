// Unit tests of ipc-ring.js, the byte rings of rust/src/protocol/ipc/ring.rs
// and the signal notifier of rust/src/protocol/ipc/unix/mod.rs: positioned
// file I/O, the header checks, both notifiers, signals that cannot be caught,
// the ring over an in-memory region (wrapping, torn and disagreeing counters,
// a dead peer, closed directions, a failing region at shutdown), and the
// rendezvous of `connect`/`listen` (Windows' cleanup, an ack without a pid,
// signals refused on either side).
import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as os from 'node:os';
import { join } from 'node:path';
import { after, describe, it } from 'node:test';
import { Endpoint } from '../src/ipc-endpoint.js';
import { JsonObject } from '../src/ipc-json.js';
import { RendezvousListener, writeAtomically } from '../src/ipc-rendezvous.js';
import {
  ABORTED,
  connectRing,
  createFile,
  FileRegion,
  HEADER,
  initialize,
  installSignal,
  listenRing,
  MemoryRegion,
  openFile,
  ownSignal,
  peerSignal,
  PollNotifier,
  regionSize,
  RING_HEADER,
  RingTransport,
  SignalNotifier,
  validate,
} from '../src/ipc-ring.js';
import { setTrace } from '../src/ipc-trace.js';
import { diagnostics } from '../src/protocol-errors.js';

const root = fs.mkdtempSync(join(os.tmpdir(), 'ipc-ring-'));
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

/** An `fs` stand-in whose positioned I/O moves the given counts in turn. */
function counting(reads, writes) {
  const calls = [];
  return {
    calls,
    readSync: (...args) => {
      calls.push(['read', args[2], args[3], args[4]]);
      return reads.shift();
    },
    writeSync: (...args) => {
      calls.push(['write', args[2], args[3], args[4]]);
      return writes.shift();
    },
    closeSync: (fd) => calls.push(['close', fd]),
  };
}

describe('FileRegion', () => {
  it('repeats short transfers and fails on an empty one', () => {
    const api = counting([2, 2, 1, 0], [3, 1, 0]);
    const region = new FileRegion(5, 100, api);
    assert.equal(region.readBytes(10, 4).length, 4);
    assert.throws(() => region.readBytes(20, 4), {
      kind: 'UnexpectedEof',
      message: 'the ring file is shorter than its header says',
    });
    region.writeBytes(30, Buffer.alloc(4));
    assert.throws(() => region.writeBytes(40, Buffer.alloc(1)), {
      kind: 'WriteZero',
      message: 'the ring file is full',
    });
    region.close();
    assert.deepEqual(api.calls, [
      ['read', 0, 4, 10],
      ['read', 2, 2, 12],
      ['read', 0, 4, 20],
      ['read', 1, 3, 21],
      ['write', 0, 4, 30],
      ['write', 3, 1, 33],
      ['write', 0, 1, 40],
      ['close', 5],
    ]);
  });

  it('creates and opens ring files, closing them when that fails', () => {
    const calls = [];
    const failing = (name) => () => {
      calls.push(name);
      throw new Error(`${name} failed`);
    };
    const api = {
      openSync: (path, flags, mode) => {
        calls.push(['open', path, flags, mode]);
        return 7;
      },
      ftruncateSync: failing('ftruncate'),
      fstatSync: failing('fstat'),
      closeSync: (fd) => calls.push(['close', fd]),
      rmSync: (path, options) => calls.push(['rm', path, options]),
    };
    assert.throws(() => createFile('r.ring', 300, api), { message: 'ftruncate failed' });
    assert.throws(() => openFile('r.ring', api), { message: 'fstat failed' });
    assert.deepEqual(calls, [
      ['open', 'r.ring', 'wx+', 0o600],
      'ftruncate',
      ['close', 7],
      ['rm', 'r.ring', { force: true }],
      ['open', 'r.ring', 'r+', undefined],
      'fstat',
      ['close', 7],
    ]);
    const path = join(root, 'real.ring');
    fs.closeSync(createFile(path, 300));
    const region = openFile(path);
    assert.equal(region.length, 300);
    region.close();
  });
});

describe('the region header', () => {
  it('rejects every malformed header', () => {
    const invalid = (message) => ({ kind: 'InvalidData', message });
    assert.throws(
      () => validate(new MemoryRegion(HEADER - 1)),
      invalid('the ring region is too small'),
    );
    const region = new MemoryRegion(regionSize(64));
    assert.throws(() => validate(region), invalid('the ring region lacks the `LIPC` magic'));
    initialize(region, 64);
    assert.equal(validate(region), 64);
    region.store(4, 2);
    assert.throws(() => validate(region), invalid('the ring region has another version'));
    region.store(4, 1);
    for (const capacity of [63, 2 ** 30 + 1, 65]) {
      region.store(8, capacity);
      assert.throws(() => validate(region), invalid('the ring region has an invalid capacity'));
    }
    region.close();
  });
});

describe('notifiers', () => {
  it('polls at once, then sleeps longer up to the poll interval', async () => {
    const notifier = new PollNotifier(2);
    notifier.notify();
    assert.equal(notifier.generation(), 0);
    await notifier.wait(0, 0, 100);
    await notifier.wait(0, 128, 100);
    await notifier.wait(0, 1000, 1);
  });

  it('signals the peer, ignores a gone one and waits for its own signal', async () => {
    const kills = [];
    new SignalNotifier(42, 12, (...args) => kills.push(args)).notify();
    assert.deepEqual(kills, [[42, 12]]);
    const gone = new SignalNotifier(42, 12, () => {
      throw new Error('kill ESRCH');
    });
    gone.notify();
    const seen = gone.generation();
    await gone.wait(seen, 0, 1000);
    await gone.wait(seen - 1, 16, 1000);
    const started = performance.now();
    await gone.wait(seen, 16, 20);
    assert.ok(performance.now() - started >= 15);
  });

  it('refuses signals that cannot be caught or that the peer must not ask for', () => {
    assert.throws(() => installSignal(os.constants.signals.SIGKILL), {
      kind: 'InvalidInput',
      message: `signal ${os.constants.signals.SIGKILL} cannot be caught`,
    });
    assert.throws(() => installSignal(40), {
      kind: 'Unsupported',
      message:
        '`signal:` endpoints are not supported by the JavaScript server: ' +
        'Node.js has no API for catching signal 40',
    });
    assert.throws(() => ownSignal(Endpoint.parse(`signal:${root}?signo=9`)), {
      kind: 'InvalidInput',
    });
    for (const signo of [0, 65]) {
      assert.throws(() => peerSignal(new JsonObject().with('signal', signo)), {
        kind: 'InvalidData',
        message: `the peer asked for the invalid signal ${signo}`,
      });
    }
    assert.equal(peerSignal(new JsonObject().with('signal', 64)), 64);
  });
});

/** A region whose every access fails once `broken` is set. */
class BrokenRegion extends MemoryRegion {
  broken = false;

  readBytes(offset, length) {
    if (this.broken) throw new Error('the region is gone');
    return super.readBytes(offset, length);
  }

  writeBytes(offset, bytes) {
    if (this.broken) throw new Error('the region is gone');
    super.writeBytes(offset, bytes);
  }

  close() {
    if (this.broken) throw new Error('the region is gone');
  }

  load(offset) {
    const value = super.load(offset);
    this.onLoad?.(offset);
    return value;
  }
}

/** Both sides of a ring of `capacity` bytes in memory. */
function pair(capacity = 64, alive = () => true) {
  const region = new BrokenRegion(regionSize(capacity));
  initialize(region, capacity);
  const client = new RingTransport('mmap', region, true, new PollNotifier(1), 1, { alive });
  const server = new RingTransport('mmap', region, false, new PollNotifier(1), 2, { alive });
  return { region, client, server };
}

/** Publishes `value` as the counter at `offset`. */
function publish(region, offset, value) {
  region.store(offset, value);
  region.store(offset + 4, ~value >>> 0);
}

// The client writes ring 0, whose counters start right after the header.
const HEAD0 = HEADER;
const DATA0 = HEADER + RING_HEADER;

describe('RingTransport', { timeout: 30_000 }, () => {
  it('wraps around the end of a ring', async () => {
    const { client, server } = pair();
    const first = Buffer.alloc(60, 1);
    assert.equal(await client.write(first), 60);
    assert.deepEqual(await server.read(100, null), first);
    const second = Buffer.from('0123456789');
    assert.equal(await client.write(second), 10);
    assert.deepEqual(await server.read(100, 1000), second);
    server.shutdown();
    await assert.rejects(client.write(second), {
      kind: 'BrokenPipe',
      message: 'the peer no longer reads',
    });
  });

  it('drains bytes published just before the end', async () => {
    const { region, client, server } = pair();
    await client.closeWrite();
    region.onLoad = (offset) => {
      if (offset !== HEAD0 + 8) return;
      region.onLoad = null;
      region.writeBytes(DATA0, Buffer.from('z'));
      publish(region, HEAD0, 1);
    };
    assert.equal((await server.read(10, null)).toString(), 'z');
    assert.equal((await server.read(10, null)).length, 0);
  });

  it('waits while head and tail disagree', async () => {
    const { region, client, server } = pair();
    publish(region, HEAD0, 100);
    const reading = server.read(10, null);
    const writing = client.write(Buffer.from('xy'));
    await delay(5);
    region.writeBytes(DATA0, Buffer.from('abc'));
    publish(region, HEAD0, 3);
    assert.equal((await reading).toString(), 'abc');
    assert.equal(await writing, 2);
  });

  it('gives up on a counter that stays torn', async () => {
    const { region, server } = pair();
    region.store(HEAD0, 5);
    await assert.rejects(server.read(10, null), {
      kind: 'InvalidData',
      message: 'a ring counter stays torn',
    });
  });

  it('notices a dead peer while reading or writing', async () => {
    const { client, server } = pair(64, () => false);
    await assert.rejects(server.read(10, null), { kind: 'ConnectionReset' });
    assert.equal(await client.write(Buffer.alloc(100)), 64);
    await assert.rejects(client.write(Buffer.alloc(1)), {
      kind: 'BrokenPipe',
      message: 'the peer no longer reads',
    });
  });

  it('times out, ends, aborts and refuses writes after the end', async () => {
    const { region, client, server } = pair();
    await assert.rejects(server.read(10, 20), { kind: 'TimedOut' });
    await client.closeWrite();
    await client.closeWrite();
    assert.equal((await server.read(10, null)).length, 0);
    await assert.rejects(client.write(Buffer.from('x')), {
      kind: 'BrokenPipe',
      message: 'the outgoing direction was closed',
    });
    region.store(HEADER + 8, ABORTED);
    await assert.rejects(server.read(10, null), { kind: 'ConnectionReset' });
    server.shutdown();
    await assert.rejects(server.write(Buffer.from('x')), {
      message: 'the outgoing direction was closed',
    });
  });

  it('hangs up even when the region fails, removing its files', async () => {
    const { region, client } = pair();
    const path = join(root, 'cleanup.ring');
    fs.writeFileSync(path, '');
    client.cleanup = [path];
    region.broken = true;
    const lines = await traced(async () => {
      client.shutdown();
      client.shutdown();
    });
    assert.deepEqual(lines, ['[ipc] mmap ring to process 1 hung up']);
    assert.equal(fs.existsSync(path), false);
    assert.equal((await client.read(10, null)).length, 0);
    await client.closeWrite();
  });
});

const ringFiles = (dir, suffix) => fs.readdirSync(dir).filter((name) => name.endsWith(suffix));

describe('connect and listen', { timeout: 30_000 }, () => {
  it('keeps the ring file until both sides hang up, as on Windows', async () => {
    const endpoint = Endpoint.parse(`mmap:${freshDir()}?capacity=64`);
    const listener = await listenRing(endpoint, { platform: 'win32' });
    const accepting = listener.accept();
    const client = await connectRing(endpoint, { platform: 'win32' });
    const server = await accepting;
    assert.equal(ringFiles(endpoint.address, '.mmap').length, 1);
    await client.write(Buffer.from('ping'));
    assert.equal((await server.read(10)).toString(), 'ping');
    client.shutdown();
    server.shutdown();
    assert.deepEqual(ringFiles(endpoint.address, '.mmap'), []);
    listener.close();
  });

  it("takes the listener's process id when the ack lacks one", async () => {
    const endpoint = Endpoint.parse(`file:${freshDir()}`);
    const rendezvous = RendezvousListener.bind(endpoint, 'file');
    const connecting = connectRing(endpoint);
    const request = await rendezvous.next();
    writeAtomically(request.path('ack'), '{"v":1}');
    const stream = await connecting;
    assert.equal(stream.transport.peer.pid, process.pid);
    stream.shutdown();
    rendezvous.close();
  });

  it(
    'refuses a signal either side cannot catch',
    { skip: WINDOWS && 'signal needs POSIX' },
    async () => {
      const dir = freshDir();
      const listener = await listenRing(Endpoint.parse(`signal:${dir}?signo=9`));
      await assert.rejects(connectRing(Endpoint.parse(`signal:${dir}?signo=9`)), {
        kind: 'InvalidInput',
        message: 'signal 9 cannot be caught',
      });
      assert.deepEqual(ringFiles(dir, '.sig'), []);
      const accepting = listener.accept();
      await assert.rejects(connectRing(Endpoint.parse(`signal:${dir}?timeout=5000`)), {
        kind: 'ConnectionRefused',
        message: 'the listener refused: signal 9 cannot be caught',
      });
      listener.close();
      assert.equal(await accepting, null);
    },
  );
});
