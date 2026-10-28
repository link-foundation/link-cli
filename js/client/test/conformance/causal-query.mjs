// Standard public LinksClient over the compiled, checked u64 Rust workspace.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { ANY } from '../../src/index.js';
import { encodeReferenceLiteral } from '../../src/lino.js';
const token = n => BigInt(n).toString(16).padStart(32, '0');
const maximum = 18446744073709551615n;
export async function causalQueryConformance(binding, { Store, batch, owner, bytes }) {
  const { CausalSession } = await import('../../src/engine-causal-session.js');
  const open = store => CausalSession.open(binding, batch(store), '1', '16', 'reject');
  const vectors = JSON.parse(readFileSync(new URL('../../../../docs/protocol/causal-query-v1.json', import.meta.url), 'utf8'));
  assert.equal(vectors.version, 1); assert.equal(vectors.cases.length, 16);
  for (const vector of vectors.cases) {
    const store = new Store();
    if (vector.seed.length) await batch(store).write(token(10), '0', vector.seed.map(row => ({
      address: row.index, observed: [], payload: { source: row.source, target: row.target, name: row.name },
    })));
    const before = bytes(store), calls = store.calls;
    const tx = await CausalSession.open(binding, batch(store), vector.first, vector.last, 'reject');
    const client = tx.client();
    try {
      for (const query of vector.queries) {
        const rows = await tx.rows();
        if (query.error) {
          await assert.rejects(client.query(query.text), vector.name);
          assert.deepEqual(await tx.rows(), rows, vector.name);
        } else await client.query(query.text);
      }
      assert.deepEqual(await tx.rows(), vector.rows, vector.name);
      const next = await tx.createLink('0', '0'); assert.equal(next, vector.next, vector.name);
      await tx.deleteLink(next);
      assert.equal((await tx.prepare(token(100))).edits.length, vector.edits, vector.name);
      assert.equal(bytes(store), before, vector.name); assert.equal(store.calls, calls, vector.name);
    } finally { await client.close(); }
  }
  {
    const store = new Store();
    await batch(store).write(token(10), '0', [{ address: `${maximum}`, observed: [],
      payload: { source: `${maximum}`, target: `${maximum}`, name: null } }]);
    const initial = bytes(store), calls = store.calls, tx = await open(store), client = tx.client();
    try {
      client.require('atomicTransactions', 'rollback');
      assert.equal(client.capabilities.maxAddress, maximum);
      assert.equal(client.capabilities.losslessQueries, true);
      assert.deepEqual(await client.create(maximum, 1n), { index: 1n, source: maximum, target: 1n });
      for (const name of ['4294967312', `${maximum}`, '*', '$x', '', "'\"雪\0\n"]) {
        await client.setName(1n, name);
        assert.equal(await client.getName(1n), name); assert.equal(await client.getByName(name), 1n);
        const ref = encodeReferenceLiteral(name);
        await client.query(`# ignored unmatched ) and quote "\n((${ref}: $s $t)) ((${ref}: $t $s)) # ignored (`);
        assert.equal(await client.count([maximum]), 1n);
        assert.deepEqual((await client.each([maximum]))[0], { index: maximum, source: maximum, target: maximum });
      }
      assert.equal(await client.count([ANY, maximum, ANY]), 2n);
      await client.update(1n, 1n, maximum);
      const rows = await tx.rows();
      for (const query of ['() ((18446744073709551616: 0 0))', '() ((-1: 0 0))', '() ((100: 0 0))']) {
        await assert.rejects(client.query(query)); assert.deepEqual(await tx.rows(), rows);
      }
      await assert.rejects(client.batch([{ op: 'create', source: 0n, target: 0n }, { op: 'query', text: '(' }]));
      assert.deepEqual(await tx.rows(), rows);
      assert.equal((await client.create(0n, 0n)).index, 2n); await client.delete(2n);
      assert.equal(bytes(store), initial); assert.equal(store.calls, calls);
      const prepared = await tx.prepare(token(999)); assert.equal(prepared.edits.length, 1);
      await assert.rejects(client.count(), error => error.outcome === 'not-sent');
      store.failure = 2; await assert.rejects(tx.commit(token(999)), /indeterminate/i);
      await owner(store).write(token(300), '9', [], null);
      const result = await tx.commit(token(999)), after = bytes(store);
      assert.equal(result[0].address, '1'); assert.equal(result[0].dot.counter, '2');
      assert.equal(result[0].payload.source, '1'); assert.equal(result[0].payload.target, `${maximum}`);
      assert.equal(result[0].payload.name, "'\"雪\0\n");
      assert.deepEqual(await tx.commit(token(999)), result); assert.equal(bytes(store), after);
      assert.equal(store.calls, calls + 2); // Lost reply CAS and a later owner write.
      assert.deepEqual(tx.prepared, prepared);
      await client.close(); await assert.rejects(client.each(), error => error.outcome === 'not-sent');
    } finally { await tx.close(); }
  }
  {
    const store = new Store(), tx = await open(store), client = tx.client();
    try {
      const result = await client.batch([{ op: 'query', text: '(() ((1 1)))' },
        { op: 'query', text: '() ((2: 2 2))' }]);
      assert.equal(result.completed, true);
      assert.deepEqual(await client.each(), [{ index: 1n, source: 1n, target: 1n }, { index: 2n, source: 2n, target: 2n }]);
      assert.equal(store.calls, 0); assert.equal((await tx.commit(token(100))).length, 2);
    } finally { await client.close(); }
  }
  {
    const store = new Store(), tx = await open(store), client = tx.client();
    try {
      for (let id = 1n; id <= 12n; id++) assert.equal((await client.create(1n, id)).index, id);
      await client.setName(1n, 'n'.repeat(100_000));
      const before = await tx.rows();
      await assert.rejects(client.exportLinks(), /reply byte limit/);
      await assert.rejects(client.query('(($i: $s $t)) (($i: $t $s))'), /reply byte limit/);
      assert.deepEqual(await tx.rows(), before);
      await client.removeName(1n);
      await client.query('(($i: $s $t)) (($i: $t $s))');
      assert.equal((await client.each([2n]))[0].source, 2n); assert.equal(store.calls, 0);
    } finally { await client.close(); }
  }
  console.log('PASS actual standard causal client: 16 shared query vectors and three bounded groups, full u64 operations, six lossless name queries, reply budgets and exact retry after later publication');
}
