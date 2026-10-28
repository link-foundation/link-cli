import assert from 'node:assert/strict';
import test from 'node:test';
import { EtcdJournalStore } from '../src/cluster.js';

const b64 = (text) => btoa(text);
const response = (value) => new Response(JSON.stringify(value));
const header = { revision: '9' };

test('etcd snapshot reads all selected journals atomically and compares modification generations', async () => {
  const calls = [];
  const store = new EtcdJournalStore(['http://localhost:2379'], { prefix: '/test/', fetch: async (url, options) => {
    const body = JSON.parse(options.body); calls.push(body);
    if (!body.compare) return response({ header, succeeded: true, responses: [
      { response_range: { kvs: [{ key: b64('/test/a'), mod_revision: '7', value: b64('["record"]') }] } },
      { response_range: {} },
    ] });
    return response({ header, succeeded: true });
  } });
  const snapshot = await store.read(['a', 'b']);
  assert.deepEqual(snapshot, [{ generation: '7', entries: ['record'] }, { generation: '0', entries: [] }]);
  assert.equal(calls[0].success.length, 2);
  assert.equal(await store.compareExchange(['a', 'b'], snapshot, [['record', 'new'], null]), true);
  assert.deepEqual(calls[1].compare.map((c) => [c.target, c.mod_revision]), [['MOD', '7'], ['MOD', '0']]);
  assert.equal(calls[1].success.length, 1);
  assert.equal(calls[1].success[0].request_put.key, b64('/test/a'));
});

test('read failover is bounded while an uncertain commit is never replayed', async () => {
  const calls = [];
  const store = new EtcdJournalStore(['http://one:1', 'http://two:2'], { fetch: async (url, options) => {
    calls.push(url);
    if (url.startsWith('http://one')) throw Error('partition');
    if (JSON.parse(options.body).compare) throw Error('response lost');
    return response({ header, succeeded: true, responses: [{ response_range: {} }] });
  } });
  const snapshot = await store.read(['a']);
  assert.equal(calls.length, 2);
  await assert.rejects(store.compareExchange(['a'], snapshot, [['new']]), { outcome: 'unknown' });
  assert.equal(calls.length, 3);
});

test('malformed, oversized and rejected coordinator responses fail closed', async () => {
  for (const value of [{}, { succeeded: true, responses: [] }, { succeeded: true, responses: [{ response_range: { more: true } }] },
    { succeeded: true, responses: [{ response_range: { kvs: [{ key: b64('/link-cli/engine1/a'), mod_revision: '1', value: b64('{}') }] } }] }]) {
    const store = new EtcdJournalStore(['http://localhost:1'], { fetch: async () => response(value) });
    await assert.rejects(store.read(['a']));
  }
  const invalid = new EtcdJournalStore(['http://localhost:1'], { fetch: async () => response({}) });
  await assert.rejects(invalid.compareExchange(['a'], [{ generation: '0', entries: [] }], [['record']]), { outcome: 'unknown' });
  const conflict = new EtcdJournalStore(['http://localhost:1'], { fetch: async () => response({ header }) });
  assert.equal(await conflict.compareExchange(['a'], [{ generation: '0', entries: [] }], [null]), false);
  const limited = new EtcdJournalStore(['http://localhost:1'], { maxRequestBytes: 10, fetch() { throw Error('must not send'); } });
  await assert.rejects(limited.read(['a']), { name: 'LimitExceededError', outcome: 'not-sent' });
  const huge = new EtcdJournalStore(['http://localhost:1'], { maxResponseBytes: 4, fetch: async () => response({ huge: 'response' }) });
  await assert.rejects(huge.read(['a']), { name: 'LimitExceededError' });
  const refused = new EtcdJournalStore(['http://localhost:1'], { fetch: async () => new Response('unavailable', { status: 503 }) });
  await assert.rejects(refused.read(['a']), /503/);
});

test('invalid cluster coordinates are rejected before network I/O', async () => {
  for (const endpoints of [[], ['ftp://localhost'], ['not a url']]) assert.throws(() => new EtcdJournalStore(endpoints));
  for (const value of [0, -1, NaN]) assert.throws(() => new EtcdJournalStore(['http://localhost:1'], { timeoutMs: value }));
  const store = new EtcdJournalStore(['http://localhost:1'], { fetch() { throw Error('must not send'); } });
  for (const keys of [[], ['a', 'a'], ['../b'], Array(33).fill('a')]) await assert.rejects(store.read(keys), TypeError);
});

const emptySnapshot = (keys) => keys.map(() => ({ generation: '0', entries: [] }));
function bindingWith(events, { failFree = false } = {}) {
  let next = 0;
  return { StagedEngine: class {
    id = next++;
    begin() { const id = this.id; return { request: () => '(count: 0)', commit() {}, rollback() {}, free() { events.push(`session ${id}`); if (failFree && id === 0) throw Error('free failed'); } }; }
    journal() { return '[]'; }
    free() { events.push(`engine ${this.id}`); }
  } };
}

test('close waits for an in-flight commit and frees every workspace after cleanup failure', async () => {
  const { ClusterEngine } = await import('../src/cluster.js');
  const events = []; let publish;
  const store = { read: async (keys) => emptySnapshot(keys), compareExchange: () => new Promise((done) => { publish = done; }) };
  const tx = await new ClusterEngine(bindingWith(events), store).begin(['a']);
  const committed = tx.commit();
  while (!publish) await Promise.resolve();
  let closed = false; const closing = tx.close().then(() => { closed = true; });
  await Promise.resolve(); await Promise.resolve(); assert.equal(closed, false);
  publish(true); await committed; await closing;
  assert.deepEqual(events, ['session 0', 'engine 0']);
  const failing = await new ClusterEngine(bindingWith(events, { failFree: true }), store).begin(['a', 'b']);
  await assert.rejects(failing.close(), /free failed/);
  assert.deepEqual(events.slice(2), ['session 0', 'engine 0', 'session 1', 'engine 1']);
});

test('cluster validates configuration and snapshots before creating private engines', async () => {
  const { ClusterEngine } = await import('../src/cluster.js');
  const events = []; const binding = bindingWith(events);
  assert.throws(() => new ClusterEngine(binding, {}), TypeError);
  for (const options of [{ concurrency: 'exclusive' }, { concurrency: 'pessimistic' }, { isolation: 'snapshot' }, { requireDurableFlush: true }])
    assert.throws(() => new ClusterEngine(binding, { read() {}, compareExchange() {} }, options), { name: 'UnsupportedError' });
  for (const snapshot of [[], null]) {
    const cluster = new ClusterEngine(binding, { read: async () => snapshot, compareExchange() {} });
    await assert.rejects(cluster.begin(['a']), /incomplete/);
  }
  const corrupt = new ClusterEngine(binding, { read: async () => [{ generation: '0', entries: [null] }], compareExchange() {} });
  await assert.rejects(corrupt.begin(['a']), /array of strings/); assert.deepEqual(events, []);
  let n = 0;
  class Broken {
    begin() { if (n++) throw Error('cannot begin'); return { request() {}, commit() {}, rollback() {}, free() { events.push('session'); } }; }
    free() { events.push('engine'); }
  }
  await assert.rejects(new ClusterEngine({ StagedEngine: Broken }, { read: async () => emptySnapshot(['a', 'b']), compareExchange() {} }).begin(['a', 'b']), /cannot begin/);
  assert.deepEqual(events, ['session', 'engine', 'engine']);
});

test('cluster freezes its expected snapshot, rejects stale groups, and classifies custom store I/O', async () => {
  const { ClusterEngine } = await import('../src/cluster.js');
  const events = []; const snapshot = emptySnapshot(['a']); let expected;
  const store = { read: async () => snapshot, compareExchange: async (_keys, versions) => { expected = versions; return false; } };
  const tx = await new ClusterEngine(bindingWith(events), store).begin(['a']);
  snapshot[0].generation = '9'; snapshot[0].entries.push('mutated');
  assert.throws(() => tx.shard('b'), /not selected/);
  const shard = tx.shard('a'); assert.equal(await shard.count(), 0n);
  await assert.rejects(tx.commit(), { outcome: 'failed' });
  assert.deepEqual(expected, emptySnapshot(['a']));
  await assert.rejects(shard.count(), /closed/); await assert.rejects(tx.commit(), /closed/);
  store.compareExchange = async () => { throw Error('connection lost'); };
  snapshot[0] = { generation: '0', entries: [] };
  const uncertain = await new ClusterEngine(bindingWith(events), store).begin(['a']);
  await assert.rejects(uncertain.commit(), { outcome: 'unknown' });
});

test('etcd rejects every malformed wire field and validates CAS inputs without sending', async () => {
  const kv = { key: b64('/link-cli/engine1/a'), mod_revision: '1', value: b64('[]') };
  for (const changes of [{ mod_revision: 1 }, { mod_revision: '+1' }, { mod_revision: '9223372036854775808' },
    { key: 'wrong' }, { value: '???' }, { value: 'W11=' }, { value: b64('{}') }, { value: b64('[null]') }]) {
    const store = new EtcdJournalStore(['http://localhost:1'], { fetch: async () => response({ succeeded: true, responses: [{ response_range: { kvs: [{ ...kv, ...changes }] } }] }) });
    await assert.rejects(store.read(['a']));
  }
  for (const range of [{ kvs: {} }, { kvs: [kv, kv] }, { more: 'false' }]) {
    const store = new EtcdJournalStore(['http://localhost:1'], { fetch: async () => response({ succeeded: true, responses: [{ response_range: range }] }) });
    await assert.rejects(store.read(['a']));
  }
  for (const value of [{ header: { revision: '9223372036854775808' } }, { header: { revision: '0' } }, { header, succeeded: 'false' }]) {
    const store = new EtcdJournalStore(['http://localhost:1'], { fetch: async () => response(value) });
    await assert.rejects(store.compareExchange(['a'], [{ generation: '0', entries: [] }], [null]), { outcome: 'unknown' });
  }
  const store = new EtcdJournalStore(['http://localhost:1'], { fetch() { throw Error('must not send'); } });
  for (const generation of [0, '+1', '9223372036854775808']) await assert.rejects(store.compareExchange(['a'], [{ generation, entries: [] }], [null]), TypeError);
  await assert.rejects(store.compareExchange(['a'], [], []), TypeError);
  for (const options of [{ prefix: '/' }, { prefix: 1 }, { fetch: null }]) assert.throws(() => new EtcdJournalStore(['http://localhost:1'], options));
  for (const endpoint of ['http://localhost:1/?query', 'http://localhost:1/#fragment']) assert.throws(() => new EtcdJournalStore([endpoint]));
  await assert.rejects(store.read([null]), TypeError);
});

test('cluster publishes changed journals, retains explicit adapter outcomes and disposes asynchronously', async () => {
  const { ClusterEngine } = await import('../src/cluster.js');
  const { LinksError } = await import('../src/errors.js');
  const events = []; let updates;
  class Native {
    begin() { return { request: () => '(count: 0)', commit() {}, rollback() {}, free() { events.push('session'); } }; }
    journal() { return '["new"]'; }
    free() { events.push('engine'); }
  }
  const store = { supportsDurableFlush: true, read: async (keys) => emptySnapshot(keys), compareExchange: async (_keys, _expected, value) => { updates = value; return true; } };
  const cluster = new ClusterEngine({ StagedEngine: Native }, store, { concurrency: 'optimistic', isolation: 'serializable', requireDurableFlush: true });
  const tx = await cluster.begin(['a']); tx.shard('a').require('durableFlush');
  await tx.commit(); assert.deepEqual(updates, [['new']]); await tx[Symbol.asyncDispose]();
  const failure = new LinksError('unknown peer outcome', { outcome: 'unknown' });
  store.compareExchange = async () => { throw failure; };
  const uncertain = await cluster.begin(['a']); await assert.rejects(uncertain.commit(), (cause) => cause === failure);
  class BrokenFree extends Native { free() { throw Error('engine free failed'); } }
  const broken = await new ClusterEngine({ StagedEngine: BrokenFree }, store).begin(['a', 'b']);
  await assert.rejects(broken.close(), /engine free failed/);
  const rolledBack = await cluster.begin(['a']); await rolledBack.rollback(); await rolledBack.close();
});

test('cleanup errors cannot replace an unknown commit outcome', async () => {
  const { ClusterEngine } = await import('../src/cluster.js');
  const { LinksError } = await import('../src/errors.js');
  const events = []; const failure = new LinksError('lost response', { outcome: 'unknown' });
  const store = { read: async (keys) => emptySnapshot(keys), compareExchange: async () => { throw failure; } };
  const tx = await new ClusterEngine(bindingWith(events, { failFree: true }), store).begin(['a', 'b']);
  await assert.rejects(tx.commit(), (cause) => cause === failure);
  assert.deepEqual(events, ['session 0', 'engine 0', 'session 1', 'engine 1']);
});

test('cleanup failure after an acknowledged commit is reported without replaying publication', async () => {
  const { ClusterEngine } = await import('../src/cluster.js');
  const events = []; let publications = 0;
  const store = { read: async (keys) => emptySnapshot(keys), compareExchange: async () => { publications++; return true; } };
  const tx = await new ClusterEngine(bindingWith(events, { failFree: true }), store).begin(['a']);
  await assert.rejects(tx.commit(), /free failed/); assert.equal(publications, 1);
  await assert.rejects(tx.commit(), /closed/); assert.equal(publications, 1);
});
