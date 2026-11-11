// Interop tests of the IPC endpoints against the Rust reference server: each
// kind serves a store with `clink --db FILE --serve ENDPOINT`, and the client
// reads, writes and receives a subscription push through it. They run when
// CLINK_BIN names a clink binary built from rust/, e.g.
//   CLINK_BIN=/path/to/clink node --test test/ipc.test.mjs
import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { after, before, describe, test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { connectIpc, connectStream, IpcTransport, parseEndpoint, supports } from '../src/ipc.js';

const CLINK = process.env.CLINK_BIN;
const RELAY = fileURLToPath(new URL('./support/ipc-relay.mjs', import.meta.url));
const LINUX = process.platform === 'linux';

/** The `--serve` endpoint of each kind, inside a fresh directory. */
const KINDS = {
  tcp: () => 'tcp:127.0.0.1:0',
  udp: () => 'udp:127.0.0.1:0?rto=20',
  unix: (dir) => `unix:${dir}/links.sock`,
  'unix (abstract)': () => `unix:@link-cli-test-${process.pid}`,
  pipe: (dir) =>
    process.platform === 'win32' ? `pipe:link-cli-test-${process.pid}` : `pipe:${dir}/r`,
  fifo: (dir) => `fifo:${dir}/r`,
  file: (dir) => `file:${dir}/r`,
  mmap: (dir) => `mmap:${dir}/r`,
  signal: (dir) => `signal:${dir}/r`,
  shm: (dir) => `shm:${dir}/r`,
  mq: (dir) => `mq:${dir}/r`,
  sysvmq: (dir) => `sysvmq:${dir}/r`,
};

const kindOf = (name) => name.split(' ')[0];

function skipReason(name) {
  if (!CLINK) return 'CLINK_BIN is not set';
  if (name === 'unix (abstract)' && !LINUX) return 'abstract sockets exist only on Linux';
  if (!supports(kindOf(name))) return `${kindOf(name)}: is not supported on this platform`;
  return undefined;
}

/** Starts `clink --serve` and resolves once it prints the endpoint it listens on. */
function serve(dir, endpoint) {
  const child = spawn(CLINK, ['--db', path.join(dir, 'db.links'), '--serve', endpoint], {
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  const exited = new Promise((resolve) => child.once('exit', resolve));
  let errors = '';
  child.stderr.on('data', (chunk) => {
    errors = (errors + chunk).slice(-4096);
  });
  const listening = new Promise((resolve, reject) => {
    let out = '';
    child.stdout.on('data', (chunk) => {
      out += chunk;
      const match = /clink server listening on (.*)\r?\n/.exec(out);
      if (match) resolve(match[1]);
    });
    child.once('error', reject);
    child.once('exit', (code) => reject(new Error(`clink --serve exited (${code}): ${errors}`)));
  });
  return {
    child,
    listening,
    async stop() {
      if (child.exitCode === null && child.signalCode === null) child.kill();
      await exited;
    },
  };
}

/** Runs `clink` with arguments and resolves to its standard output. */
const run = (args) =>
  new Promise((resolve, reject) => {
    execFile(CLINK, args, { timeout: 30_000 }, (error, stdout, stderr) => {
      if (error) reject(new Error(`${error.message}\n${stderr}`));
      else resolve(stdout);
    });
  });

const waitFor = async (check, ms) => {
  const until = Date.now() + ms;
  while (!check()) {
    if (Date.now() > until) return false;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  return true;
};

for (const [name, endpointIn] of Object.entries(KINDS)) {
  const reason = skipReason(name);
  describe(`ipc ${name}`, { skip: reason, timeout: 60_000 }, () => {
    let dir;
    let server;
    let endpoint;
    const clients = [];

    before(async () => {
      dir = fs.mkdtempSync(path.join(os.tmpdir(), 'link-cli-ipc-'));
      server = serve(dir, endpointIn(dir));
      endpoint = await server.listening;
    });

    after(async () => {
      for (const client of clients) await client.close().catch(() => {});
      await server?.stop();
      if (dir) fs.rmSync(dir, { recursive: true, force: true });
    });

    const open = async () => {
      const client = await connectIpc(endpoint, { timeout: 15_000 });
      clients.push(client);
      return client;
    };

    test('text CRUD and a subscription push', async () => {
      const client = await open();
      assert.equal(client.transport.capabilities.transport, kindOf(name));
      assert.equal(client.transport.endpoint, parseEndpoint(endpoint).toString());
      const link = await client.create(1, 1);
      assert.deepEqual(link, { index: 1n, source: 1n, target: 1n });
      assert.equal(await client.count([]), 1n);
      const updated = await client.update(1, 1, 2);
      assert.deepEqual(updated[0].after, { index: 1n, source: 1n, target: 2n });

      const pushes = [];
      client.onNotification((notification) => pushes.push(notification));
      await client.subscribe([]);
      // A connection's own writes are not pushed to it: write through another one.
      const other = await open();
      await other.create(2, 2);
      await other.close();
      assert.ok(await waitFor(() => pushes.length > 0, 5_000), 'a push arrives');

      const deleted = await client.delete(1);
      assert.deepEqual(deleted[0].before, { index: 1n, source: 1n, target: 2n });
      assert.equal(await client.count([]), 1n);
    });

    test('large messages cross in both directions', async () => {
      const client = await open();
      const link = await client.create(3, 3);
      // Past one ring capacity and many datagram chunks; the store, not the
      // transport, makes much longer names slow.
      const big = 'x'.repeat(70_000);
      await client.setName(link.index, big);
      assert.equal(await client.getName(link.index), big);
    });

    test('a raw stream half-closes and the server answers first', async () => {
      const stream = await connectStream(endpoint, { timeout: 15_000 });
      const transport = new IpcTransport(stream, { kind: kindOf(name) });
      assert.equal(transport.kind, kindOf(name));
      await transport.close();
      assert.ok(await waitFor(() => stream.destroyed, 5_000), 'the stream closes');
    });
  });
}

/** `clink --connect exec:RELAY --protocol binary`: Rust speaks binary through the JS stream. */
describe('the binary protocol through a JS relay', { timeout: 60_000 }, () => {
  for (const name of ['fifo', 'udp', 'shm']) {
    test(name, { skip: skipReason(name) }, async () => {
      const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'link-cli-ipc-'));
      const server = serve(dir, KINDS[name](dir));
      try {
        const endpoint = await server.listening;
        const via = `exec:${JSON.stringify([process.execPath, RELAY, endpoint])}`;
        const created = await run(['--connect', via, '--protocol', 'binary', '() ((1 1))']);
        assert.match(created, /\(\) \(\(1: 1 1\)\)/);
        const read = await run([
          '--connect',
          via,
          '--protocol',
          'binary',
          '((($i: $s $t)) (($i: $s $t)))',
        ]);
        assert.match(read, /\(1: 1 1\)/);
      } finally {
        await server.stop();
        fs.rmSync(dir, { recursive: true, force: true });
      }
    });
  }
});

test('the clink bridge reports a listener that is missing', { timeout: 60_000 }, async (t) => {
  if (!CLINK || !supports('mq') || !LINUX) {
    t.skip(CLINK ? 'mq: is not supported on this platform' : 'CLINK_BIN is not set');
    return;
  }
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'link-cli-ipc-'));
  try {
    await assert.rejects(connectIpc(`mq:${dir}/absent?timeout=500`), /cannot connect to mq:/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
