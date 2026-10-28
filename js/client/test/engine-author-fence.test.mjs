import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { AuthorFence } from '../src/engine-author-fence.js';
const A = '0'.repeat(31) + '1', token = n => BigInt(n).toString(16).padStart(32, '0');
class Store {
  supportsDurableFlush = true;
  values = new Map(); calls = 0; before; failure;
  async read(keys) { return keys.map(key => structuredClone(this.values.get(key) ?? { generation: '0', entries: [] })); }
  async compareExchange(keys, expected, updates) {
    const before = this.before; this.before = undefined; await before?.();
    this.calls++;
    const failure = this.failure; this.failure = undefined;
    if (failure === 'before') throw Error('before');
    if (keys.some((key, i) => (this.values.get(key)?.generation ?? '0') !== expected[i].generation)) return false;
    keys.forEach((key, i) => { if (updates[i] !== null) this.values.set(key, { generation: (BigInt(expected[i].generation) + 1n).toString(), entries: [...updates[i]] }); });
    if (failure === 'after') throw Error('after');
    return true;
  }
}
const fence = (store, options) => new AuthorFence(store, 'fence', A, options);
const bytes = store => JSON.stringify([...store.values]);
const vectors = JSON.parse(readFileSync(new URL('../../../docs/protocol/author-fence-v1.json', import.meta.url)));
test('independent canonical claim journal fixtures are owned and read-only', async () => {
  for (const vector of vectors.cases) {
    const store = new Store(); store.values.set('fence', vector.journal); const before = bytes(store);
    if (vector.valid) assert.deepEqual(await fence(store).snapshot(), { generation: vector.journal.generation, epoch: vector.epoch, claims: vector.claims });
    else await assert.rejects(fence(store).snapshot(), undefined, vector.name);
    assert.equal(bytes(store), before); assert.equal(store.calls, 0);
  }
});
test('non-ASCII wire records are refused before the JSON parser is called', async () => {
  const store = new Store(), f = fence(store);
  const header = JSON.stringify(['author-fence1', A]);
  // 100 UTF-16 units, 300 UTF-8 bytes: the byte budget precedes parsing.
  store.values.set('fence', {generation:'1', entries:[header, '雪'.repeat(100)]});
  const before = bytes(store);
  const original = JSON.parse; let calls = 0;
  JSON.parse = () => { calls++; throw Error('JSON parser was reached'); };
  try { await assert.rejects(f.snapshot(), TypeError); }
  finally { JSON.parse = original; }
  assert.equal(calls, 0);
  assert.equal(bytes(store), before); assert.equal(store.calls, 0);
});
test('explicit takeover fences old reads and publications without undoing earlier data', async () => {
  const store = new Store(), coordinator = fence(store), first = await coordinator.claim('0', token(10));
  const old = coordinator.bind(first); first.token = token(99);
  const expected = await old.read(['data']);
  assert.equal(await old.compareExchange(['data'], expected, [['one']]), true);
  const second = await coordinator.claim('1', token(20)), current = coordinator.bind(second);
  const before = bytes(store), calls = store.calls;
  await assert.rejects(old.read(['data']), /fenced/i);
  await assert.rejects(old.compareExchange(['data'], expected, [['stale']]), /fenced/i);
  await assert.rejects(coordinator.claim('0', token(10)), /fenced/i);
  assert.equal(bytes(store), before); assert.equal(store.calls, calls);
  assert.deepEqual((await current.read(['data']))[0].entries, ['one']);
  const snapshot = await coordinator.snapshot(); snapshot.claims[1].token = token(99);
  assert.equal((await coordinator.snapshot()).claims[1].token, token(20));
});
test('a takeover inside guarded CAS conflicts atomically and explicit retry is fenced', async () => {
  const store = new Store(), coordinator = fence(store), old = coordinator.bind(await coordinator.claim('0', token(10)));
  const expected = await old.read(['a', 'b']);
  store.before = () => coordinator.claim('1', token(20));
  assert.equal(await old.compareExchange(['a', 'b'], expected, [['a'], ['b']]), false);
  assert.equal(store.values.has('a'), false); assert.equal(store.values.has('b'), false);
  await assert.rejects(old.compareExchange(['a', 'b'], expected, [['a'], ['b']]), /fenced/i);
});
test('same-base claim races and uncertain replies never blindly retake ownership', async () => {
  const store = new Store(), owner = fence(store);
  const races = await Promise.allSettled([owner.claim('0', token(10)), owner.claim('0', token(20))]);
  assert.equal(races.filter(v => v.status === 'fulfilled').length, 1);
  const winner = races.find(v => v.status === 'fulfilled').value;
  assert.deepEqual(await owner.claim('0', winner.token), winner);
  for (const failure of ['before', 'after']) {
    const s = new Store(), f = fence(s); s.failure = failure;
    await assert.rejects(f.claim('0', token(10)), /indeterminate/i); assert.equal(s.calls, 1);
    const grant = await f.claim('0', token(10)); assert.equal(grant.epoch, '1');
    await f.claim('1', token(20)); const before = bytes(s), calls = s.calls;
    await assert.rejects(f.claim('0', token(10)), /fenced/i);
    assert.equal(bytes(s), before); assert.equal(s.calls, calls);
  }
});
test('finite budgets and maximum generation preserve retained active retries', async () => {
  for (const options of [{ maxEntries: 2 }, { maxBytes: 160 }]) {
    const s = new Store(), f = fence(s, options), first = await f.claim('0', token(10));
    const before = bytes(s), calls = s.calls;
    await assert.rejects(f.claim('1', token(20)));
    assert.deepEqual(await f.claim('0', token(10)), first);
    assert.equal(bytes(s), before); assert.equal(s.calls, calls);
  }
  const s = new Store(), f = fence(s), first = await f.claim('0', token(10));
  s.values.get('fence').generation = '18446744073709551615';
  await assert.rejects(f.claim('1', token(20)));
  assert.deepEqual(await f.claim('0', token(10)), first);
});
test('guarded read-only CAS preserves the fence generation and provider ownership', async () => {
  const s = new Store(), f = fence(s), grant = await f.claim('0', token(10)), guarded = f.bind(grant);
  const before = bytes(s), expected = await guarded.read(['data']);
  assert.equal(await guarded.compareExchange(['data'], expected, [null]), true);
  assert.equal(bytes(s), before); assert.equal(guarded.supportsDurableFlush, true);
  assert.equal(guarded.intoStore(), s);
});
test('invalid options, requests, snapshots and guard frames cannot publish', async () => {
  const s = new Store();
  for (const args of [[null,'fence',A], [s,'bad/key',A], [s,'fence',token(0)], [s,'fence',A,{maxEntries:1}],
    [s,'fence',A,{maxBytes:0}], [s,'fence',A,{requireDurableFlush:1}]]) assert.throws(() => new AuthorFence(...args));
  s.supportsDurableFlush = false; assert.throws(() => fence(s, {requireDurableFlush:true}));
  assert.equal(fence(s).bind({author:A,token:token(10),epoch:'1'}).supportsDurableFlush, false);
  s.supportsDurableFlush = true; assert.doesNotThrow(() => fence(s, {requireDurableFlush:true}));
  const f = fence(s);
  for (const epoch of ['01', '-1', '', '18446744073709551616', 0]) await assert.rejects(f.claim(epoch, token(10)));
  await assert.rejects(f.claim('1', token(10)), /conflict/i);
  for (const claim of [null, {}, {author:A,token:token(10),epoch:'0'}, {author:A,token:'bad',epoch:'1'}]) assert.throws(() => f.bind(claim));
  for (const values of [null, [], [null], [{generation:'0',entries:null}]]) {
    const malformed = new Store(); malformed.read = async () => values;
    await assert.rejects(fence(malformed).snapshot()); assert.equal(malformed.calls, 0);
  }
  const guarded = f.bind(await f.claim('0', token(10)));
  for (const keys of [null, [], ['fence'], ['x','x'], ['bad/key'], Array.from({length:32}, (_,i) => `x${i}`)]) await assert.rejects(guarded.read(keys));
  for (const [expected, updates] of [[null,[]], [[],[]], [[{}],null], [[{}],[]]]) await assert.rejects(guarded.compareExchange(['data'], expected, updates));
  for (const reply of ['yes', 0, undefined]) {
    const malformed = new Store(); malformed.compareExchange = async () => reply;
    await assert.rejects(fence(malformed).claim('0', token(10)), /indeterminate/i);
  }
  s.compareExchange = async () => ({});
  await assert.rejects(guarded.compareExchange(['data'], [{generation:'0',entries:[]}], [['new']]), /indeterminate/i);
});
test('guard captures selected keys, expected frames and writes before its first await', async () => {
  const s = new Store(), f = fence(s), guarded = f.bind(await f.claim('0', token(10)));
  const keys = ['data'], expected = await guarded.read(keys), updates = [['one']];
  const pending = guarded.compareExchange(keys, expected, updates);
  keys[0] = 'fence'; expected[0].generation = '99'; updates[0][0] = 'changed';
  assert.equal(await pending, true); assert.deepEqual(s.values.get('data').entries, ['one']);
});
test('new owner resolves a decided batch and burns the previous grant suffix', async () => {
  const { CausalBatch } = await import('../src/engine-causal-batch.js');
  const { AddressAllocator } = await import('../src/engine-address-allocator.js');
  const s = new Store(), f = fence(s), first = f.bind(await f.claim('0', token(10)));
  assert.equal((await new AddressAllocator(first, 'addresses', '1', '100').reserve(A, token(110), '4')).last, '4');
  const old = new CausalBatch(first, 'owner', 'receipts', A), edits = [{address:'1',observed:[],payload:{source:'0',target:'0',name:null}}];
  s.failure = 'after'; await assert.rejects(old.write(token(300), '0', edits), /indeterminate/i);
  const next = f.bind(await f.claim('1', token(20))), before = bytes(s);
  await assert.rejects(old.write(token(300), '0', edits), /fenced/i); assert.equal(bytes(s), before);
  const current = new CausalBatch(next, 'owner', 'receipts', A);
  assert.equal((await current.write(token(300), '0', edits))[0].dot.counter, '1');
  const grant = await new AddressAllocator(next, 'addresses', '1', '100').reserve(A, token(210), '2');
  assert.equal(grant.first, '5'); const epoch = structuredClone(s.values.get('fence'));
  await current.write(token(400), (await current.snapshot()).generation, [{...edits[0],address:grant.first}]);
  assert.equal((await current.snapshot()).lastCounter, '2'); assert.deepEqual(s.values.get('fence'), epoch);
});
