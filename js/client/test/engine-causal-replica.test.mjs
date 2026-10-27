import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { CausalReplica } from '../src/engine-causal-replica.js';
import { CausalVersion } from '../src/engine-causal-version.js';

const alice = '00000000000000000000000000000001';
const bob = '00000000000000000000000000000002';
const token = n => n.toString(16).padStart(32, '0');
const row = name => ({ source: '2', target: '3', name });
const wires = versions => versions.map(value => CausalVersion.encode(value));

class MemoryStore {
  supportsDurableFlush = true;
  generation = 0n; entries = []; calls = 0; failure;
  async read(keys) {
    assert.deepEqual(keys, ['replica']);
    return [{ generation: this.generation.toString(), entries: [...this.entries] }];
  }
  async compareExchange(keys, expected, updates) {
    assert.deepEqual(keys, ['replica']); this.calls++;
    if (this.failure === 'before') { this.failure = undefined; throw new Error('before publication'); }
    if (expected[0].generation !== this.generation.toString()) return false;
    this.entries = [...updates[0]]; this.generation++;
    if (this.failure === 'after') { this.failure = undefined; throw new Error('lost publication reply'); }
    return true;
  }
}
const replica = (store = new MemoryStore(), author = alice, options = {}) =>
  new CausalReplica(store, 'replica', author, options);

test('durable retry identity survives later edits and reopening without reissuing a mutation', async () => {
  const store = new MemoryStore(), first = replica(store);
  const initial = await first.write(token(1), '1', [], row('initial'));
  assert.equal(initial.dot.counter, '1');
  const edited = await first.write(token(2), '1', [initial], row('edited'));
  assert.equal(edited.dot.counter, '2');
  const reopened = replica(store), calls = store.calls;
  assert.deepEqual(await reopened.write(token(1), '1', [], row('initial')), initial);
  assert.equal(store.calls, calls);
  assert.deepEqual(wires(await reopened.read('1')), wires([edited]));
  await assert.rejects(reopened.write(token(1), '1', [], row('changed retry')));
  await assert.rejects(reopened.write(token(1), '1', [initial], row('initial')));
  assert.equal(store.calls, calls);
  initial.clock[0].counter = '99';
  assert.equal((await reopened.snapshot()).lastCounter, '2');
  assert.equal((await reopened.read('1'))[0].clock[0].counter, '2');
});

test('two active local handles cannot publish the same author counter', async () => {
  const store = new MemoryStore(), left = replica(store), right = replica(store);
  // Both asynchronous reads return generation zero before either CAS runs.
  const results = await Promise.allSettled([
    left.write(token(1), '1', [], row('left')),
    right.write(token(2), '2', [], row('right')),
  ]);
  assert.equal(results.filter(value => value.status === 'fulfilled').length, 1);
  const rejected = results.find(value => value.status === 'rejected');
  assert.match(rejected.reason.message, /conflict/i);
  const second = await right.write(token(2), '2', [], row('right'));
  assert.equal(second.dot.counter, '2');
  assert.equal((await left.snapshot()).lastCounter, '2');
  assert.deepEqual((await left.snapshot()).versions.map(value => value.dot.counter).sort(), ['1', '2']);
});

test('before/after publication failures use durable retry identities without blind CAS retries', async () => {
  for (const failure of ['before', 'after']) {
    const store = new MemoryStore(), owner = replica(store); store.failure = failure;
    await assert.rejects(owner.write(token(1), '1', [], row('uncertain')), /indeterminate/i);
    assert.equal(store.calls, 1);
    const following = await owner.write(token(2), '2', [], row('following'));
    const calls = store.calls;
    const retry = await replica(store).write(token(1), '1', [], row('uncertain'));
    if (failure === 'after') {
      assert.equal(following.dot.counter, '2'); assert.equal(retry.dot.counter, '1');
      assert.equal(store.calls, calls);
    } else {
      assert.equal(following.dot.counter, '1'); assert.equal(retry.dot.counter, '2');
      assert.equal(store.calls, calls + 1);
    }
    assert.equal((await owner.snapshot()).lastCounter, '2');
  }
});

test('independently writable replicas retain concurrent deletion, resolve observed siblings and reject resurrection', async () => {
  const left = replica(), right = replica(new MemoryStore(), bob);
  const initial = await left.write(token(1), '1', [], row('initial'));
  assert.equal(await right.receive([initial]), true);
  const update = await left.write(token(2), '1', [initial], row('update'));
  const deletion = await right.write(token(1), '1', [initial], null);
  await left.receive([deletion]); await right.receive([update]);
  assert.deepEqual(wires(await left.read('1')), wires(await right.read('1')));
  assert.equal((await left.read('1')).length, 2);
  const resolved = await left.write(token(3), '1', await left.read('1'), null);
  await right.receive([resolved]);
  assert.equal(await right.receive([initial]), false);
  assert.deepEqual(wires(await right.read('1')), wires([resolved]));
  assert.equal((await right.snapshot()).lastCounter, '1');
  assert.deepEqual(await right.read('2'), []);
});

test('historical equivocation and foreign local-author counters reject before publication', async () => {
  const store = new MemoryStore(), owner = replica(store);
  const remote = CausalVersion.advance('1', bob, '0', [], row('remote'));
  await owner.receive([remote]);
  await owner.write(token(1), '1', [remote], row('resolved'));
  const resolved = (await owner.read('1'))[0];
  const before = structuredClone(store.entries), calls = store.calls;
  const forged = { ...remote, payload: row('different old payload') };
  await assert.rejects(owner.write(token(2), '1', [forged, resolved], null));
  await assert.rejects(owner.receive([forged]));
  const charlie = token(3), clock = [{ author: bob, counter: '2' }, { author: charlie, counter: '1' }];
  const first = { address: '2', dot: { author: bob, counter: '2' }, clock, payload: null };
  const contradictory = { ...first, dot: { author: charlie, counter: '1' } };
  const isolated = replica(); await isolated.receive([first]);
  await assert.rejects(isolated.receive([contradictory]));
  await assert.rejects(owner.receive([CausalVersion.advance('2', alice, '8', [], null)]));
  await assert.rejects(owner.receive([{ ...remote, address: '2' }]));
  await assert.rejects(owner.receive([remote, { ...remote, address: '0' }]));
  assert.equal(store.calls, calls); assert.deepEqual(store.entries, before);
  const unreceived = new MemoryStore(), observing = replica(unreceived);
  const later = CausalVersion.advance('1', bob, '1', [remote], row('later'));
  const mutation = await observing.write(token(1), '1', [later, remote, remote], null);
  const evidence = [...unreceived.entries], publications = unreceived.calls;
  await assert.rejects(replica(unreceived).receive([forged]));
  await assert.rejects(replica(unreceived).write(token(1), '1', [forged, later], null));
  assert.deepEqual(await observing.write(token(1), '1', [later], null), mutation);
  assert.deepEqual(unreceived.entries, evidence); assert.equal(unreceived.calls, publications);
});

test('bounded admission and invalid local snapshots preserve all stored bytes', async () => {
  for (const options of [{ maxEntries: 2 }, { maxBytes: 180 }]) {
    const store = new MemoryStore(), owner = replica(store, alice, options);
    if (options.maxEntries) await owner.write(token(1), '1', [], row('first'));
    const before = structuredClone(store.entries), calls = store.calls;
    await assert.rejects(owner.write(token(2), '2', [], row('x'.repeat(300))));
    assert.deepEqual(store.entries, before); assert.equal(store.calls, calls);
  }
  const store = new MemoryStore(), owner = replica(store);
  await owner.write(token(1), '1', [], row('first'));
  const original = [...store.entries], calls = store.calls;
  for (const entries of [[original[0] + ' ', ...original.slice(1)],
    [original[0], '{'], [original[0], original[1] + ' '], original.slice(1), [original[0], original[1], original[1]]]) {
    store.entries = entries; await assert.rejects(owner.snapshot());
    assert.equal(store.calls, calls);
  }
  store.entries = original;
  await assert.rejects(replica(store, bob).snapshot());
  await assert.rejects(owner.receive(Array(65).fill(CausalVersion.advance('1', bob, '0', [], null))));
  assert.equal(store.calls, calls);
  const parse = JSON.parse;
  try {
    JSON.parse = () => assert.fail('an inadmissible frame reached JSON.parse');
    for (const frame of ['[[[[[[]]]]]]', '"' + '1'.repeat(1048575) + '"',
      '[' + Array(32770).fill('null').join(',') + ']', '{}', '}', '"', '[', ']', '\\', '\x1f', '\x7f']) {
      store.entries = [original[0], frame];
      await assert.rejects(owner.snapshot(), /invalid causal replica state/);
      assert.equal(store.calls, calls);
    }
  } finally { JSON.parse = parse; store.entries = original; }
  // The pre-parser bounds must admit all semantic dimensions at their maxima.
  const clock = Array.from({ length: 64 }, (_, index) => ({ author: token(index + 2), counter: '1' }));
  const maximum = clock.map((dot, index) => ({ address: String(index + 1), dot,
    clock: structuredClone(clock), payload: row('x'.repeat(4096)) }));
  const large = new MemoryStore();
  assert.equal(await replica(large).receive(maximum), true);
  assert.equal((await replica(large).snapshot()).versions.length, 64);
  assert.equal(large.entries.length, 2);
  assert.ok(large.entries[1].length < 1048576);
});

test('unselected durability, invalid arguments and corrupt providers reject at their boundaries', async () => {
  for (const args of [[null, 'replica', alice], [new MemoryStore(), '', alice],
    [new MemoryStore(), 'replica', '0'.repeat(32)],
    [new MemoryStore(), 'replica', alice, { maxEntries: 1 }],
    [new MemoryStore(), 'replica', alice, { maxBytes: 0 }]])
    assert.throws(() => new CausalReplica(...args));
  const weak = new MemoryStore(); weak.supportsDurableFlush = false;
  assert.throws(() => replica(weak, alice, { requireDurableFlush: true }));
  assert.deepEqual((await replica(weak).snapshot()).versions, []);
  const store = new MemoryStore(), owner = replica(store);
  await assert.rejects(owner.read('0'));
  await assert.rejects(owner.write('invalid', '1', [], null));
  for (const snapshot of [[], [{ generation: '0', entries: ['unexpected'] }],
    [{ generation: '01', entries: [] }], [{ generation: '1', entries: [1] }]]) {
    store.read = async () => snapshot; await assert.rejects(owner.snapshot());
  }
  assert.equal(store.calls, 0);
  const uncertain = new MemoryStore(); uncertain.compareExchange = async () => undefined;
  await assert.rejects(replica(uncertain).write(token(1), '1', [], null), /indeterminate/i);
});


test('independent local journal vectors preserve canonical owner and history invariants', async () => {
  const fixtures = JSON.parse(readFileSync(new URL('../../../docs/protocol/causal-replica-v1.json', import.meta.url)));
  for (const fixture of fixtures.cases) {
    const store = new MemoryStore(); store.generation = BigInt(fixture.generation); store.entries = [...fixture.entries];
    const owner = replica(store, fixtures.author, fixture.options), before = [...store.entries];
    if (fixture.valid) {
      const result = await owner.snapshot();
      assert.equal(result.lastCounter, fixture.lastCounter, fixture.name);
      assert.deepEqual(wires(result.versions), fixture.versions.map(value => JSON.stringify(value)), fixture.name);
    } else await assert.rejects(owner.snapshot(), undefined, fixture.name);
    assert.equal(store.calls, 0); assert.deepEqual(store.entries, before);
  }
  const remote = counter => CausalVersion.advance('1', bob, String(counter - 1), [], null);
  const third = CausalVersion.advance('1', token(3), '0', [], null);
  for (const batch of [[remote(1), remote(2), third], [third, remote(2), remote(1)]]) {
    const store = new MemoryStore(), owner = replica(store);
    await owner.receive(batch);
    assert.deepEqual(wires(await owner.read('1')), wires([remote(2), third]));
    assert.deepEqual(wires((await replica(store).snapshot()).versions), wires([remote(2), third]));
  }
});
