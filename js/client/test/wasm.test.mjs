import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { createWasmClient, WasmTransport } from '../src/index.js';

/** A stand-in for the `Clink` class of `rust/wasm`: same `request` and `free`. */
function fakeClink(answer = () => '(count: 0)') {
  return {
    requests: [],
    freed: 0,
    request(text) {
      this.requests.push(text);
      return answer(text);
    },
    free() {
      this.freed += 1;
    },
  };
}

test('the WebAssembly adapter states that it has no raw TCP', () => {
  const client = createWasmClient(fakeClink());
  assert.equal(client.capabilities.rawTcp, false);
  assert.equal(client.capabilities.transport, 'in-process');
  assert.throws(() => client.require('rawTcp'), { name: 'UnsupportedError' });
  assert.throws(() => client.require('atomicTransactions'), { name: 'UnsupportedError' });
  assert.throws(() => new WasmTransport({}), TypeError);
});

test('requests run in order and a create equal to nothing new resolves to null', async () => {
  const clink = fakeClink((text) => (text === '(create: (1 1))' ? '' : '(count: 7)'));
  const client = createWasmClient(clink);
  const [count, created] = await Promise.all([client.count(), client.create(1n, 1n)]);
  assert.equal(count, 7n);
  assert.equal(created, null);
  assert.deepEqual(clink.requests, ['(count: ())', '(create: (1 1))']);
});

test('aborted or late requests never reach the store', async () => {
  const clink = fakeClink();
  const client = createWasmClient(clink);
  const controller = new AbortController();
  controller.abort();
  await assert.rejects(client.count([], { signal: controller.signal }), { name: 'AbortError', outcome: 'not-sent' });
  await assert.rejects(client.count([], { deadline: Date.now() - 1 }), { name: 'DeadlineExceededError', outcome: 'not-sent' });
  assert.equal(clink.requests.length, 0);
});

test('request and reply limits', async () => {
  const clink = fakeClink(() => `(count: ${'9'.repeat(100)})`);
  const client = createWasmClient(clink, { maxRequestBytes: 16, maxReplyBytes: 64 });
  await assert.rejects(client.getByName('a name longer than sixteen bytes'), { name: 'LimitExceededError', outcome: 'not-sent' });
  assert.equal(clink.requests.length, 0);
  await assert.rejects(client.count(), { name: 'LimitExceededError', outcome: 'unknown' });
  assert.equal(clink.requests.length, 1);
});

test('close frees the store only when the client owns it', async () => {
  const borrowed = fakeClink();
  await createWasmClient(borrowed).close();
  assert.equal(borrowed.freed, 0);
  const owned = fakeClink();
  const client = createWasmClient(owned, { owned: true });
  await client.close();
  await client.close();
  assert.equal(owned.freed, 1);
  await assert.rejects(client.count(), { name: 'ConnectionClosedError', outcome: 'not-sent' });
});

test('the universal entry point imports nothing from node:', async () => {
  const seen = new Set();
  const visit = async (url) => {
    if (seen.has(url.href)) return;
    seen.add(url.href);
    const source = await readFile(url, 'utf8');
    for (const [, specifier] of source.matchAll(/^\s*(?:import|export)\b[^'"]*?from\s+'([^']+)'/gm)) {
      assert.ok(specifier.startsWith('./'), `${url.pathname} imports ${specifier}`);
      await visit(new URL(specifier, url));
    }
  };
  await visit(new URL('../src/index.js', import.meta.url));
  assert.ok(seen.size > 5);
  for (const file of seen) assert.ok(!/\/(tcp|process)\.js$/.test(file), file);
});

test('a request whose scope expires while queued never reaches native code', async () => {
  const clink = fakeClink(); const transport = new WasmTransport(clink);
  const controller = new AbortController();
  const pending = transport.send('(count: ())', { signal: controller.signal }); controller.abort();
  await assert.rejects(pending, { outcome: 'not-sent', name: 'AbortError' });
  assert.equal(clink.requests.length, 0); await transport.close();
});
