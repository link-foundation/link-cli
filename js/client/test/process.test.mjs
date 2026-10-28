import assert from 'node:assert/strict';
import test from 'node:test';
import { existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { startServer, connectProcess, ProcessTransport } from '../src/process.js';
import { LinksError, ConnectionClosedError } from '../src/errors.js';

const fixture = fileURLToPath(new URL('../../../experiments/issue-138/process-peer.mjs', import.meta.url));

function peer(mode) {
  const directory = mkdtempSync(join(tmpdir(), 'clink-process-'));
  const pidPath = join(directory, 'pid');
  return {
    options: { command: process.execPath, args: [fixture, mode, pidPath], db: join(directory, 'db'), timeout: 3000 },
    pid: () => Number(readFileSync(pidPath, 'utf8')),
    cleanup() {
      try { process.kill(this.pid(), 'SIGKILL'); } catch { /* Already reaped. */ }
      rmSync(directory, { recursive: true, force: true });
    },
  };
}

test('an invalid startup address cannot leak the owned process', async () => {
  const child = peer('invalid');
  try {
    await assert.rejects(startServer(child.options), /host:port/);
    assert.throws(() => process.kill(child.pid(), 0), { code: 'ESRCH' });
  } finally { child.cleanup(); }
});

test('startup validates inputs and cancellation before spawning', async () => {
  for (const options of [{}, { command: 'node' }, { command: 'node', db: 'db', args: [0] }])
    await assert.rejects(startServer(options), TypeError);
  const signal = AbortSignal.abort(new Error('cancelled'));
  await assert.rejects(startServer({ command: 'node', db: 'db', signal }), { name: 'AbortError', outcome: 'not-sent' });
  await assert.rejects(startServer({ command: '/missing/clink-command', db: 'db' }), { name: 'ConnectionClosedError', outcome: 'not-sent' });
});

test('startup failure and deadline reap children and retain diagnostics', async () => {
  for (const mode of ['exit', 'silent']) {
    const child = peer(mode);
    try {
      await assert.rejects(startServer({ ...child.options, timeout: 500 }),
        mode === 'exit' ? /injected startup failure/ : { name: 'DeadlineExceededError', outcome: 'not-sent' });
      assert.throws(() => process.kill(child.pid(), 0), { code: 'ESRCH' });
    } finally { child.cleanup(); }
  }
});

test('owned servers expose diagnostics, await shutdown and escalate ignored interrupts', async () => {
  const child = peer('ignore-interrupt');
  try {
    const server = await startServer({ ...child.options, host: '::1' });
    assert.equal(server.pid, child.pid());
    assert.equal(server.stderr, '');
    const result = await server.close({ timeout: 10 });
    assert.equal(result.signal, 'SIGKILL');
    assert.deepEqual(await server.exited, result);
    assert.deepEqual(await server.close(), result);
    await server[Symbol.asyncDispose]();
  } finally { child.cleanup(); }
});

test('process clients forward requests and dispose their server after connection failures', async () => {
  const healthy = peer('healthy');
  try {
    const client = await connectProcess(healthy.options);
    assert.equal(await client.count(), 0n);
    assert.equal(client.transport.stats.requests, 2);
    const server = client.transport.server;
    // Waiting for a real diagnostic also checks stderr written after startup.
    for (let i = 0; i < 100 && !server.stderr.includes('request complete'); i++)
      await new Promise((resolve) => setTimeout(resolve, 10));
    assert.match(server.stderr, /request complete/);
    await client[Symbol.asyncDispose]();
    assert.throws(() => process.kill(healthy.pid(), 0), { code: 'ESRCH' });
  } finally { healthy.cleanup(); }
  const incompatible = peer('bad-peer');
  try {
    await assert.rejects(connectProcess(incompatible.options), { name: 'ProtocolMismatchError' });
    assert.ok(existsSync(incompatible.options.args[2]));
    assert.throws(() => process.kill(incompatible.pid(), 0), { code: 'ESRCH' });
  } finally { incompatible.cleanup(); }
});

test('transport close errors still stop the owned server and retain typed failures', async () => {
  for (const failure of [new LinksError('typed'), new Error('ordinary')]) {
    let stopped = false;
    const transport = new ProcessTransport({ close: async () => { throw failure; } },
      { close: async () => { stopped = true; } });
    await assert.rejects(transport.close(), failure instanceof LinksError
      ? (error) => error === failure : ConnectionClosedError);
    assert.equal(stopped, true);
  }
});
