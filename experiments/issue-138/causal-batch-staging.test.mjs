// Finite composition experiment, not the production batch wire or session API.
import test from 'node:test';
import assert from 'node:assert/strict';
import { CausalReplica } from '../../js/client/src/engine-causal-replica.js';
import { CausalVersion } from '../../js/client/src/engine-causal-version.js';
import { publishBatch } from './causal-batch-staging.mjs';

const author = '00000000000000000000000000000001';
const other = '00000000000000000000000000000002';
const token = n => BigInt(n).toString(16).padStart(32, '0');
const payload = name => ({ source: '0', target: '2', name });
const edits = () => [
  { address: '1', observed: [], payload: payload('\ufeff雪😀\0') },
  { address: '2', observed: [], payload: payload('second') },
];
class Store {
  supportsDurableFlush = true;
  values = new Map(); calls = 0; failure;
  async read(keys) {
    return keys.map(key => structuredClone(this.values.get(key) ?? { generation: '0', entries: [] }));
  }
  async compareExchange(keys, expected, updates) {
    this.calls++;
    if (this.failure === 'before') { this.failure = undefined; throw new Error('before'); }
    if (keys.some((key, i) => (this.values.get(key)?.generation ?? '0') !== expected[i].generation)) return false;
    // Commit every key together. No await or callback divides the publication.
    keys.forEach((key, i) => this.values.set(key, {
      generation: (BigInt(expected[i].generation) + 1n).toString(), entries: [...updates[i]],
    }));
    if (this.failure === 'after') { this.failure = undefined; throw new Error('after'); }
    return true;
  }
}
const owner = store => new CausalReplica(store, 'replica', author);
const generation = async store => (await store.read(['replica']))[0].generation;
const bytes = store => JSON.stringify([...store.values].sort());

test('stage existing owner records and publish two keys at one boundary', async () => {
  const store = new Store();
  const result = await publishBatch(store, token(100), '0', edits());
  assert.equal(store.calls, 1);
  assert.deepEqual(result.map(v => v.dot.counter), ['1', '2']);
  assert.deepEqual((await owner(store).snapshot()).versions, result);
  const raw = store.values.get('replica').entries.map(JSON.parse);
  assert.deepEqual(raw.map(value => value[0]), ['causal-replica1', 'causal-write1', 'causal-write1']);
  assert.equal(raw[1][1], token(100)); assert.equal(raw[2][1], token(101));
  result[0].payload.name = 'borrowed mutation';
  assert.equal((await owner(store).read('1'))[0].payload.name, '\ufeff雪😀\0');
});

test('a failing later edit publishes neither the earlier edit nor a receipt', async () => {
  const store = new Store(), replica = owner(store);
  const prior = CausalVersion.advance('2', other, '0', [], payload('known'));
  await replica.receive([prior]);
  const invalid = structuredClone(prior); invalid.payload = null;
  const request = edits(); request[1].observed = [invalid];
  const before = bytes(store), calls = store.calls;
  await assert.rejects(publishBatch(store, token(100), await generation(store), request));
  assert.equal(bytes(store), before); assert.equal(store.calls, calls);
  assert.equal((await replica.snapshot()).lastCounter, '0');
  assert.deepEqual(await replica.read('1'), []);
});

test('two stages from the same base cannot both publish', async () => {
  const store = new Store();
  const results = await Promise.allSettled([
    publishBatch(store, token(100), '0', edits()),
    publishBatch(store, token(200), '0', edits()),
  ]);
  assert.equal(results.filter(result => result.status === 'fulfilled').length, 1);
  assert.match(results.find(result => result.status === 'rejected').reason.message, /conflict/i);
  assert.equal((await owner(store).snapshot()).lastCounter, '2');
});

test('receipt retries survive lost replies, later writes and fresh handles', async () => {
  const store = new Store(); store.failure = 'after';
  await assert.rejects(publishBatch(store, token(100), '0', edits()), /indeterminate/i);
  assert.equal(store.calls, 1);
  await owner(store).write(token(200), '3', [], payload('following'));
  const calls = store.calls, before = bytes(store);
  const result = await publishBatch(store, token(100), '0', edits());
  assert.deepEqual(result.map(v => v.dot.counter), ['1', '2']);
  assert.equal(bytes(store), before); assert.equal(store.calls, calls);
  const changed = edits(); changed[1].payload = null;
  await assert.rejects(publishBatch(store, token(100), '0', changed));
  await assert.rejects(publishBatch(store, token(100), await generation(store), edits()));
  assert.equal(bytes(store), before); assert.equal(store.calls, calls);
});

test('before-publication retry needs the same base and performs no blind retry', async () => {
  const store = new Store(); store.failure = 'before';
  await assert.rejects(publishBatch(store, token(100), '0', edits()), /indeterminate/i);
  assert.equal(store.calls, 1); assert.equal(store.values.size, 0);
  const result = await publishBatch(store, token(100), '0', edits());
  assert.deepEqual(result.map(v => v.dot.counter), ['1', '2']); assert.equal(store.calls, 2);
  const before = bytes(store), calls = store.calls;
  await assert.rejects(publishBatch(store, token(300), '0', edits()), /conflict/i);
  assert.equal(bytes(store), before); assert.equal(store.calls, calls);
});

test('a child-token collision never reuses an old mutation as a new batch result', async () => {
  const store = new Store();
  await owner(store).write(token(100), '1', [], payload('\ufeff雪😀\0'));
  const before = bytes(store), calls = store.calls;
  await assert.rejects(publishBatch(store, token(100), await generation(store), edits()), /collision/i);
  assert.equal(bytes(store), before); assert.equal(store.calls, calls);
  assert.equal((await owner(store).snapshot()).lastCounter, '1');
});

test('an empty batch publishes a valid header and a retry receipt', async () => {
  const store = new Store();
  assert.deepEqual(await publishBatch(store, token(100), '0', []), []);
  assert.deepEqual(await owner(store).snapshot(), { author, lastCounter: '0', versions: [] });
  const calls = store.calls, before = bytes(store);
  assert.deepEqual(await publishBatch(store, token(100), '0', []), []);
  assert.equal(bytes(store), before); assert.equal(store.calls, calls);
});

test('staging bounds and malformed publication results have finite outcomes', async () => {
  const store = new Store(), before = bytes(store);
  const missing = edits(); missing.push({ address: '3', observed: [], payload: null }, { address: '4', observed: [], payload: null });
  await assert.rejects(publishBatch(store, token(100), '0', missing));
  await assert.rejects(publishBatch(store, token(2n ** 128n - 1n), '0', edits()));
  const duplicated = edits(); duplicated[1].address = '1';
  await assert.rejects(publishBatch(store, token(100), '0', duplicated));
  assert.equal(bytes(store), before); assert.equal(store.calls, 0);
  store.compareExchange = async () => undefined;
  await assert.rejects(publishBatch(store, token(100), '0', edits()), /indeterminate/i);
  assert.equal(bytes(store), before);
});
