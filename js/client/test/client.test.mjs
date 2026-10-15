import assert from 'node:assert/strict';
import test from 'node:test';
import { LinksClient, LinksDecorator, tcpServerCapabilities, wasmCapabilities } from '../src/index.js';

/** A transport answering from a script: `answer(text) => reply text`. */
function scripted(answer, capabilities = tcpServerCapabilities()) {
  const sent = [];
  return {
    sent,
    capabilities,
    closed: 0,
    async send(text, scope) {
      sent.push(text);
      if (scope?.signal?.aborted) throw new Error('scope ignored');
      return answer(text, sent.length);
    },
    async close() {
      this.closed += 1;
    },
  };
}

const BIG = 2n ** 53n + 1n; // not representable as a Number

test('addresses travel as BigInt, never rounded through Number', async () => {
  const transport = scripted(() => `() ((${BIG}: ${BIG} 4294967295))`, tcpServerCapabilities({ maxAddress: 2n ** 64n - 1n, addressBits: 64 }));
  const client = new LinksClient(transport);
  const link = await client.create(BIG, 4294967295n);
  assert.deepEqual(link, { index: BIG, source: BIG, target: 4294967295n });
  assert.equal(transport.sent[0], `(create: (${BIG} 4294967295))`);
  await assert.rejects(client.create(2 ** 53 + 2, 1), { name: 'AddressRangeError', outcome: 'not-sent' });
  assert.equal(transport.sent.length, 1);
});

test('addresses beyond the backend range are refused before sending', async () => {
  const transport = scripted(() => '(count: 0)');
  const client = new LinksClient(transport);
  await assert.rejects(client.create(2n ** 32n, 1n), { name: 'AddressRangeError', outcome: 'not-sent' });
  await assert.rejects(client.count([1n, 2n ** 32n]), { name: 'AddressRangeError' });
  await assert.rejects(client.delete(-1n), { name: 'AddressRangeError' });
  assert.equal(transport.sent.length, 0);
  assert.equal(await client.count([4294967295n]), 0n);
});

test('requests and replies of every operation', async () => {
  const answers = {
    '(count: (* 1))': '(count: 2)',
    '(each: (* * 2))': '(2: 1 2)\n(3: 2 2)',
    '(update: (2 2 1))': '((2: 1 2)) ((2: 2 1))',
    '(delete: 1)': '((1: 1 1)) ()\n((2: 1 2)) ()',
    '(get-name: 2)': "(name: 'a pair')",
    '(get-name: 3)': '',
    "(set-name: (2 'a pair'))": '(link: 102)',
    "(get-by-name: 'a pair')": '(link: 2)',
    '(get-by-name: nobody)': '',
    '(remove-name: 2)': '',
    '': '(1: 1 1)\n(pair: 1 1)',
    '() ((1 1))': '() ((1: 1 1))',
  };
  const transport = scripted((text) => {
    assert.ok(text in answers, `unexpected request ${text}`);
    return answers[text];
  });
  const client = new LinksClient(transport);
  assert.equal(await client.count(['*', 1]), 2n);
  assert.deepEqual(await client.each([null, '*', 2]), [
    { index: 2n, source: 1n, target: 2n },
    { index: 3n, source: 2n, target: 2n },
  ]);
  assert.deepEqual(await client.update(2, 2, 1), [{ before: { index: 2n, source: 1n, target: 2n }, after: { index: 2n, source: 2n, target: 1n } }]);
  assert.equal((await client.delete(1)).length, 2);
  assert.equal(await client.getName(2), 'a pair');
  assert.equal(await client.getName(3), null);
  assert.equal(await client.setName(2, 'a pair'), 102n);
  assert.equal(await client.getByName('a pair'), 2n);
  assert.equal(await client.getByName('nobody'), null);
  assert.equal(await client.removeName(2), undefined);
  assert.deepEqual(await client.exportLinks(), [
    { index: '1', source: '1', target: '1' },
    { index: 'pair', source: '1', target: '1' },
  ]);
  assert.deepEqual(await client.query('() ((1 1))'), [{ before: null, after: { index: '1', source: '1', target: '1' } }]);
});

test('(error: ...) replies reject with a failed RemoteError', async () => {
  const client = new LinksClient(scripted(() => "(error: 'Link not found: 7')"));
  await assert.rejects(client.delete(7), (error) => error.name === 'RemoteError' && error.outcome === 'failed' && error.remoteMessage === 'Link not found: 7');
});

test('an unexpected reply shape is a malformed message', async () => {
  const client = new LinksClient(scripted(() => '(link: 1)'));
  await assert.rejects(client.count(), { name: 'MalformedMessageError' });
});

test('aborted or expired requests are not sent', async () => {
  const transport = scripted(() => '(count: 0)');
  const client = new LinksClient(transport);
  const controller = new AbortController();
  controller.abort(new Error('user left'));
  await assert.rejects(client.count([], { signal: controller.signal }), (error) => error.name === 'AbortError' && error.outcome === 'not-sent' && /user left/.test(error.message));
  await assert.rejects(client.count([], { deadline: Date.now() - 1 }), { name: 'DeadlineExceededError', outcome: 'not-sent' });
  await assert.rejects(client.count([], { deadline: new Date(0) }), { name: 'DeadlineExceededError' });
  await assert.rejects(client.count([], { timeout: 0 }), { name: 'DeadlineExceededError' });
  assert.equal(transport.sent.length, 0);
});

test('require() names missing guarantees instead of emulating them', () => {
  const tcp = new LinksClient(scripted(() => ''));
  assert.equal(tcp.require('persistent', 'rawTcp'), tcp);
  assert.throws(() => tcp.require('atomicTransactions', 'durableFlush'), (error) => error.name === 'UnsupportedError' && /atomicTransactions, durableFlush/.test(error.message));
  const wasm = new LinksClient(scripted(() => '', wasmCapabilities()));
  assert.throws(() => wasm.require('rawTcp'), { name: 'UnsupportedError' });
  assert.throws(() => wasm.require('persistent'), { name: 'UnsupportedError' });
  assert.throws(() => wasm.require('teleport'), TypeError);
  assert.equal(wasm.capabilities.rawTcp, false);
});

test('a batch reports every operation and stops after the first failure', async () => {
  let created = 0;
  const transport = scripted((text) => {
    if (text.startsWith('(create:')) {
      created += 1;
      return `() ((${created}: 1 1))`;
    }
    if (text === '(delete: 9)') return "(error: 'Link not found: 9')";
    return '(count: 0)';
  });
  const client = new LinksClient(transport);
  const ok = await client.batch([{ op: 'create', source: 1, target: 1 }, { op: 'create', source: 1, target: 1 }, { op: 'count' }], { window: 2 });
  assert.equal(ok.completed, true);
  assert.deepEqual(
    ok.results.map((result) => result.value),
    [{ index: 1n, source: 1n, target: 1n }, { index: 2n, source: 1n, target: 1n }, 0n],
  );

  const broken = await client.batch([{ op: 'create', source: 1, target: 1 }, { op: 'delete', index: 9 }, { op: 'create', source: 1, target: 1 }, { op: 'create', source: 1, target: 1 }], { window: 1 });
  assert.equal(broken.completed, false);
  assert.deepEqual(broken.results.map((result) => result.status), ['ok', 'failed', 'not-sent', 'not-sent']);
  assert.equal(broken.firstError.name, 'RemoteError');
  assert.deepEqual([broken.ok, broken.failed, broken.unknown, broken.notSent], [1, 1, 0, 2]);
});

test('a malformed batch operation sends nothing', async () => {
  const transport = scripted(() => '(count: 0)');
  const client = new LinksClient(transport);
  await assert.rejects(client.batch([{ op: 'count' }, { op: 'create', source: 1, target: 2n ** 40n }]), /operation 1 \(create\)/);
  await assert.rejects(client.batch([{ op: 'explode' }]), /unknown op "explode"/);
  assert.equal(transport.sent.length, 0);
});

test('an aborted batch leaves the rest not sent', async () => {
  const controller = new AbortController();
  const transport = scripted((text, count) => {
    if (count === 2) controller.abort();
    return '() ((1: 1 1))';
  });
  const client = new LinksClient(transport);
  const operations = Array.from({ length: 5 }, () => ({ op: 'create', source: 1, target: 1 }));
  const result = await client.batch(operations, { window: 1, signal: controller.signal });
  assert.deepEqual(result.results.map((entry) => entry.status), ['ok', 'ok', 'not-sent', 'not-sent', 'not-sent']);
  assert.equal(result.firstError.name, 'AbortError');
});

test('a decorator is a drop-in client over any backend', async () => {
  const calls = [];
  class Recording extends LinksDecorator {
    create(source, target, options) {
      calls.push(['create', source, target]);
      return super.create(source, target, options);
    }
  }
  const transport = scripted(() => '() ((1: 1 1))');
  const decorated = new Recording(new Recording(new LinksClient(transport)));
  assert.deepEqual(await decorated.create(1n, 1n), { index: 1n, source: 1n, target: 1n });
  assert.equal(calls.length, 2);
  assert.equal(decorated.capabilities.backend, 'clink-server');
  assert.equal(decorated.require('persistent'), decorated);
  const batch = await decorated.batch([{ op: 'create', source: 1, target: 1 }]);
  assert.equal(batch.completed, true);
  await decorated[Symbol.asyncDispose]();
  assert.equal(transport.closed, 1);
});
