// Unit tests of ipc-rendezvous.js, the rendezvous directory of
// rust/src/protocol/ipc/rendezvous.rs: claiming a directory (a live
// listener refuses, a stale one is cleared), malformed and mismatched
// requests, the client's checks (no listener, another version or kind, a
// gone listener), answers, refusals, timeouts and `accept_with`.
import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as os from 'node:os';
import { join } from 'node:path';
import { after, describe, it } from 'node:test';
import { Endpoint } from '../src/ipc-endpoint.js';
import { JsonObject } from '../src/ipc-json.js';
import {
  acceptWith,
  Backoff,
  canonicalKind,
  readText,
  Rendezvous,
  RendezvousListener,
  Request,
  writeAtomically,
} from '../src/ipc-rendezvous.js';
import { setTrace } from '../src/ipc-trace.js';
import { diagnostics } from '../src/protocol-errors.js';

const root = fs.mkdtempSync(join(os.tmpdir(), 'ipc-rendezvous-'));
after(() => fs.rmSync(root, { recursive: true, force: true }));
let dirs = 0;
const freshDir = () => join(root, `d${(dirs += 1)}`);

// No process has this id: `kill(pid, 0)` fails with `ESRCH`.
const DEAD = 0x7ffffffe;
const ID = '0123456789abcdef0123';
const POSIX_USER =
  process.platform !== 'win32' && typeof process.getuid === 'function' && process.getuid() !== 0;

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

const listenerJson = (fields) =>
  Object.entries(fields)
    .reduce((object, [name, value]) => object.with(name, value), new JsonObject())
    .toJson();

describe('rendezvous files', () => {
  it('back off, write atomically and read UTF-8', async () => {
    const backoff = new Backoff(0);
    assert.deepEqual([backoff.limit, backoff.current], [0.05, 0.05]);
    const doubling = new Backoff(3);
    await doubling.sleep();
    await doubling.sleep();
    await doubling.sleep();
    assert.equal(doubling.current, 3);
    const dir = freshDir();
    fs.mkdirSync(join(dir, 'taken'), { recursive: true });
    fs.writeFileSync(join(dir, 'taken', 'inside'), '');
    assert.throws(() => writeAtomically(join(dir, 'taken'), 'x'));
    assert.equal(fs.existsSync(join(dir, 'taken.tmp')), false);
    writeAtomically(join(dir, 'file'), 'é');
    assert.equal(readText(join(dir, 'file')), 'é');
    fs.writeFileSync(join(dir, 'bad'), Buffer.from([0xff]));
    assert.throws(() => readText(join(dir, 'bad')), {
      kind: 'InvalidData',
      message: 'stream did not contain valid UTF-8',
    });
    assert.deepEqual(['pipe', 'mmap'].map(canonicalKind), ['fifo', 'mmap']);
  });

  it('answer requests once', () => {
    const dir = freshDir();
    fs.mkdirSync(dir);
    const request = new Request(ID, new JsonObject(), dir);
    assert.equal(request.pid(), 0);
    request.ack(new JsonObject().with('extra', 'yes'));
    const ack = JsonObject.parse(readText(request.path('ack')));
    assert.deepEqual(
      [ack.number('v'), ack.number('pid'), ack.text('extra')],
      [1n, BigInt(process.pid), 'yes'],
    );
    request.finish();
    assert.equal(fs.existsSync(request.path('ack')), true);
    fs.rmSync(request.path('ack'));
    const unanswered = new Request(ID, new JsonObject(), dir);
    unanswered.finish();
    assert.equal(
      JsonObject.parse(readText(unanswered.path('ack'))).text('error'),
      'the listener could not set the connection up',
    );
    // A refusal that cannot be written is dropped.
    new Request(ID, new JsonObject(), join(dir, 'missing')).refuse('no');
  });
});

describe('rendezvous listener', { timeout: 30_000 }, () => {
  it('refuses a live listener and clears a stale one', () => {
    const dir = freshDir();
    fs.mkdirSync(dir);
    fs.writeFileSync(
      join(dir, 'listener.json'),
      listenerJson({ v: 1, kind: 'mmap', pid: process.pid }),
    );
    const endpoint = Endpoint.parse(`mmap:${dir}`);
    assert.throws(() => RendezvousListener.bind(endpoint, 'mmap'), {
      kind: 'AddrInUse',
      message: `process ${process.pid} already listens on \`${dir}\``,
    });
    fs.writeFileSync(join(dir, 'listener.json'), listenerJson({ v: 1, kind: 'mmap', pid: DEAD }));
    for (const name of ['a.req', 'a.ack', 'a.ring', 'keep.txt'])
      fs.writeFileSync(join(dir, name), '');
    const listener = RendezvousListener.bind(endpoint, 'pipe');
    assert.deepEqual(fs.readdirSync(dir).sort(), ['keep.txt', 'listener.json']);
    const owner = JsonObject.parse(readText(join(dir, 'listener.json')));
    assert.deepEqual([owner.text('kind'), owner.number('pid')], ['fifo', BigInt(process.pid)]);
    assert.equal(listener.endpoint(), `mmap:${dir}`);
    // A directory that is not empty stays.
    assert.equal(listener.closed, false);
    listener.close();
    listener.close();
    assert.equal(listener.closed, true);
    assert.deepEqual(fs.readdirSync(dir), ['keep.txt']);
  });

  it(
    'claims a directory it cannot list',
    { skip: !POSIX_USER && 'needs POSIX permissions that bind an unprivileged user' },
    () => {
      const dir = freshDir();
      fs.mkdirSync(dir, { mode: 0o300 });
      fs.chmodSync(dir, 0o300);
      try {
        const listener = RendezvousListener.bind(Endpoint.parse(`file:${dir}`), 'file');
        listener.close();
      } finally {
        if (fs.existsSync(dir)) fs.chmodSync(dir, 0o700);
      }
    },
  );

  it('refuses malformed and mismatched requests', async () => {
    const dir = freshDir();
    const listener = RendezvousListener.bind(Endpoint.parse(`mmap:${dir}?accept-poll=1`), 'mmap');
    const good = { v: 1, kind: 'mmap', pid: 7 };
    const requests = {
      '00000000000000000000': 'not json',
      '00000000000000000001': listenerJson({ ...good, v: 2 }),
      '00000000000000000002': listenerJson({ ...good, kind: 'file' }),
      '00000000000000000003': listenerJson({ v: 1, kind: 'mmap' }),
      '0000short': listenerJson(good),
      [ID]: listenerJson(good),
    };
    for (const [id, text] of Object.entries(requests))
      fs.writeFileSync(join(dir, `${id}.req`), text);
    // What cannot be read stays for later; other files are no requests.
    fs.mkdirSync(join(dir, '00.req'));
    fs.writeFileSync(join(dir, 'other.txt'), '');
    let request;
    const lines = await traced(async () => {
      request = await listener.next();
    });
    assert.equal(request.id, ID);
    assert.equal(request.pid(), 7);
    const refused = Object.keys(requests).filter((id) => id !== ID);
    assert.deepEqual(lines, [
      ...refused.map((id) => `[ipc] mmap refused malformed request ${id}.req`),
      `[ipc] mmap request ${ID} from process 7`,
    ]);
    for (const id of refused) {
      const answer = JsonObject.parse(readText(join(dir, `${id}.ack`)));
      assert.equal(answer.text('error'), 'malformed or mismatched request');
    }
    const waiting = listener.next();
    setTimeout(() => listener.close(), 20);
    assert.equal(await waiting, null);
    assert.equal(await listener.next(), null);
  });

  it('leaves a listener file it does not own', () => {
    const dir = freshDir();
    const listener = RendezvousListener.bind(Endpoint.parse(`mmap:${dir}`), 'mmap');
    fs.writeFileSync(join(dir, 'listener.json'), listenerJson({ v: 1, kind: 'mmap', pid: DEAD }));
    listener.close();
    assert.deepEqual(fs.readdirSync(dir), ['listener.json']);
    const gone = RendezvousListener.bind(Endpoint.parse(`mmap:${dir}`), 'mmap');
    fs.rmSync(dir, { recursive: true });
    gone.close();
  });
});

describe('rendezvous client', { timeout: 30_000 }, () => {
  it('checks the listener before asking', () => {
    const dir = freshDir();
    const endpoint = Endpoint.parse(`mmap:${dir}`);
    assert.throws(() => Rendezvous.begin(endpoint, 'mmap'), {
      kind: 'ConnectionRefused',
      message: `nobody listens on \`${dir}\`: No such file or directory (os error 2)`,
    });
    fs.mkdirSync(dir);
    const cases = [
      [{ v: 2, kind: 'mmap', pid: process.pid }, `\`${dir}\` uses another rendezvous version`],
      [{ v: 1, pid: process.pid }, `\`${dir}\` listens for \`:\` connections, not \`mmap:\``],
      [
        { v: 1, kind: 'file', pid: 1 },
        `\`${dir}\` listens for \`file:\` connections, not \`mmap:\``,
      ],
      [{ v: 1, kind: 'mmap' }, `the listener of \`${dir}\` (process 0) is gone`],
      [{ v: 1, kind: 'mmap', pid: DEAD }, `the listener of \`${dir}\` (process ${DEAD}) is gone`],
    ];
    for (const [fields, message] of cases) {
      fs.writeFileSync(join(dir, 'listener.json'), listenerJson(fields));
      assert.throws(() => Rendezvous.begin(endpoint, 'mmap'), {
        kind: 'ConnectionRefused',
        message,
      });
    }
    fs.writeFileSync(join(dir, 'listener.json'), listenerJson({ v: 1, kind: 'fifo', pid: 5 }));
    const rendezvous = Rendezvous.begin(Endpoint.parse(`fifo:${dir}?poll=3`), 'pipe', () => true);
    assert.deepEqual([rendezvous.serverPid, rendezvous.poll], [5, 3]);
    assert.match(rendezvous.id, /^[0-9a-f]{20}$/);
  });

  it('waits for an answer, a refusal, a gone listener or the timeout', async () => {
    const dir = freshDir();
    const listener = RendezvousListener.bind(Endpoint.parse(`mmap:${dir}?accept-poll=1`), 'mmap');
    const begin = (query = '') =>
      Rendezvous.begin(Endpoint.parse(`mmap:${dir}?poll=1${query}`), 'mmap');

    const asking = begin().request(new JsonObject().with('size', 9));
    const request = await listener.next();
    assert.equal(request.fields.number('size'), 9n);
    request.ack(new JsonObject().with('ok', 'yes'));
    const answer = await asking;
    assert.equal(answer.text('ok'), 'yes');
    assert.equal(fs.existsSync(request.path('ack')), false);

    const refusing = begin().request(new JsonObject());
    (await listener.next()).refuse('full');
    await assert.rejects(refusing, {
      kind: 'ConnectionRefused',
      message: 'the listener refused: full',
    });

    const lonely = begin();
    await assert.rejects(
      lonely.request(new JsonObject(), () => false),
      {
        kind: 'ConnectionRefused',
        message: `the listener (process ${process.pid}) is gone`,
      },
    );
    assert.equal(fs.existsSync(lonely.path('req')), false);

    await assert.rejects(begin('&timeout=30').request(new JsonObject()), {
      kind: 'TimedOut',
      message: `no answer from the listener of \`${dir}\` in time`,
    });
    listener.close();
  });
});

describe('accept_with', { timeout: 30_000 }, () => {
  it('refuses what fails to open and accepts what opens', async () => {
    const dir = freshDir();
    const listener = RendezvousListener.bind(Endpoint.parse(`file:${dir}?accept-poll=1`), 'file');
    const begin = () => Rendezvous.begin(Endpoint.parse(`file:${dir}?poll=1`), 'file');
    const failing = begin().request(new JsonObject());
    let opens = 0;
    const stream = { shutdown: () => {} };
    let accepted;
    const lines = await traced(async () => {
      accepted = await acceptWith(listener, 'file', async (request) => {
        opens += 1;
        if (opens === 1) {
          // The next client asks while this one is refused.
          begin()
            .request(new JsonObject())
            .catch(() => {});
          throw new Error('no room');
        }
        return { fields: new JsonObject().with('n', request.id.length), stream };
      });
    });
    assert.equal(accepted, stream);
    await assert.rejects(failing, { message: 'the listener refused: no room' });
    assert.match(lines.join('\n'), /\[ipc\] file refused [0-9a-f]{20}: no room/);
    assert.match(lines.at(-1), /^\[ipc\] file accepted [0-9a-f]{20} from process [0-9]+$/);

    // An answer that cannot be written shuts the opened stream down.
    const rendezvous = Rendezvous.begin(Endpoint.parse(`file:${dir}?poll=1&timeout=300`), 'file');
    fs.mkdirSync(rendezvous.path('ack'));
    fs.writeFileSync(join(rendezvous.path('ack'), 'inside'), '');
    rendezvous.request(new JsonObject()).catch(() => {});
    let shut = 0;
    await assert.rejects(
      acceptWith(listener, 'file', async () => ({
        fields: new JsonObject(),
        stream: { shutdown: () => shut++ },
      })),
    );
    assert.equal(shut, 1);
    fs.rmSync(rendezvous.path('ack'), { recursive: true });

    listener.close();
    assert.equal(await acceptWith(listener, 'file', async () => ({})), null);
  });
});
