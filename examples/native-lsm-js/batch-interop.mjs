// One local atomic batch, two-key receipts and exact retries in real directories.
import assert from 'node:assert/strict';
import { readdirSync } from 'node:fs';
import { FileLsmJournalStore } from '../../js/client/src/engine-file-lsm-store.js';
import { CausalBatch } from '../../js/client/src/engine-causal-batch.js';
import { CausalVersion } from '../../js/client/src/engine-causal-version.js';

const [action, directory, mirrors] = process.argv.slice(2);
assert.ok(['write', 'read', 'mutate', 'read-advanced', 'blocked', 'cut-before', 'cut-after',
  'retry-before', 'retry-after', 'following', 'read-following'].includes(action));
assert.ok(['plain', 'mirrored'].includes(mirrors));
const author = '00000000000000000000000000000001';
const token = n => n.toString(16).padStart(32, '0');
const row = name => ({ source: '2', target: '3', name });
const first = [CausalVersion.advance('1', author, '0', [], row('initial\ufeff雪😀\0')),
  CausalVersion.advance('2', author, '1', [], null)];
const second = [CausalVersion.advance('1', author, '2', [first[0]], null),
  CausalVersion.advance('2', author, '3', [first[1]], row('selected\ufeff雪😀\0'))];
const following = CausalVersion.advance('3', author, '4', [], row('following'));
const initialEdits = () => first.map(value => ({ address: value.address, observed: [], payload: value.payload }));
const nextEdits = () => second.map((value, i) => ({ address: value.address, observed: [first[i]], payload: value.payload }));
const wire = values => values.map(value => CausalVersion.encode(value));
const mirrored = mirrors === 'mirrored';
const options = { maxKeys: 32, maxBytes: 65536, bufferKeys: 16, bufferBytes: 32768, maxRuns: 8,
  maxPages: 128, mirroredRoot: mirrored, mirroredBanks: mirrored, mirroredTables: mirrored };
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
      assert.deepEqual(wire(await batches.write(token(100), '0', initialEdits())), wire(first));
      physical.flush(); physical.compact();
    }
    await check(['read-advanced', 'retry-after', 'following'].includes(action) ? 1 : action === 'read-following' ? 2 : 0);
    if (['mutate', 'cut-before', 'cut-after', 'retry-before', 'retry-after'].includes(action)) {
      assert.deepEqual(wire(await batches.write(token(200), '1', nextEdits())), wire(second));
      await check(1);
    }
    if (action === 'following') {
      assert.deepEqual(wire(await batches.write(token(300), '2', [{ address: '3', observed: [], payload: following.payload }])), wire([following]));
      assert.deepEqual(await batches.write(token(400), '3', []), []);
      await check(2);
    }
    if (['mutate', 'retry-before', 'following'].includes(action)) { physical.flush(); physical.compact(); }
  } finally { physical.close(); }
}
console.log(`PASS JS causal batch native LSM ${action} ${mirrors}`);
