import { causalQueryConformance } from './causal-query.mjs';
// Actual Rust u64 workspace, exercised by the existing Node/Bun/Deno runner.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { CausalSession } from '../../src/engine-causal-session.js';
import { CausalBatch } from '../../src/engine-causal-batch.js';
import { CausalReplica } from '../../src/engine-causal-replica.js';
import { CausalVersion } from '../../src/engine-causal-version.js';

const token = n => BigInt(n).toString(16).padStart(32, '0');
const author = token(1), other = token(2);
class Store {
  supportsDurableFlush = true;
  values = new Map(); calls = 0; failure = 0;
  async read(keys) { return keys.map(key => structuredClone(this.values.get(key) ?? { generation: '0', entries: [] })); }
  async compareExchange(keys, expected, updates) {
    this.calls++; const failure = this.failure; this.failure = 0;
    if (failure === 1) throw new Error('before');
    if (keys.some((key, i) => (this.values.get(key)?.generation ?? '0') !== expected[i].generation)) return false;
    keys.forEach((key, i) => this.values.set(key, { generation: (BigInt(expected[i].generation) + 1n).toString(), entries: [...updates[i]] }));
    if (failure === 2) throw new Error('after');
    return true;
  }
}
const batch = store => new CausalBatch(store, 'replica', 'batches', author);
const owner = store => new CausalReplica(store, 'replica', author);
const bytes = store => JSON.stringify([...store.values].sort());
const wires = values => values.map(CausalVersion.encode);
async function seeded() {
  const store = new Store();
  await batch(store).write(token(10), '0', [[1, 0, 2, 'old'], [3, 1, 1, null], [4, 0, 2, null]].map(([id, source, target, name]) =>
    ({ address: `${id}`, observed: [], payload: { source: `${source}`, target: `${target}`, name } })));
  return store;
}
export async function causalSessionConformance(binding) {
  await causalQueryConformance(binding, { Store, batch, owner, bytes });
  const open = (store, first = '32', last = '33', options = {}) => CausalSession.open(binding, batch(store), first, last, 'max-dot', options);
  {
    const store = await seeded(), initial = bytes(store), calls = store.calls, tx = await open(store);
    try {
      assert.equal(await tx.getName('1'), 'old');
      assert.equal(await tx.createLink('0', '2'), '32'); await tx.setName('32', '\ufeff雪😀\0');
      await tx.updateLink('1', '2', '0'); await tx.deleteLink('1');
      assert.ok(await tx.getLink('3')); assert.ok(await tx.getLink('4'));
      assert.equal(await tx.getByName('\ufeff雪😀\0'), '32'); assert.equal(bytes(store), initial); assert.equal(store.calls, calls);
      const result = await tx.commit(token(100)); assert.deepEqual(result.map(v => v.address), ['1', '32']);
      assert.equal(result[0].payload, null); assert.equal(store.calls, calls + 1);
      result[1].payload.name = 'caller mutation'; assert.equal((await tx.commit(token(100)))[1].payload.name, '\ufeff雪😀\0');
      await assert.rejects(tx.commit(token(200))); await assert.rejects(tx.createLink('0', '0'));
      assert.equal((await batch(store).snapshot()).lastCounter, '5');
    } finally { await tx.close(); }
  }
  {
    const maximum = '18446744073709551615', store = new Store();
    await batch(store).write(token(10), '0', [{ address: maximum, observed: [], payload: { source: '0', target: '0', name: null } }]);
    const tx = await open(store);
    try {
      assert.equal(await tx.createLink('0', '0'), '32'); await assert.rejects(tx.ensureCreated('31')); await assert.rejects(tx.ensureCreated('34'));
      assert.equal(await tx.ensureCreated('33'), '33'); const before = await tx.rows(); await assert.rejects(tx.createLink('0', '0')); assert.deepEqual(await tx.rows(), before);
      await tx.deleteLink(maximum); await tx.ensureCreated(maximum); await tx.setName(maximum, 'known identity');
      assert.equal((await tx.commit(token(100))).length, 3);
    } finally { await tx.close(); }
    const last = await open(new Store(), maximum, maximum);
    try { assert.equal(await last.createLink('0', '0'), maximum); await assert.rejects(last.createLink('0', '0')); assert.equal((await last.commit(token(100))).length, 1); }
    finally { await last.close(); }
  }
  {
    const store = await seeded(), tx = await open(store);
    try {
      await tx.setName('1', 'temporary'); await tx.setName('1', 'old'); await tx.deleteLink(await tx.createLink('0', '0'));
      const calls = store.calls; assert.deepEqual(await tx.commit(token(100)), []); assert.equal((await batch(store).snapshot()).lastCounter, '3');
      assert.equal(store.calls, calls + 1); assert.deepEqual(await tx.commit(token(100)), []); assert.equal(store.calls, calls + 1);
      const abandoned = await open(store, '40', '41'); await abandoned.deleteLink('1'); await abandoned.rollback(); await abandoned.rollback();
      await assert.rejects(abandoned.commit(token(200))); assert.equal(store.calls, calls + 1);
    } finally { await tx.close(); }
  }
  for (const failure of [1, 2]) {
    const store = await seeded(), tx = await open(store);
    try {
      await tx.setName('1', 'new'); store.failure = failure; await assert.rejects(tx.commit(token(100)), /indeterminate/i);
      assert.equal(tx.prepared.edits.length, 1); assert.equal(tx.prepared.edits[0].observed.length, 1);
      await assert.rejects(tx.setName('1', 'changed'));
      if (failure === 2) await owner(store).write(token(300), '9', [], null);
      const rejectedToken = assert.rejects(tx.commit(token(200)));
      const retry = tx.commit(token(100)); await rejectedToken;
      const result = await retry, before = bytes(store); assert.equal(result[0].dot.counter, '4');
      assert.deepEqual(wires(await tx.commit(token(100))), wires(result)); assert.equal(bytes(store), before);
    } finally { await tx.close(); }
  }
  {
    const store = await seeded(); await owner(store).receive([CausalVersion.advance('1', other, '0', [], null)]);
    const tx = await open(store);
    try {
      assert.equal(await tx.getLink('1'), null); await tx.ensureCreated('1'); await tx.setName('1', 'recreated');
      assert.equal((await tx.prepare(token(100))).edits[0].observed.length, 2); assert.equal((await tx.commit(token(100))).length, 1);
      assert.equal((await batch(store).snapshot()).versions.filter(v => v.address === '1').length, 1);
    } finally { await tx.close(); }
    for (const stateStructure of ['flat', 'shared', 'sorted-runs']) for (const constraintValidation of ['scan', 'indexed']) {
      const store = new Store(), tx = await open(store, '32', '33', { stateStructure, constraintValidation, constraints: ['unique-pairs'] });
      try { await tx.createLink('0', '0'); await tx.createLink('0', '0'); await assert.rejects(tx.commit(token(100))); assert.equal(store.values.size, 0); assert.equal(store.calls, 0); }
      finally { await tx.close(); }
    }
  }
  {
    const store = await seeded(), stale = await open(store); await stale.deleteLink('1'); await owner(store).write(token(300), '9', [], null);
    const before = bytes(store), calls = store.calls;
    try { await assert.rejects(stale.commit(token(100))); assert.equal(bytes(store), before); assert.equal(store.calls, calls); }
    finally { await stale.close(); }
    for (const [first, last] of [['0', '1'], ['33', '32'], ['1', '1']]) await assert.rejects(open(store, first, last));
    const empty = new Store(), full = await open(empty, '1', '65');
    try { for (let i = 0; i < 65; i++) await full.createLink('0', '0'); await assert.rejects(full.commit(token(100))); assert.equal(empty.values.size, 0); assert.equal(empty.calls, 0); }
    finally { await full.close(); }
  }
  {
    const store = await seeded(), tx = await open(store); await tx.setName('1', '');
    const request = await tx.prepare(token(100)); request.edits[0].payload.name = 'caller mutation'; request.edits[0].observed.length = 0;
    const rows = await tx.rows(); rows[0].name = null;
    assert.equal((await tx.commit(token(100)))[0].payload.name, ''); assert.equal((await tx.rows())[0].name, '');
    const retained = await tx.rows(), calls = store.calls; await tx.close(); await tx.close();
    assert.equal(retained[0].name, ''); await assert.rejects(tx.rows()); await assert.rejects(tx.commit(token(100))); assert.equal(store.calls, calls);
  }
  for (const invalid of ['', 'invalid', '0'.repeat(32), 'F'.repeat(32)]) {
    const store = await seeded(), before = bytes(store), calls = store.calls, tx = await open(store);
    try {
      await tx.setName('1', 'new'); await assert.rejects(tx.prepare(invalid)); assert.equal(tx.prepared, null); assert.equal(bytes(store), before); assert.equal(store.calls, calls);
      const rejected = assert.rejects(tx.commit(invalid));
      const valid = tx.commit(token(100)); await rejected; await valid; assert.equal(store.calls, calls + 1);
    }
    finally { await tx.close(); }
  }
  {
    const store = await seeded(), before = bytes(store), calls = store.calls, tx = await open(store);
    try { await tx.setName('1', 'a'.repeat(4097)); await assert.rejects(tx.prepare(token(100))); assert.equal(tx.prepared, null); assert.equal(bytes(store), before); assert.equal(store.calls, calls); }
    finally { await tx.close(); }
  }
  const vectors = JSON.parse(readFileSync(new URL('../../../../docs/protocol/causal-session-v1.json', import.meta.url), 'utf8'));
  assert.equal(vectors.cases.length, 343);
  for (const item of vectors.cases) for (const failure of [0, 1, 2]) {
    const store = await seeded(), before = bytes(store), calls = store.calls, tx = await open(store);
    try {
      for (const [position, operation] of item.operations.entries()) {
        const original = await tx.rows(); let error;
        try {
          switch (operation) {
            case 'create': await tx.createLink('0', '2'); break;
            case 'rename': await tx.setName('1', 'new\ufeff\0🙂'); break;
            case 'update': await tx.updateLink('1', '2', '0'); break;
            case 'delete': await tx.deleteLink('1'); break;
            case 'restore': await tx.ensureCreated('1'); await tx.updateLink('1', '0', '2'); await tx.setName('1', 'old'); break;
            case 'cancel-create': if (await tx.getLink('32')) await tx.deleteLink('32'); break;
            case 'noop': break;
            default: throw new Error('unknown reference operation');
          }
        } catch (cause) { error = cause; }
        assert.equal(error !== undefined, item.errors[position]); if (error) assert.deepEqual(await tx.rows(), original);
        assert.equal(bytes(store), before); assert.equal(store.calls, calls);
      }
      assert.deepEqual((await tx.rows()).map(row => [row.index, row.source, row.target, row.name]), item.rows);
      store.failure = failure;
      if (failure === 0) await tx.commit(token(100)); else await assert.rejects(tx.commit(token(100)));
      const written = await tx.commit(token(100)); assert.deepEqual(written.map(v => v.address), item.changed);
      assert.equal((await batch(store).snapshot()).lastCounter, `${3 + written.length}`);
      assert.deepEqual(await tx.commit(token(100)), written); assert.equal(store.calls, calls + (failure === 1 ? 2 : 1));
    } finally { await tx.close(); }
  }
  console.log('PASS actual causal sessions: nine contract groups, 1029 finite schedules and 686 exact before/after-reply retries');
}
