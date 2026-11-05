import assert from 'node:assert/strict';
import test from 'node:test';
import { ANY, DeadlineExceededError, isNotification, LinksClient, LinksDecorator, parseDocument, parseNotification, parseTripletNotification, tcpServerCapabilities, wasmCapabilities } from '../src/index.js';

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

test('subscriptions: requests, replies and pushes', async () => {
  const transport = scripted((text) => (text.startsWith('(subscribe:') ? '(subscription: 3)' : ''));
  const client = new LinksClient(transport);
  // A transport without pushes cannot subscribe.
  await assert.rejects(client.subscribe([]), { name: 'UnsupportedError', outcome: 'not-sent' });
  assert.throws(() => client.onNotification(() => {}), { name: 'UnsupportedError' });
  assert.equal(transport.sent.length, 0);

  const pushes = ['(notify: 3)\n() ((1: 0 0))'];
  transport.receivePush = async (scope) => {
    if (pushes.length > 0) return pushes.shift();
    throw new DeadlineExceededError('late', { outcome: 'not-sent' });
  };
  transport.onPush = (listener) => {
    listener('(notify: 4)\n((1: 0 0)) ((1: 1 1))\n((2: 1 1)) ()');
    return () => {};
  };
  assert.equal(await client.subscribe([ANY, 1n, ANY]), 3);
  await client.unsubscribe(3);
  assert.deepEqual(transport.sent, ['(subscribe: (* 1 *))', '(unsubscribe: 3)']);
  assert.deepEqual(await client.receiveNotification(), {
    subscription: 3,
    changes: [{ before: null, after: { index: 1n, source: 0n, target: 0n } }],
  });
  // The deadline passing is no error: nothing arrived in time.
  assert.equal(await client.receiveNotification({ timeout: 0 }), null);
  const seen = [];
  client.onNotification((notification) => seen.push(notification));
  assert.deepEqual(seen, [
    {
      subscription: 4,
      changes: [
        { before: { index: 1n, source: 0n, target: 0n }, after: { index: 1n, source: 1n, target: 1n } },
        { before: { index: 2n, source: 1n, target: 1n }, after: null },
      ],
    },
  ]);
  await assert.rejects(client.unsubscribe(2n ** 32n), { name: 'AddressRangeError' });
  await assert.rejects(client.subscribe([1, 2, 3, 4]), TypeError);
});

test('a push is told from a reply by its first link, (notify: N)', () => {
  const push = parseDocument('(notify: 1)\n() ((1: 0 0))');
  assert.equal(isNotification(push), true);
  assert.equal(isNotification(parseDocument('(notify: 12)')), true);
  for (const text of ['(count: 1)', '(notify: 1 1)', '(notify: x)', '(notify: (1 2))', '() ((1: 0 0))', '']) {
    assert.equal(isNotification(parseDocument(text)), false, JSON.stringify(text));
  }
  assert.deepEqual(parseNotification(push), { subscription: 1, changes: [{ before: null, after: { index: 1n, source: 0n, target: 0n } }] });
  assert.throws(() => parseNotification(parseDocument('(count: 1)')), { name: 'MalformedMessageError' });
  assert.throws(() => parseNotification(parseDocument('(notify: 1)\n(1: 0 0)')), { name: 'MalformedMessageError' });
});

test('triplets: requests and replies of every operation', async () => {
  const answers = {
    '(count: (* 4))': '(count: 2)',
    '(each: (* 4 5 6))': '(10: 4 5 6)\n(11: 4 5 6)',
    '(create: (0 0 0))': '() ((3: 3 0 0))',
    '(create: (4 5 6))': '() ((10: 4 5 6))',
    '(update: (10 4 6 5))': '((10: 4 5 6)) ((10: 4 6 5))',
    '(delete: 5)': '((5: 3 0 0)) ()\n((11: 4 5 6)) ()',
    '() ((Person 0 0))': '() ((17: Person 0 0))',
    '': '(3: Person 0 0)',
  };
  const transport = scripted((text) => answers[text]);
  const client = new LinksClient(transport);
  assert.equal(await client.countTriplets([ANY, 4]), 2n);
  assert.deepEqual(await client.eachTriplets([null, 4, 5, 6]), [
    { index: 10n, type: 4n, from: 5n, to: 6n },
    { index: 11n, type: 4n, from: 5n, to: 6n },
  ]);
  assert.deepEqual(await client.createTripletPoint(), { index: 3n, type: 3n, from: 0n, to: 0n });
  assert.deepEqual(await client.createTriplet(4, 5, 6), { index: 10n, type: 4n, from: 5n, to: 6n });
  assert.deepEqual(await client.updateTriplet(10, 4, 6, 5), [
    { before: { index: 10n, type: 4n, from: 5n, to: 6n }, after: { index: 10n, type: 4n, from: 6n, to: 5n } },
  ]);
  assert.deepEqual(await client.deleteTriplet(5), [
    { before: { index: 5n, type: 3n, from: 0n, to: 0n }, after: null },
    { before: { index: 11n, type: 4n, from: 5n, to: 6n }, after: null },
  ]);
  assert.deepEqual(await client.queryTriplets('() ((Person 0 0))'), [{ before: null, after: { index: '17', type: 'Person', from: '0', to: '0' } }]);
  assert.deepEqual(await client.exportTriplets(), [{ index: '3', type: 'Person', from: '0', to: '0' }]);
  const batch = await client.batch([
    { op: 'countTriplets', restriction: [ANY, 4] },
    { op: 'createTriplet', type: 4, from: 5, to: 6 },
    { op: 'updateTriplet', index: 10, type: 4, from: 6, to: 5 },
    { op: 'deleteTriplet', index: 5 },
    { op: 'eachTriplets', restriction: [ANY, 4, 5, 6] },
    { op: 'queryTriplets', text: '() ((Person 0 0))' },
  ]);
  assert.equal(batch.completed, true);
  assert.equal(batch.results[0].value, 2n);
  assert.equal(batch.results[1].value.type, 4n);
  assert.equal(batch.results[5].value[0].after.type, 'Person');
});

test('triplets: a part is checked before sending and a reply is checked after', async () => {
  const transport = scripted(() => '(1: 2 3)');
  const client = new LinksClient(transport);
  await assert.rejects(client.eachTriplets([1, 2, 3, 4, 5]), TypeError);
  await assert.rejects(client.countTriplets('1'), TypeError);
  await assert.rejects(client.createTriplet(2n ** 32n, 0, 0), { name: 'AddressRangeError', outcome: 'not-sent' });
  await assert.rejects(client.updateTriplet(1, 1, -1n, 0), { name: 'AddressRangeError' });
  await assert.rejects(client.queryTriplets(' '), TypeError);
  assert.equal(transport.sent.length, 0);
  // A doublet server's reply is no triplet: the mismatch is not read as one.
  await assert.rejects(client.eachTriplets(), { name: 'MalformedMessageError' });
  const noChange = new LinksClient(scripted(() => ''));
  await assert.rejects(noChange.createTriplet(1, 0, 0), { name: 'MalformedMessageError' });
  const error = new LinksClient(scripted(() => "(error: 'The type 99 is not a triplet.')"));
  await assert.rejects(error.createTriplet(99, 0, 0), { name: 'RemoteError', message: /The type 99 is not a triplet/ });
});

test('triplets: a reply packed with --arity 2..3 reads as the canonical form', async () => {
  const client = new LinksClient(scripted(() => '(10: (4 5 6))\n(11: 4 5 6)'));
  assert.deepEqual(await client.eachTriplets(), [
    { index: 10n, type: 4n, from: 5n, to: 6n },
    { index: 11n, type: 4n, from: 5n, to: 6n },
  ]);
  const changes = new LinksClient(scripted(() => '((10: (4 5 6))) ((10: (4 6 5)))'));
  assert.deepEqual(await changes.updateTriplet(10, 4, 6, 5), [
    { before: { index: 10n, type: 4n, from: 5n, to: 6n }, after: { index: 10n, type: 4n, from: 6n, to: 5n } },
  ]);
  const notification = parseTripletNotification(parseDocument('(notify: 1)\n() ((16: (4 6 7)))'));
  assert.deepEqual(notification, { subscription: 1, changes: [{ before: null, after: { index: 16n, type: 4n, from: 6n, to: 7n } }] });
  for (const text of ['(1: (2 3))', '(1: (2 3 4 5))', '(1: 2 3 4 5)', '(2 3 4)', '(1: (2: 3 4 5))', '(1: 2 (3 5) 4)']) {
    await assert.rejects(new LinksClient(scripted(() => text)).eachTriplets(), { name: 'MalformedMessageError' }, text);
  }
});

test('triplets: subscriptions, and a backend stated not to serve them', async () => {
  const transport = scripted((text) => (text.startsWith('(subscribe:') ? '(subscription: 3)' : ''));
  const client = new LinksClient(transport);
  await assert.rejects(client.subscribeTriplets([]), { name: 'UnsupportedError', outcome: 'not-sent' });
  await assert.rejects(client.receiveTripletNotification(), { name: 'UnsupportedError' });
  assert.throws(() => client.onTripletNotification(() => {}), { name: 'UnsupportedError' });
  const pushes = ['(notify: 3)\n() ((16: 4 6 7))'];
  transport.receivePush = async () => {
    if (pushes.length > 0) return pushes.shift();
    throw new DeadlineExceededError('late', { outcome: 'not-sent' });
  };
  transport.onPush = (listener) => {
    listener('(notify: 4)\n((16: 4 6 7)) ()');
    return () => {};
  };
  assert.equal(await client.subscribeTriplets([ANY, 4n, ANY, 7n]), 3);
  assert.equal(transport.sent[0], '(subscribe: (* 4 * 7))');
  assert.deepEqual(await client.receiveTripletNotification(), { subscription: 3, changes: [{ before: null, after: { index: 16n, type: 4n, from: 6n, to: 7n } }] });
  assert.equal(await client.receiveTripletNotification({ timeout: 0 }), null);
  const seen = [];
  client.onTripletNotification((notification) => seen.push(notification));
  assert.deepEqual(seen, [{ subscription: 4, changes: [{ before: { index: 16n, type: 4n, from: 6n, to: 7n }, after: null }] }]);
  assert.throws(() => client.onTripletNotification('no'), TypeError);

  assert.equal(client.capabilities.triplets, null);
  assert.throws(() => client.require('triplets'), { name: 'UnsupportedError' });
  assert.equal(new LinksClient(scripted(() => ''), { capabilities: { triplets: true } }).require('triplets').capabilities.triplets, true);

  const noTriplets = scripted(() => '(count: 0)', wasmCapabilities());
  const wasm = new LinksClient(noTriplets);
  await assert.rejects(wasm.countTriplets(), { name: 'UnsupportedError', outcome: 'not-sent' });
  await assert.rejects(wasm.createTripletPoint(), { name: 'UnsupportedError' });
  await assert.rejects(wasm.batch([{ op: 'createTriplet', type: 0, from: 0, to: 0 }]), { name: 'UnsupportedError' });
  assert.throws(() => wasm.onTripletNotification(() => {}), { name: 'UnsupportedError' });
  assert.equal(noTriplets.sent.length, 0);
});

test('a decorator forwards every triplet call', async () => {
  const called = [];
  const links = Object.fromEntries(
    ['countTriplets', 'eachTriplets', 'createTriplet', 'createTripletPoint', 'updateTriplet', 'deleteTriplet', 'queryTriplets', 'exportTriplets', 'subscribeTriplets', 'receiveTripletNotification', 'onTripletNotification'].map(
      (name) => [name, (...args) => called.push([name, ...args])],
    ),
  );
  const decorated = new LinksDecorator(links);
  decorated.countTriplets([1], 'o');
  decorated.eachTriplets([1], 'o');
  decorated.createTriplet(1, 2, 3, 'o');
  decorated.createTripletPoint('o');
  decorated.updateTriplet(1, 2, 3, 4, 'o');
  decorated.deleteTriplet(1, 'o');
  decorated.queryTriplets('q', 'o');
  decorated.exportTriplets('o');
  decorated.subscribeTriplets([1], 'o');
  decorated.receiveTripletNotification('o');
  decorated.onTripletNotification('l');
  assert.deepEqual(called, [
    ['countTriplets', [1], 'o'],
    ['eachTriplets', [1], 'o'],
    ['createTriplet', 1, 2, 3, 'o'],
    ['createTripletPoint', 'o'],
    ['updateTriplet', 1, 2, 3, 4, 'o'],
    ['deleteTriplet', 1, 'o'],
    ['queryTriplets', 'q', 'o'],
    ['exportTriplets', 'o'],
    ['subscribeTriplets', [1], 'o'],
    ['receiveTripletNotification', 'o'],
    ['onTripletNotification', 'l'],
  ]);
});
