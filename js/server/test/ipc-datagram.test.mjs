// Unit tests of ipc-datagram.js, the reliable session of
// rust/src/protocol/ipc/datagram.rs: the `datagram_sessions_survive_loss`,
// `a_peer_hanging_up_instead_of_acknowledging_the_end_still_ends_the_stream`,
// `a_peer_removing_the_link_before_the_end_is_acknowledged_still_ends_the_stream`
// and `datagram_frames_round_trip` tests of rust/src/protocol/ipc/tests.rs
// over an in-memory link (Rust's `memory::pair`), plus the frames a peer may
// send out of order, heartbeats, dead peers and the lingering hang-up.
import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import {
  ABORT,
  ACK,
  agreeChunk,
  DATA,
  EOF,
  frame,
  HEADER,
  parseFrame,
  PING,
  Session,
} from '../src/ipc-datagram.js';
import { IpcStream } from '../src/ipc-stream.js';
import { hungUp, sleep } from '../src/ipc-util.js';

/** One end of an in-memory link; every `loseEvery`-th frame of the pair is lost. */
class MemoryLink {
  peer = null;
  sent = [];
  closed = false;
  failing = false;
  /** Once set, sends fail as a removed System V queue does (`EIDRM`). */
  gone = false;

  constructor(counter, loseEvery) {
    this.counter = counter;
    this.loseEvery = loseEvery;
  }

  async send(bytes) {
    if (this.failing) throw new Error('the link failed');
    if (this.gone) throw hungUp();
    this.sent.push(parseFrame(bytes));
    this.counter.count += 1;
    if (this.loseEvery > 0 && this.counter.count % this.loseEvery === 0) return true;
    const copy = Buffer.from(bytes);
    const peer = this.peer;
    if (peer !== null) setImmediate(() => peer.onFrame?.(copy));
    return true;
  }

  close() {
    this.closed = true;
  }
}

function memoryPair(loseEvery = 0) {
  const counter = { count: 0 };
  const a = new MemoryLink(counter, loseEvery);
  const b = new MemoryLink(counter, loseEvery);
  a.peer = b;
  b.peer = a;
  return [a, b];
}

const SETTINGS = { chunk: 64, rto: 5, heartbeat: null, dead: null };

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

const tick = () => new Promise((resolve) => setImmediate(resolve));

describe('datagram frames', () => {
  it('round trip', () => {
    const bytes = frame(DATA, 7, Buffer.from('links'));
    assert.equal(bytes.length, HEADER + 5);
    const parsed = parseFrame(bytes);
    assert.deepEqual([parsed.kind, parsed.seq, parsed.payload.toString()], [DATA, 7, 'links']);
    assert.equal(parseFrame(bytes.subarray(0, 4)), null);
    assert.deepEqual(frame(PING, -1), Buffer.from([PING, 0, 0, 0, 255, 255, 255, 255]));
    assert.equal(agreeChunk(4096, 1024), 1024);
    assert.equal(agreeChunk(4096), 4096);
    assert.equal(agreeChunk(1, 1), 16);
    assert.equal(agreeChunk(100_000, null), 65000);
  });
});

describe('datagram sessions', { timeout: 60_000 }, () => {
  it('survive loss', async () => {
    for (const loseEvery of [0, 3, 7]) {
      const [left, right] = memoryPair(loseEvery);
      const a = new IpcStream(Session.start('test', left, SETTINGS));
      const b = new IpcStream(Session.start('test', right, SETTINGS));
      assert.equal(a.transport.chunk, 64);
      const sent = payload(5_000, loseEvery);
      const writing = (async () => {
        await a.write(sent);
        await a.closeWrite();
      })();
      const received = await readToEnd(b);
      await writing;
      assert.ok(received.equals(sent), `lose every ${loseEvery}`);
      a.shutdown();
      b.shutdown();
      await Promise.all([a.transport.stopped, b.transport.stopped]);
      assert.ok(left.closed && right.closed);
    }
  });

  it('end the stream when the peer hangs up instead of acknowledging the end', async () => {
    const [link, peer] = memoryPair();
    const received = [];
    let accepted = 0;
    peer.onFrame = (bytes) => {
      const { kind, seq, payload: data } = parseFrame(bytes);
      if (kind === DATA) {
        if (seq === accepted + 1) {
          accepted = seq;
          received.push(data);
        }
        peer.send(frame(ACK, seq));
      } else if (kind === EOF) {
        peer.send(frame(ABORT, 0));
      }
    };
    const a = new IpcStream(Session.start('test', link, SETTINGS));
    await a.write(Buffer.from('links'));
    await a.closeWrite();
    assert.equal(Buffer.concat(received).toString(), 'links');
    await assert.rejects(a.write(Buffer.from('more')), { kind: 'BrokenPipe' });
    await a.closeWrite();
    // A hung-up session ends without lingering or sending ABORT.
    const before = link.sent.length;
    a.shutdown();
    await a.transport.stopped;
    assert.equal(link.sent.length, before);
  });

  it('end the stream when the peer removes the link before acknowledging the end', async () => {
    const [link, peer] = memoryPair();
    const received = [];
    let accepted = 0;
    peer.onFrame = (bytes) => {
      const { kind, seq, payload: data } = parseFrame(bytes);
      if (kind === DATA) {
        if (seq === accepted + 1) {
          accepted = seq;
          received.push(data);
        }
        peer.send(frame(ACK, seq));
      } else if (kind === EOF) {
        // The end is read; the queues are removed with the ACK unread.
        link.gone = true;
      }
    };
    const a = new IpcStream(Session.start('test', link, SETTINGS));
    await a.write(Buffer.from('links'));
    await a.closeWrite();
    assert.equal(Buffer.concat(received).toString(), 'links');
    await assert.rejects(a.write(Buffer.from('more')), { kind: 'BrokenPipe' });
    a.shutdown();
    await a.transport.stopped;
  });

  it('fail a write or an end whose link fails otherwise', async () => {
    const [vanished] = memoryPair();
    const writer = new IpcStream(Session.start('test', vanished, SETTINGS));
    vanished.gone = true;
    await assert.rejects(writer.write(Buffer.from('links')), { kind: 'ConnectionReset' });
    writer.shutdown();
    await writer.transport.stopped;

    const [broken] = memoryPair();
    const ender = new IpcStream(Session.start('test', broken, SETTINGS));
    broken.failing = true;
    await assert.rejects(ender.closeWrite(), { message: 'the link failed' });
    ender.shutdown();
    await ender.transport.stopped;
  });

  it('acknowledge frames out of order as Rust does', async () => {
    const [link, peer] = memoryPair();
    peer.onFrame = () => {};
    const session = Session.start('test', link, { ...SETTINGS, chunk: 16 });
    const inject = async (kind, seq, data = Buffer.alloc(0)) => {
      link.onFrame(frame(kind, seq, data));
      await tick();
    };
    const acks = () => link.sent.filter((sent) => sent.kind === ACK).map((sent) => sent.seq);
    link.onFrame(Buffer.alloc(3));
    await inject(DATA, 1, Buffer.from('a'));
    await inject(DATA, 2, Buffer.from('b'));
    await inject(DATA, 2, Buffer.from('b'));
    await inject(DATA, 1, Buffer.from('a'));
    await inject(DATA, 5, Buffer.from('e'));
    await inject(DATA, 3, Buffer.alloc(17));
    await inject(PING, 0);
    await inject(ACK, 0xffffffff);
    assert.deepEqual(acks(), [1, 2, 2, 2]);
    assert.equal((await session.read(10, null)).toString(), 'ab');
    await inject(EOF, 3);
    await inject(DATA, 4, Buffer.from('d'));
    assert.deepEqual(acks(), [1, 2, 2, 2, 3]);
    assert.equal((await session.read(10, null)).length, 0);
    session.shutdown();
    session.shutdown();
    await session.stopped;
    // A stopped session ignores what still arrives.
    await inject(DATA, 4, Buffer.from('d'));
    assert.deepEqual(acks(), [1, 2, 2, 2, 3]);
  });

  it('stop delivering beyond the window until the reader catches up', async () => {
    const [link, peer] = memoryPair();
    peer.onFrame = () => {};
    const session = Session.start('test', link, { ...SETTINGS, chunk: 65000 });
    const chunk = Buffer.alloc(65000, 1);
    for (let seq = 1; seq <= 18; seq += 1) link.onFrame(frame(DATA, seq, chunk));
    await tick();
    const acks = link.sent.filter((sent) => sent.kind === ACK).map((sent) => sent.seq);
    // The 18th frame finds more than a window queued: neither taken nor acknowledged.
    assert.deepEqual(acks.slice(-2), [16, 17]);
    assert.equal(acks.length, 17);
    session.shutdown();
    link.failing = true;
    await session.stopped;
  });

  it('time reads out, report receive errors and refuse writes after the end', async () => {
    const [link, peer] = memoryPair();
    peer.onFrame = (bytes) => peer.send(frame(ACK, parseFrame(bytes).seq));
    const session = Session.start('test', link, SETTINGS);
    await assert.rejects(session.read(1, 10), { kind: 'TimedOut' });
    await session.closeWrite();
    await assert.rejects(session.write(Buffer.from('x')), {
      kind: 'BrokenPipe',
      message: 'the outgoing direction was closed',
    });
    link.onError(new Error('the socket broke'));
    await assert.rejects(session.read(1, null), { kind: 'ConnectionReset' });
    await assert.rejects(session.write(Buffer.from('x')), { kind: 'BrokenPipe' });
    // The housekeeping of a hung-up session does nothing.
    await sleep(120);
    session.shutdown();
    assert.equal((await session.read(1, null)).length, 0);
    await assert.rejects(session.write(Buffer.from('x')), {
      message: 'the outgoing direction was closed',
    });
    await session.stopped;
  });

  it('fail a write whose peer hangs up while it waits', async () => {
    const [link, peer] = memoryPair();
    peer.onFrame = () => {};
    const session = Session.start('test', link, SETTINGS);
    const writing = session.write(Buffer.from('x'));
    await sleep(20);
    link.onFrame(frame(ABORT, 0));
    link.onFrame(frame(ABORT, 0));
    await assert.rejects(writing, { kind: 'BrokenPipe', message: 'the peer no longer reads' });
    await assert.rejects(session.write(Buffer.from('y')), {
      kind: 'BrokenPipe',
      message: 'the peer no longer reads',
    });
    session.shutdown();
    await session.stopped;
  });

  it('linger at most a second for the end, then hang up', async () => {
    const [link, peer] = memoryPair();
    peer.onFrame = () => {};
    const session = Session.start('test', link, SETTINGS);
    const writing = session.write(Buffer.from('x'));
    await sleep(10);
    const started = performance.now();
    session.shutdown();
    await assert.rejects(writing, { kind: 'BrokenPipe', message: 'the connection was closed' });
    await session.stopped;
    assert.deepEqual(
      link.sent.slice(-4).map((sent) => sent.kind),
      [EOF, ABORT, ABORT, ABORT],
    );
    assert.ok(performance.now() - started >= 900);
  });

  it('send heartbeats and notice dead or silent peers', async () => {
    const [link, peer] = memoryPair();
    const kinds = [];
    peer.onFrame = (bytes) => kinds.push(parseFrame(bytes).kind);
    const beating = Session.start('test', link, { ...SETTINGS, heartbeat: 10 });
    await sleep(60);
    assert.ok(kinds.includes(PING));
    link.failing = true;
    await sleep(30);
    link.failing = false;
    link.peerAlive = () => false;
    await assert.rejects(beating.read(1, 1000), { kind: 'ConnectionReset' });
    beating.shutdown();
    await beating.stopped;

    const [quiet] = memoryPair();
    quiet.peerAlive = () => true;
    const silent = Session.start('test', quiet, { ...SETTINGS, dead: 50 });
    await assert.rejects(silent.read(1, 2000), { kind: 'ConnectionReset' });
    silent.shutdown();
    await silent.stopped;
  });

  it('swallow failed acknowledgements and aborts', async () => {
    const [link] = memoryPair();
    const session = Session.start('test', link, SETTINGS);
    link.failing = true;
    link.onFrame(frame(DATA, 1, Buffer.from('a')));
    assert.equal((await session.read(1, null)).toString(), 'a');
    session.shutdown();
    await session.stopped;
  });
});
