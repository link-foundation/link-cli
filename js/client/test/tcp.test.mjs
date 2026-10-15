import assert from 'node:assert/strict';
import net from 'node:net';
import test from 'node:test';
import { encodeMessage, MessageReader } from '../src/framing.js';
import { connectTcp, parseAddress, TcpTransport } from '../src/tcp.js';

/**
 * A fake peer speaking the framing: `answer(text, connection)` returns the
 * reply text, a promise of it, or undefined to stay silent.
 */
async function fakeServer(answer, { raw } = {}) {
  const connections = new Set();
  const server = net.createServer((socket) => {
    const connection = { socket, requests: [], ended: false };
    connections.add(connection);
    socket.on('end', () => {
      connection.ended = true;
    });
    socket.on('error', () => {});
    if (raw) {
      raw(socket);
      return;
    }
    const reader = new MessageReader();
    let order = Promise.resolve();
    socket.on('data', (chunk) => {
      for (const text of reader.push(chunk)) {
        connection.requests.push(text);
        const reply = answer(text, connection);
        // Replies stay in request order, like the real server's.
        order = order.then(async () => {
          const value = await reply;
          if (value !== undefined && !socket.destroyed) socket.write(encodeMessage(value));
        });
      }
    });
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  return {
    address: `127.0.0.1:${server.address().port}`,
    connections,
    async close() {
      for (const { socket } of connections) socket.destroy();
      await new Promise((resolve) => server.close(resolve));
    },
  };
}

const countZero = (text) => (text === '(count: (0))' ? '(count: 0)' : undefined);

function deferred() {
  let resolve;
  const promise = new Promise((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

test('addresses parse with IPv4, IPv6 and objects', () => {
  assert.deepEqual(parseAddress('127.0.0.1:8080'), { host: '127.0.0.1', port: 8080 });
  assert.deepEqual(parseAddress('[::1]:9'), { host: '::1', port: 9 });
  assert.deepEqual(parseAddress({ port: 7 }), { host: '127.0.0.1', port: 7 });
  assert.throws(() => parseAddress('nowhere'), TypeError);
  assert.throws(() => parseAddress('host:70000'), TypeError);
});

test('pipelined replies are matched to their requests in order', async () => {
  const server = await fakeServer((text) => {
    if (text === '(count: (0))') return '(count: 0)';
    const index = /(\d+)\)+$/.exec(text)[1];
    // Answer later requests sooner; order must still hold.
    return new Promise((resolve) => setTimeout(() => resolve(`(count: ${index})`), Number(index) % 3));
  });
  const client = await connectTcp(server.address, { maxInFlight: 8 });
  const counts = await Promise.all(Array.from({ length: 200 }, (_, index) => client.count([index + 1])));
  assert.deepEqual(counts, Array.from({ length: 200 }, (_, index) => BigInt(index + 1)));
  assert.equal(client.transport.stats.requests, 201);
  await client.close();
  await server.close();
});

test('aborting a queued request drops it; aborting a running one closes the connection', async () => {
  const held = [];
  const server = await fakeServer((text) => {
    if (text === '(count: (0))') return '(count: 0)';
    const reply = deferred();
    held.push(reply);
    return reply.promise;
  });
  const client = await connectTcp(server.address, { maxInFlight: 1 });
  const [connection] = server.connections;

  const first = client.count([1]);
  const queued = new AbortController();
  const second = client.count([2], { signal: queued.signal });
  queued.abort();
  await assert.rejects(second, { name: 'AbortError', outcome: 'not-sent' });
  while (held.length < 1) await new Promise((resolve) => setTimeout(resolve, 1));
  held[0].resolve('(count: 1)');
  assert.equal(await first, 1n);
  assert.deepEqual(connection.requests, ['(count: (0))', '(count: ((1)))']);

  const running = new AbortController();
  const third = client.count([3], { signal: running.signal });
  const fourth = client.count([4]);
  while (held.length < 2) await new Promise((resolve) => setTimeout(resolve, 1));
  running.abort();
  await assert.rejects(third, { name: 'AbortError', outcome: 'unknown' });
  await assert.rejects(fourth, { name: 'ConnectionClosedError', outcome: 'not-sent' });
  await assert.rejects(client.count([5]), { name: 'ConnectionClosedError', outcome: 'not-sent' });
  await client.close();
  await server.close();
});

test('an absolute deadline ends a request the server never answers', async () => {
  const server = await fakeServer(countZero);
  const client = await connectTcp(server.address);
  const started = Date.now();
  await assert.rejects(client.count([1], { deadline: Date.now() + 50 }), { name: 'DeadlineExceededError', outcome: 'unknown' });
  assert.ok(Date.now() - started < 2000);
  await client.close();
  await server.close();
});

test('request and reply sizes are bounded', async () => {
  const server = await fakeServer((text) => (text === '(count: (0))' ? '(count: 0)' : `(name: ${'x'.repeat(200)})`));
  const client = await connectTcp(server.address, { maxRequestBytes: 64, maxReplyBytes: 100 });
  await assert.rejects(client.setName(1, 'y'.repeat(100)), { name: 'LimitExceededError', outcome: 'not-sent' });
  assert.equal(client.transport.stats.requests, 1);
  await assert.rejects(client.getName(1), { name: 'LimitExceededError', outcome: 'unknown' });
  assert.equal(client.transport.closed, true);
  await client.close();
  await server.close();
});

test('writes wait for the socket to drain', async () => {
  let paused;
  const server = await fakeServer(countZero, {
    raw(socket) {
      const reader = new MessageReader({ maxBytes: 2 ** 30 });
      paused = socket;
      socket.on('data', (chunk) => {
        for (const text of reader.push(chunk)) socket.write(encodeMessage(text === '(count: (0))' ? '(count: 0)' : '(count: 7)'));
      });
    },
  });
  const client = await connectTcp(server.address, { maxInFlight: 1000, maxRequestBytes: 2 ** 30 });
  paused.pause();
  const big = `(get-by-name: ${'z'.repeat(1024 * 1024)})`;
  const pending = Array.from({ length: 64 }, () => client.transport.send(big));
  await new Promise((resolve) => setTimeout(resolve, 50));
  // The kernel buffers hold a few MiB at most: most requests are still queued, not written.
  assert.ok(client.transport.stats.requests < 64, `wrote ${client.transport.stats.requests} requests into a stalled socket`);
  paused.resume();
  assert.equal((await Promise.all(pending)).length, 64);
  assert.equal(client.transport.stats.requests, 65);
  await client.close();
  await server.close();
});

test('a peer that does not speak the links protocol is refused', async () => {
  const http = await fakeServer(null, {
    raw(socket) {
      socket.once('data', () => socket.end('HTTP/1.1 400 Bad Request\r\n\r\n'));
    },
  });
  await assert.rejects(connectTcp(http.address, { timeout: 2000 }), (error) => ['ProtocolMismatchError', 'ConnectionClosedError'].includes(error.name));
  await http.close();

  const other = await fakeServer(() => "(error: 'unknown command')");
  await assert.rejects(connectTcp(other.address), { name: 'ProtocolMismatchError', message: /unknown command/ });
  await other.close();

  const wrongShape = await fakeServer(() => '(1: 1 1)');
  await assert.rejects(connectTcp(wrongShape.address), { name: 'ProtocolMismatchError' });
  await wrongShape.close();

  const binary = await fakeServer(null, {
    raw(socket) {
      socket.once('data', () => socket.write(Uint8Array.of(0x20, 0, 0, 0, 0xff, 10, 46, 10)));
    },
  });
  await assert.rejects(connectTcp(binary.address), { name: 'ProtocolMismatchError' });
  await binary.close();
});

test('connecting fails as not-sent when nobody listens or time runs out', async () => {
  const probe = net.createServer();
  await new Promise((resolve) => probe.listen(0, '127.0.0.1', resolve));
  const { port } = probe.address();
  await new Promise((resolve) => probe.close(resolve));
  await assert.rejects(connectTcp(`127.0.0.1:${port}`), { name: 'ConnectionClosedError', outcome: 'not-sent' });
  const silent = await fakeServer(() => undefined);
  await assert.rejects(connectTcp(silent.address, { timeout: 50 }), { name: 'DeadlineExceededError' });
  await silent.close();
});

test('close waits for answers, then ends only an owned connection', async () => {
  const server = await fakeServer((text) => new Promise((resolve) => setTimeout(() => resolve(text === '(count: (0))' ? '(count: 0)' : '(count: 3)'), 20)));
  const client = await connectTcp(server.address);
  const [connection] = server.connections;
  const pending = client.count([3]);
  await client.close();
  assert.equal(await pending, 3n);
  await new Promise((resolve) => setTimeout(resolve, 20));
  assert.equal(connection.ended, true);

  const socket = net.connect(parseAddress(server.address));
  await new Promise((resolve) => socket.once('connect', resolve));
  const borrowed = new TcpTransport(socket, { owned: false });
  assert.equal(await borrowed.send('(count: (0))'), '(count: 0)');
  await borrowed.close();
  assert.equal(socket.destroyed, false);
  assert.equal(socket.listenerCount('data'), 0);
  socket.destroy();
  await server.close();
});
