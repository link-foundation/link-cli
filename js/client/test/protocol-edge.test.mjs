import assert from 'node:assert/strict';
import test from 'node:test';
import { LinksClient, LinksDecorator, createWasmClient, MemoryCommitLog, ReplicatedCommitLog } from '../src/index.js';
import { scopeOf, scopeError, watchScope } from '../src/deadline.js';
import { checkAddress, toAddress, requests, replies, errorMessage } from '../src/operations.js';
import { decodeReferenceLiteral, formatReference, group, named, parseDocument, formatDocument } from '../src/lino.js';
import { MessageReader, encodeMessage } from '../src/framing.js';

const document = parseDocument;
test('malformed operation replies cannot become successful mutations or reads', async () => {
  for (const value of [undefined, null, {}, '-1', '1.0', true, -1, NaN])
    assert.throws(() => toAddress(value), { name: 'AddressRangeError' });
  assert.equal(checkAddress('42', null), 42n);
  for (const restriction of [{}, [1, 2, 3, 4]]) assert.throws(() => requests.count(restriction), TypeError);
  for (const name of [null, {}, 1]) {
    assert.throws(() => requests.setName(1, name), TypeError);
    assert.throws(() => requests.getByName(name), TypeError);
  }
  assert.deepEqual(requests.each(), [named('each', [group([])])]);
  const invalid = {
    count: ['', 'count', '(count:)', '(count: 1 2)', '(count: (x))', '(count: nope)'],
    name: ['name', '(name:)', '(name: 1 2)', '(name: (x y))', '(wrong: text)'],
    link: ['link', '(link:)', '(link: 1 2)', '(link: nope)'],
    links: ['plain', '()', '(1: 1)', '(1: (x y) 1)', '(a: 1 1)'],
    namedLinks: ['plain', '()', '(1: 1)', '(1: (x y) 1)'],
    changes: ['plain', '(1: 1 1)', '() () ()', 'a b', '(() ()) ()'],
    namedChanges: ['plain', '() (1: (x y) 1)'],
    empty: ['(link: 1)'],
  };
  for (const [kind, texts] of Object.entries(invalid))
    for (const text of texts) assert.throws(() => replies[kind](document(text)), { name: 'MalformedMessageError' }, `${kind}: ${text}`);
  assert.deepEqual(replies.changes(document('() (1: 0 0)')), [{ before: null, after: { index: 1n, source: 0n, target: 0n } }]);
  assert.equal(replies.link([]), null);
  for (const text of ['plain', '(error: a b)', '(error: (x y))', '(other: x)', '() ()']) assert.equal(errorMessage(document(text)), null);
  assert.equal(errorMessage(document('(error: broken)')), 'broken');
  const brokenCreate = createWasmClient({ request: () => '((1: 1 1)) ()' });
  await assert.rejects(brokenCreate.create(0, 0), /no created link/); await brokenCreate.close();
});

test('decorators preserve the complete client contract and request scopes', async () => {
  const answers = new Map([
    ['(count: ())', '(count: 1)'], ['(each: ())', '(1: 0 0)'],
    ['(update: (1 0 1))', '((1: 0 0)) ((1: 0 1))'], ['(delete: 1)', '((1: 0 1)) ()'],
    ['(get-name: 1)', '(name: value)'], ['(set-name: (1 value))', '(link: 1)'],
    ['(get-by-name: value)', '(link: 1)'], ['(remove-name: 1)', ''], ['', '(1: 0 0)'],
    ['() ((0 0))', '() ((1: 0 0))'],
  ]);
  const transport = { send: async (text) => { assert.ok(answers.has(text), text); return answers.get(text); } };
  const client = new LinksClient(transport); const decorated = new LinksDecorator(new LinksDecorator(client));
  assert.equal(client.maxAddress, null);
  assert.equal(await decorated.requestText('(get-name: 1)'), '(name: value)');
  assert.deepEqual(await decorated.request([named('count', [group([])])]), [named('count', ['1'])]);
  assert.equal(await decorated.count(), 1n); assert.equal((await decorated.each()).length, 1);
  assert.equal((await decorated.update(1, 0, 1)).length, 1); assert.equal((await decorated.delete(1)).length, 1);
  assert.equal(await decorated.getName(1), 'value'); assert.equal(await decorated.setName(1, 'value'), 1n);
  assert.equal(await decorated.getByName('value'), 1n); await decorated.removeName(1);
  assert.equal((await decorated.query('() ((0 0))')).length, 1); assert.equal((await decorated.exportLinks()).length, 1);
  await decorated[Symbol.asyncDispose]();
  assert.throws(() => new LinksDecorator(), TypeError); assert.throws(() => new LinksClient(), TypeError);
  await client[Symbol.asyncDispose]();
  for (const query of ['', 1]) await assert.rejects(client.query(query), TypeError);
  for (const window of [0, NaN, 1.1]) await assert.rejects(client.batch([], { window }), TypeError);
  const batch = await client.batch([
    { op: 'each' }, { op: 'getName', index: 1 }, { op: 'setName', index: 1, name: 'value' },
    { op: 'getByName', name: 'value' }, { op: 'removeName', index: 1 },
    { op: 'update', index: 1, source: 0, target: 1 }, { op: 'query', text: '() ((0 0))' },
  ]);
  assert.equal(batch.ok, 7); assert.equal(batch.firstError, null);
  const unexpected = new LinksClient({ send() { throw Error('lost response'); } });
  assert.equal((await unexpected.batch([{ op: 'count' }])).unknown, 1);
});

test('scope validation and far deadlines are bounded and cancellation fires once', (t) => {
  for (const deadline of [NaN, Infinity, new Date('invalid')]) assert.throws(() => scopeOf({ deadline }), TypeError);
  for (const timeout of [-1, NaN, Infinity]) assert.throws(() => scopeOf({ timeout }), TypeError);
  let now = 100; t.mock.method(Date, 'now', () => now);
  assert.equal(scopeOf({ deadline: new Date(150), timeout: 100 }).deadline, 150);
  assert.equal(scopeOf({ deadline: null, timeout: 10 }).deadline, 110);
  assert.equal(scopeError({ signal: { aborted: true, reason: '' } }, 'not-sent').message, 'the request was aborted');
  const scheduled = []; const cleared = [];
  t.mock.method(globalThis, 'setTimeout', (callback, delay) => { scheduled.push({ callback, delay }); return scheduled.length; });
  t.mock.method(globalThis, 'clearTimeout', (id) => cleared.push(id));
  const controller = new AbortController(); const ended = [];
  const deadline = now + 2 ** 31 + 10;
  const stop = watchScope({ signal: controller.signal, deadline }, (errorOf) => ended.push(errorOf('unknown')));
  assert.equal(scheduled[0].delay, 2 ** 31 - 1);
  now = deadline - 1; scheduled[0].callback(); assert.equal(scheduled[1].delay, 1);
  now = deadline; scheduled[1].callback(); scheduled[1].callback(); controller.abort(); stop();
  assert.equal(ended.length, 1); assert.equal(ended[0].outcome, 'unknown');
  assert.equal(ended[0].name, 'DeadlineExceededError'); assert.ok(cleared.length > 0);
});

test('quote runs, top-level names and framing limits preserve protocol boundaries', () => {
  assert.deepEqual(parseDocument("''x''"), ['x']);
  assert.deepEqual(parseDocument("''a(b)c''"), ['a(b)c']);
  assert.deepEqual(parseDocument("'a''b'"), ["a'b"]);
  assert.deepEqual(parseDocument("'' ''"), [group(['', ''])]);
  assert.deepEqual(parseDocument('name: a b'), [named('name', ['a', 'b'])]);
  assert.deepEqual(parseDocument('  \r\n  '), []);
  assert.deepEqual(parseDocument(formatDocument([group(['x'])])), [group(['x'])]);
  for (const text of [')', '(a b', '~1{00', "''a)b(''", "''a(b''"]) assert.throws(() => parseDocument(text), { name: 'MalformedMessageError' });
  for (const literal of ['~2{00}', '~1{gg}', 'raw']) assert.throws(() => decodeReferenceLiteral(literal), { name: 'MalformedMessageError' });
  assert.throws(() => formatReference(1), TypeError); assert.throws(() => parseDocument(null), TypeError);
  assert.throws(() => new MessageReader({ maxBytes: 2 }).push(encodeMessage('abc')), { name: 'LimitExceededError' });
});

test('volatile custom branch capabilities and control failures stay explicit', async () => {
  const client = createWasmClient({ request: () => '', rollback() { throw Error('rollback unavailable'); } }, { owned: true });
  await assert.rejects(client.transport.control('unknown'), { name: 'UnsupportedError' });
  await assert.rejects(client.transport.control('rollback'), (error) => error.outcome === 'failed');
  await client.close();
  const primary = new MemoryCommitLog(); const replica = new MemoryCommitLog();
  assert.equal(new ReplicatedCommitLog(primary, [replica]).supportsDurableFlush, false);
});
