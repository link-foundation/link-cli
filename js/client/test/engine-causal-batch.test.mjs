// Local atomic publication and independently generated receipt-admission contracts.
import test from 'node:test';
import assert from 'node:assert/strict';
import { CausalReplica } from '../src/engine-causal-replica.js';
import { CausalVersion } from '../src/engine-causal-version.js';
import { CausalBatch } from '../src/engine-causal-batch.js';
import { readFileSync } from 'node:fs';
import { PhysicalLsmJournalStore } from '../src/engine-lsm-store.js';
import { LsmHarness, selected } from './support/lsm-store.mjs';
const publishBatch = (store, token, generation, edits) =>
  new CausalBatch(store, 'replica', 'batch-receipts', author).write(token, generation, edits);

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

test('physical provider generations remain opaque across receipt-only batches', async () => {
  const harness = new LsmHarness();
  const store = PhysicalLsmJournalStore.open(harness.wal, harness.provider,
    { ...selected, maxBytes: 65536, bufferBytes: 32768 });
  const batch = new CausalBatch(store, 'replica', 'batch-receipts', author);
  try {
    await batch.write(token(100), '0', edits());
    await batch.write(token(200), '1', []);
    const before = store.read(['replica', 'batch-receipts']);
    assert.deepEqual(before.map(value => value.generation), ['1', '2']);
    assert.equal((await batch.snapshot()).generation, '1');
    await batch.write(token(300), (await batch.snapshot()).generation, [
      { address: '3', observed: [], payload: payload('following empty receipt') },
    ]);
    assert.equal((await batch.snapshot()).generation, '3');
    const committed = store.read(['replica', 'batch-receipts']);
    assert.deepEqual(await batch.write(token(200), '1', []), []);
    assert.deepEqual(store.read(['replica', 'batch-receipts']), committed);
  } finally { store.close(); }
});

test('caller edits become owned before the first asynchronous store read', async () => {
  const store = new Store(), request = edits();
  const pending = publishBatch(store, token(100), '0', request);
  request[0].payload.name = 'changed after admission';
  request[1].address = '9'; request[1].observed.push({ invalid: true });
  const result = await pending;
  assert.equal(result[0].payload.name, '\ufeff雪😀\0');
  assert.equal(result[1].address, '2');
  assert.deepEqual(await publishBatch(store, token(100), '0', edits()), result);
});

test('staging bounds and malformed publication results have finite outcomes', async () => {
  const store = new Store(), before = bytes(store);
  const missing = edits(); for (let address = 3; address <= 65; address++) missing.push({ address: String(address), observed: [], payload: null });
  await assert.rejects(publishBatch(store, token(100), '0', missing));
  await assert.rejects(publishBatch(store, token(2n ** 128n - 1n), '0', edits()));
  const duplicated = edits(); duplicated[1].address = '1';
  await assert.rejects(publishBatch(store, token(100), '0', duplicated));
  assert.equal(bytes(store), before); assert.equal(store.calls, 0);
  store.compareExchange = async () => undefined;
  await assert.rejects(publishBatch(store, token(100), '0', edits()), /indeterminate/i);
  assert.equal(bytes(store), before);
});

test('selected keys, malformed provider snapshots and preparse bounds reject unchanged', async () => {
  for (const key of ['', 'replica', 'bad/key', 1])
    assert.throws(() => new CausalBatch(new Store(), 'replica', key, author));
  const store = new Store();
  for (const result of [[], null, [null, null], [{ generation: '01', entries: [] }, {}],
    [{ generation: '0', entries: [] }, { generation: '0', entries: [1] }]]) {
    store.read = async () => result;
    await assert.rejects(publishBatch(store, token(100), '0', edits()));
    assert.equal(store.calls, 0);
  }
  const normal = new Store(), batch = new CausalBatch(normal, 'replica', 'batch-receipts', author);
  const header = JSON.stringify(['causal-batches1', 'replica', author]);
  const parse = JSON.parse;
  try {
    JSON.parse = () => assert.fail('inadmissible frame reached JSON parsing');
    for (const frame of ['[', ']', '\\', '\x1f', '\x7f', '"', '}',
      '[' + Array(32770).fill('null').join(',') + ']', '"' + '1'.repeat(1048575) + '"']) {
      normal.values.set('batch-receipts', { generation: '1', entries: [header, frame] });
      await assert.rejects(batch.snapshot(), /invalid causal batch state/);
      assert.equal(normal.calls, 0);
    }
  } finally { JSON.parse = parse; }
});

test('following batches append independent receipts and retain original retry results', async () => {
  const store = new Store();
  const first = await publishBatch(store, token(100), '0', edits());
  const second = await publishBatch(store, token(200), '1', [
    { address: '1', observed: [first[0]], payload: null },
    { address: '3', observed: [], payload: payload('following batch') },
  ]);
  assert.deepEqual(second.map(value => value.dot.counter), ['3', '4']);
  assert.deepEqual(await publishBatch(store, token(100), '0', edits()), first);
  assert.deepEqual((await new CausalBatch(store, 'replica', 'batch-receipts', author).snapshot()).versions, [second[0], first[1], second[1]]);
});

test('all selected batch dimensions admit their maxima and journal exhaustion has no effects', async () => {
  const store = new Store();
  const request = Array.from({ length: 64 }, (_, index) => ({ address: String(index + 1), observed: [], payload: null }));
  const result = await publishBatch(store, token(100), '0', request);
  assert.equal(result.length, 64); assert.equal(result[63].dot.counter, '64');
  assert.equal(store.calls, 1);
  const limited = new Store(), batch = new CausalBatch(limited, 'replica', 'batch-receipts', author, { maxEntries: 3 });
  await batch.write(token(100), '0', edits());
  await batch.write(token(200), '1', []);
  const before = bytes(limited), calls = limited.calls;
  await assert.rejects(batch.write(token(300), '2', []));
  assert.equal(bytes(limited), before); assert.equal(limited.calls, calls);
  const tooSmall = new Store();
  await assert.rejects(new CausalBatch(tooSmall, 'replica', 'batch-receipts', author, { maxEntries: 2 }).write(token(100), '0', edits()));
  assert.equal(tooSmall.values.size, 0); assert.equal(tooSmall.calls, 0);
});

test('retry validates dominated historical observations even when normalized request matches', async () => {
  const store = new Store();
  const old = CausalVersion.advance('1', other, '0', [], payload('old'));
  const newer = CausalVersion.advance('1', other, '1', [old], payload('new'));
  const request = [{ address: '1', observed: [old, newer], payload: null }];
  const first = await publishBatch(store, token(100), '0', request);
  assert.deepEqual(await publishBatch(store, token(100), '0', [{ ...request[0], observed: [newer] }]), first);
  const before = bytes(store), calls = store.calls;
  const forged = structuredClone(old); forged.payload = null;
  await assert.rejects(publishBatch(store, token(100), '0', [{ ...request[0], observed: [forged, newer] }]));
  assert.equal(bytes(store), before); assert.equal(store.calls, calls);
});


test('independent batch receipt vectors reject mismatched owner evidence without publication', async () => {
  const fixtures = JSON.parse(readFileSync(new URL('../../../docs/protocol/causal-batch-v1.json', import.meta.url)));
  for (const fixture of fixtures.cases) {
    const store = new Store();
    store.values.set(fixtures.ownerKey, structuredClone(fixture.owner));
    store.values.set(fixtures.receiptKey, structuredClone(fixture.receipts));
    const batch = new CausalBatch(store, fixtures.ownerKey, fixtures.receiptKey, fixtures.author, fixture.options);
    const before = bytes(store);
    if (fixture.valid) {
      const snapshot = await batch.snapshot();
      assert.equal(snapshot.generation, fixture.owner.generation, fixture.name);
      assert.equal(snapshot.lastCounter, fixture.lastCounter, fixture.name);
      assert.deepEqual(snapshot.versions.map(CausalVersion.encode), fixture.versions.map(value => JSON.stringify(value)), fixture.name);
    } else await assert.rejects(batch.snapshot(), undefined, fixture.name);
    assert.equal(store.calls, 0, fixture.name); assert.equal(bytes(store), before, fixture.name);
  }
});
