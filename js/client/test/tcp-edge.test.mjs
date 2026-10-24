import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import net from 'node:net';
import test from 'node:test';
import { TcpTransport, verifyProtocol } from '../src/tcp.js';
import { MessageReader, encodeMessage } from '../src/framing.js';

class Socket extends EventEmitter {
  destroyed = false;
  writes = [];
  write(bytes) { this.writes.push(bytes); return true; }
  end() {}
  destroy() { if (!this.destroyed) { this.destroyed = true; this.emit('close'); } }
  setNoDelay(value) { this.noDelay = value; }
}

test('connect cancellation is not-sent and destroys a pending socket', async (t) => {
  const socket = new Socket();
  const connect = t.mock.method(net, 'connect', () => socket);
  await assert.rejects(TcpTransport.connect('localhost:1', { signal: AbortSignal.abort() }), { name: 'AbortError', outcome: 'not-sent' });
  assert.equal(connect.mock.callCount(), 0);
  const controller = new AbortController();
  const pending = TcpTransport.connect('localhost:1', { signal: controller.signal });
  controller.abort();
  await assert.rejects(pending, { name: 'AbortError', outcome: 'not-sent' });
  assert.equal(socket.destroyed, true);
  const connected = new Socket(); connect.mock.mockImplementation(() => connected);
  const ready = TcpTransport.connect('localhost:1'); connected.emit('connect');
  const transport = await ready;
  assert.equal(connected.noDelay, true); transport.destroy(); await transport.close();
});

test('socket errors classify every pipelined request and leave queued work not-sent', async () => {
  const socket = new Socket(); const transport = new TcpTransport(socket, { maxInFlight: 2 });
  await assert.rejects(transport.send('cancelled', { signal: AbortSignal.abort() }), { outcome: 'not-sent' });
  const first = transport.send('one'); const second = transport.send('two'); const queued = transport.send('three');
  socket.emit('error', new Error('connection reset'));
  await assert.rejects(first, { name: 'ConnectionClosedError', outcome: 'unknown' });
  await assert.rejects(second, { name: 'ConnectionClosedError', outcome: 'unknown' });
  await assert.rejects(queued, { name: 'ConnectionClosedError', outcome: 'not-sent' });
  assert.equal(socket.writes.length, 2);
  transport.destroy(); await transport.close();
});

test('unsolicited replies and decoder faults close the connection', async (t) => {
  const unsolicited = new Socket(); const idle = new TcpTransport(unsolicited);
  unsolicited.emit('data', encodeMessage('(count: 1)'));
  assert.equal(idle.closed, true); assert.equal(unsolicited.destroyed, true); await idle.close();
  const socket = new Socket(); const transport = new TcpTransport(socket);
  const pending = transport.send('request');
  t.mock.method(MessageReader.prototype, 'push', () => { throw new Error('decoder allocation failure'); });
  socket.emit('data', new Uint8Array([1]));
  await assert.rejects(pending, { name: 'MalformedMessageError', outcome: 'unknown', message: 'decoder allocation failure' });
  await transport.close();
  await assert.rejects(verifyProtocol({ send: async () => '(' }), { name: 'ProtocolMismatchError', message: /not LiNo/ });
});

test('graceful close forcibly ends an unresponsive owned peer after its grace period', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const socket = new Socket(); const transport = new TcpTransport(socket);
  const closing = transport.close();
  assert.equal(socket.destroyed, false);
  t.mock.timers.tick(1000);
  await closing;
  assert.equal(socket.destroyed, true);
});
