// Ordinary sessions publish one local batch; retained requests resolve process-exit replies.
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { CausalSession } from '../../js/client/src/engine-causal-session.js';
import { FileLsmJournalStore } from '../../js/client/src/engine-file-lsm-store.js';
import { CausalBatch } from '../../js/client/src/engine-causal-batch.js';
import { CausalVersion } from '../../js/client/src/engine-causal-version.js';
import { encodeReferenceLiteral } from '../../js/client/src/lino.js';

const packagePath = resolve(process.env.CLINK_WASM ?? 'js/pkg');
const binding = await import(pathToFileURL(join(packagePath, 'clink_wasm.js')).href);
binding.initSync({ module: readFileSync(join(packagePath, 'clink_wasm_bg.wasm')) });

const [action, directory, mirrors] = process.argv.slice(2);
assert.ok(['write', 'read', 'mutate', 'read-advanced', 'blocked', 'cut-before', 'cut-after',
  'retry-before', 'retry-after', 'following', 'read-following'].includes(action));
assert.ok(['plain', 'mirrored'].includes(mirrors));
const author = '00000000000000000000000000000001';
const token = n => n.toString(16).padStart(32, '0');
const row = name => ({ source: '2', target: '3', name });
const first = [CausalVersion.advance('1', author, '0', [], row('initial\ufeff雪😀\0')),
  CausalVersion.advance('2', author, '1', [], row('second'))];
const second = [CausalVersion.advance('1', author, '2', [first[0]], null),
  CausalVersion.advance('2', author, '3', [first[1]], row('selected\ufeff雪😀\0'))];
const following = CausalVersion.advance('3', author, '4', [], row('following'));
const initialEdits = () => first.map(value => ({ address: value.address, observed: [], payload: value.payload }));
const nextEdits = () => second.map((value, i) => ({ address: value.address, observed: [first[i]], payload: value.payload }));
const wire = values => values.map(value => CausalVersion.encode(value));
const mirrored = mirrors === 'mirrored';
const options = { maxKeys: 32, maxBytes: 65536, bufferKeys: 16, bufferBytes: 32768, maxRuns: 8,
  maxPages: 128, mirroredRoot: mirrored, mirroredBanks: mirrored, mirroredTables: mirrored };
// Include ownership markers and every physical file while this process owns it.
const physicalBytes = () => readdirSync(directory).sort().map(name => [name, readFileSync(join(directory, name))]);
if (action === 'blocked') {
  assert.throws(() => new FileLsmJournalStore(directory, options));
} else {
  if (action === 'write') assert.deepEqual(readdirSync(directory), []);
  const physical = new FileLsmJournalStore(directory, options);
  const store = {
    get supportsDurableFlush() { return physical.supportsDurableFlush; },
    read: keys => physical.read(keys),
    compareExchange: (...args) => {
      if (action === 'cut-before') process.exit(73);
      const result = physical.compareExchange(...args);
      if (action === 'cut-after' && result === true) process.exit(73);
      return result;
    },
  };
  const batches = new CausalBatch(store, 'replica', 'batches', author, { requireDurableFlush: true });
  const session = (first, last) => CausalSession.open(binding, batches, first, last, 'reject');
  const create = async (client, id, name) => {
    assert.deepEqual(await client.create(2n, 3n), { index: id, source: 2n, target: 3n });
    await client.setName(id, name);
  };
  const change = (before, after) => {
    const named = index => ({ index, source: 'second', target: '3' });
    return { before: named(before), after: after === null ? null : named(after) };
  };
  const prepare = async (tx, client, value, generation, edits) => {
    const expected = { token: token(value), generation, edits };
    const request = await tx.prepare(token(value));
    assert.deepEqual(request, expected);
    if (request.edits.length) request.edits[0].address = '99';
    assert.deepEqual(tx.prepared, expected);
    await assert.rejects(client.count());
    await assert.rejects(client.query('(($i: $s $t)) (($i: $s $t))'));
    assert.deepEqual(await tx.prepare(token(value)), expected);
  };
  const check = async phase => {
    const expected = phase ? [...second] : [...first];
    if (phase === 2) expected.push(following);
    const snapshot = await batches.snapshot();
    // Empty receipts do not change the owner key in the physical LSM provider.
    assert.equal(snapshot.generation, phase === 2 ? '3' : phase ? '2' : '1');
    assert.equal(snapshot.lastCounter, phase === 2 ? '5' : phase ? '4' : '2');
    assert.deepEqual(wire(snapshot.versions), wire(expected));
    assert.deepEqual(wire(await batches.write(token(100), '0', initialEdits())), wire(first));
    if (phase) assert.deepEqual(wire(await batches.write(token(200), '1', nextEdits())), wire(second));
    snapshot.versions[0].clock[0].counter = '99';
    assert.deepEqual(wire((await batches.snapshot()).versions), wire(expected));
  };
  try {
    if (action === 'write') {
      const tx = await session('1', '2');
      try {
        const client = tx.client(), bytes = physicalBytes();
        await create(client, 1n, first[0].payload.name); await create(client, 2n, 'second');
        assert.deepEqual(await client.query('(($i: $s $t)) (($i: $s $t))'),
          [change(first[0].payload.name, first[0].payload.name), change('second', 'second')]);
        const literal = encodeReferenceLiteral(first[0].payload.name);
        assert.deepEqual(await client.query(`((${literal}: $s $t)) ((${literal}: $s $t))`),
          [change(first[0].payload.name, first[0].payload.name)]);
        assert.equal((await tx.rows()).length, 2);
        assert.deepEqual((await batches.snapshot()).versions, []);
        await prepare(tx, client, 100, '0', initialEdits());
        assert.deepEqual(physicalBytes(), bytes);
        assert.deepEqual(wire(await tx.commit(token(100))), wire(first));
      } finally { await tx.close(); }
      physical.flush(); physical.compact();
    }
    await check(['read-advanced', 'retry-after', 'following'].includes(action) ? 1 : action === 'read-following' ? 2 : 0);
    if (['mutate', 'cut-before', 'cut-after', 'retry-before', 'retry-after'].includes(action)) {
      if (action === 'retry-after') {
        // Exact pre-exit request: never rebuild a session on a newer base.
        assert.deepEqual(wire(await batches.write(token(200), '1', nextEdits())), wire(second));
      } else {
        const stale = await session('8', '9'), tx = await session('3', '3');
        try {
          const client = tx.client(), bytesBefore = physicalBytes();
          await stale.client().setName(1n, 'stale');
          const observed = await client.query('(($i: $s $t)) (($i: $s $t))');
          const originalReply = structuredClone(observed);
          assert.deepEqual(await client.query('((1: $s $t)) ()'), [change('1', null)]);
          await client.setName(2n, second[1].payload.name);
          assert.equal(await tx.getLink('1'), null);
          assert.equal(await tx.getName('2'), second[1].payload.name);
          assert.deepEqual(observed, originalReply);
          await prepare(tx, client, 200, '1', nextEdits());
          assert.deepEqual(physicalBytes(), bytesBefore);
          assert.deepEqual(wire(await tx.commit(token(200))), wire(second));
          const before = await store.read(['replica', 'batches']);
          const bytes = physicalBytes();
          await assert.rejects(stale.commit(token(500)), /conflict/i);
          assert.deepEqual(await store.read(['replica', 'batches']), before);
          assert.deepEqual(physicalBytes(), bytes);
        } finally { await tx.close(); await stale.close(); }
      }
      await check(1);
    }
    if (action === 'following') {
      const tx = await session('3', '3');
      try {
        const client = tx.client(), bytes = physicalBytes();
        await create(client, 3n, 'following');
        await prepare(tx, client, 300, '2', [{ address: '3', observed: [], payload: following.payload }]);
        assert.deepEqual(physicalBytes(), bytes);
        assert.deepEqual(wire(await tx.commit(token(300))), wire([following]));
      }
      finally { await tx.close(); }
      const empty = await session('4', '4');
      try { assert.deepEqual(await empty.commit(token(400)), []); } finally { await empty.close(); }
      await check(2);
    }
    if (['mutate', 'retry-before', 'following'].includes(action)) { physical.flush(); physical.compact(); }
  } finally { physical.close(); }
}
console.log(`PASS JS causal session native LSM ${action} ${mirrors}`);
