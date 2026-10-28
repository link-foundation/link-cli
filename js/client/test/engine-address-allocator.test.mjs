import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { AddressAllocator } from '../src/engine-address-allocator.js';

const A = '00000000000000000000000000000001', B = '00000000000000000000000000000002';
const token = n => BigInt(n).toString(16).padStart(32, '0');
const maximum = '18446744073709551615';
class Store {
  supportsDurableFlush = true;
  value = { generation: '0', entries: [] };
  calls = 0; failure; before;
  async read() { return structuredClone([this.value]); }
  async compareExchange(keys, expected, updates) {
    const before = this.before; this.before = undefined; await before?.();
    this.calls++;
    const failure = this.failure; this.failure = undefined;
    if (failure === 'before') throw new Error('before publication');
    if (expected[0].generation !== this.value.generation) return false;
    assert.deepEqual(keys, ['addresses']);
    this.value = { generation: (BigInt(this.value.generation) + 1n).toString(), entries: [...updates[0]] };
    if (failure === 'after') throw new Error('lost publication reply');
    return true;
  }
}
const allocator = (store, first = '1', last = '20', options) => new AddressAllocator(store, 'addresses', first, last, options);
const bytes = store => JSON.stringify(store.value);
const fixtures = JSON.parse(readFileSync(new URL('../../../docs/protocol/address-reservations-v1.json', import.meta.url)));

test('independent canonical journal and admission vectors preserve provider bytes', async () => {
  for (const fixture of fixtures.cases) {
    const store = new Store(); store.value = structuredClone(fixture.journal);
    const owner = allocator(store, fixture.first, fixture.last, fixture.options), before = bytes(store);
    if (fixture.valid) assert.deepEqual(await owner.snapshot(), {
      generation: fixture.journal.generation, next: fixture.next, reservations: fixture.reservations,
    }, fixture.name);
    else await assert.rejects(owner.snapshot(), undefined, fixture.name);
    assert.equal(bytes(store), before); assert.equal(store.calls, 0);
  }
});

test('exact author/token/count retries survive uncertain replies and following grants', async () => {
  for (const failure of [undefined, 'before', 'after']) {
    const store = new Store(); store.failure = failure;
    if (failure) await assert.rejects(allocator(store).reserve(A, token(10), '3'), /indeterminate/i);
    else assert.equal((await allocator(store).reserve(A, token(10), '3')).last, '3');
    assert.equal(store.calls, 1);
    const first = await allocator(store).reserve(A, token(10), '3');
    assert.deepEqual(first, { author: A, token: token(10), count: '3', first: '1', last: '3' });
    assert.equal((await allocator(store).reserve(B, token(20), '2')).first, '4');
    const calls = store.calls, before = bytes(store);
    assert.deepEqual(await allocator(store).reserve(A, token(10), '3'), first);
    await assert.rejects(allocator(store).reserve(B, token(10), '3'));
    await assert.rejects(allocator(store).reserve(A, token(10), '2'));
    first.first = '19'; const snapshot = await allocator(store).snapshot();
    snapshot.reservations[0].last = '19';
    assert.equal((await allocator(store).snapshot()).reservations[0].last, '3');
    assert.equal(bytes(store), before); assert.equal(store.calls, calls);
  }
});

test('same-base races grant one range and explicit retry grants a disjoint next range', async () => {
  const store = new Store();
  const outcomes = await Promise.allSettled([allocator(store).reserve(A, A, '2'), allocator(store).reserve(B, B, '2')]);
  assert.equal(outcomes.filter(v => v.status === 'fulfilled').length, 1);
  assert.match(outcomes.find(v => v.status === 'rejected').reason.message, /conflict/i);
  assert.equal(outcomes.find(v => v.status === 'rejected').reason.outcome, 'failed');
  await allocator(store).reserve(A, A, '2'); await allocator(store).reserve(B, B, '2');
  assert.deepEqual((await allocator(store).snapshot()).reservations.map(v => [v.first, v.last]), [['1', '2'], ['3', '4']]);
  const next = new Store(); next.before = () => allocator(next).reserve(B, B, '2');
  await assert.rejects(allocator(next).reserve(A, A, '2'), /conflict/i);
  assert.equal((await allocator(next).reserve(A, A, '2')).first, '3');
});

test('only an explicit true publication reply can acknowledge a new grant', async () => {
  for (const response of ['true', {}, [], null, undefined, 0]) {
    const store = new Store(), before = bytes(store);
    store.compareExchange = async () => { store.calls++; return response; };
    await assert.rejects(allocator(store).reserve(A, A, '1'), error =>
      /indeterminate/i.test(error.message) && error.outcome === 'unknown');
    assert.equal(store.calls, 1); assert.equal(bytes(store), before);
  }
});

test('full-width unsigned domains stop at their last value and never cycle', async () => {
  const store = new Store(), owner = allocator(store, '18446744073709551614', maximum);
  assert.equal((await owner.reserve(A, A, '1')).first, '18446744073709551614');
  assert.equal((await owner.reserve(B, B, '1')).last, maximum);
  const before = bytes(store), calls = store.calls;
  assert.equal((await owner.snapshot()).next, null);
  await assert.rejects(owner.reserve(A, token(3), '1'));
  assert.equal((await owner.reserve(B, B, '1')).last, maximum);
  assert.equal(bytes(store), before); assert.equal(store.calls, calls);
  const whole = allocator(new Store(), '1', maximum);
  assert.equal((await whole.reserve(A, A, maximum)).last, maximum);
});

test('finite entry/byte budgets still permit retained retries without appending', async () => {
  for (const options of [{ maxEntries: 2 }, { maxBytes: 170 }]) {
    const store = new Store(), owner = allocator(store, '1', '20', options);
    await owner.reserve(A, A, '1'); const before = bytes(store), calls = store.calls;
    await assert.rejects(owner.reserve(B, B, '1'));
    assert.equal((await owner.reserve(A, A, '1')).first, '1');
    assert.equal(bytes(store), before); assert.equal(store.calls, calls);
  }
  const tiny = new Store(); await assert.rejects(allocator(tiny, '1', '20', { maxBytes: 1 }).reserve(A, A, '1'));
  assert.equal(tiny.calls, 0);
});

test('bad domains, providers and options reject before provider access', () => {
  const store = new Store();
  for (const [first, last] of [['0','1'], ['2','1'], ['01','2'], ['1','18446744073709551616'], [1,'2'], ['-1','2'], ['','2']])
    assert.throws(() => allocator(store, first, last));
  for (const key of ['', 'bad/key', 'x'.repeat(129), null]) assert.throws(() => new AddressAllocator(store, key, '1', '2'));
  for (const provider of [null, {}, { read() {} }, { compareExchange() {} }]) assert.throws(() => allocator(provider));
  for (const options of [{ maxEntries: 1 }, { maxEntries: 4097 }, { maxEntries: 2.5 }, { maxBytes: 0 }, { maxBytes: 1048577 }, { maxBytes: 1.5 }, { requireDurableFlush: 1 }])
    assert.throws(() => allocator(store, '1', '20', options));
  store.supportsDurableFlush = false;
  assert.throws(() => allocator(store, '1', '20', { requireDurableFlush: true }));
  assert.doesNotThrow(() => allocator(store));
  store.supportsDurableFlush = true;
  assert.doesNotThrow(() => allocator(store, '1', '20', { requireDurableFlush: true }));
});

test('bad requests and incoherent provider replies cannot publish', async () => {
  const store = new Store();
  for (const [author, id, count] of [[null,A,'1'], ['0'.repeat(32),A,'1'], ['F'.repeat(32),A,'1'], [A,'bad','1'], [A,A,'0'], [A,A,'01'], [A,A,1], [A,A,'18446744073709551616']])
    await assert.rejects(allocator(store).reserve(author, id, count));
  assert.equal(store.calls, 0);
  for (const result of [null, [], [null], [{ generation: '01', entries: [] }], [{ generation: '0', entries: null }], [{ generation: '1', entries: [null] }], [{ generation: '1', entries: [1] }]]) {
    const provider = new Store(); provider.read = async () => result;
    await assert.rejects(allocator(provider).reserve(A, A, '1')); assert.equal(provider.calls, 0);
  }
  store.value.generation = maximum;
  store.value.entries = ['["address-reservations1","1","20"]'];
  await assert.rejects(allocator(store).reserve(A, A, '1')); assert.equal(store.calls, 0);
});

test('canonical fields reject every trailing line terminator before publication', async () => {
  for (const ending of ['\n', '\r', '\r\n', '\u2028', '\u2029']) {
    const store = new Store(), before = bytes(store);
    assert.throws(() => new AddressAllocator(store, `addresses${ending}`, '1', '20'));
    assert.throws(() => allocator(store, `1${ending}`, '20'));
    assert.throws(() => allocator(store, '1', `20${ending}`));
    await assert.rejects(allocator(store).reserve(`${A}${ending}`, A, '1'));
    await assert.rejects(allocator(store).reserve(A, `${A}${ending}`, '1'));
    await assert.rejects(allocator(store).reserve(A, A, `1${ending}`));
    store.value.generation = `0${ending}`;
    await assert.rejects(allocator(store).snapshot());
    store.value.generation = '0';
    assert.equal(store.calls, 0); assert.equal(bytes(store), before);
  }
});
