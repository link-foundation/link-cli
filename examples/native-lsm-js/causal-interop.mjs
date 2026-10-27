// Finite local ownership/retry scenario shared with independent Rust/C# ports.
import assert from 'node:assert/strict';
import { readdirSync } from 'node:fs';
import { FileLsmJournalStore } from '../../js/client/src/engine-file-lsm-store.js';
import { CausalReplica } from '../../js/client/src/engine-causal-replica.js';
import { CausalVersion } from '../../js/client/src/engine-causal-version.js';

const [action, directory, mirrors] = process.argv.slice(2);
assert.ok(['write', 'read', 'mutate', 'read-advanced', 'blocked', 'cut-before', 'cut-after',
  'retry-before', 'retry-after', 'following', 'read-following'].includes(action));
assert.ok(['plain', 'mirrored'].includes(mirrors));
const alice = '00000000000000000000000000000001', bob = '00000000000000000000000000000002';
const token = n => n.toString(16).padStart(32, '0');
const row = name => ({ source: '2', target: '3', name });
const initial = CausalVersion.advance('1', alice, '0', [], row('initial\ufeff雪😀\0'));
const deletion = CausalVersion.advance('1', bob, '0', [initial], null);
const selected = CausalVersion.advance('1', alice, '1', [deletion], row('selected\ufeff雪😀\0'));
const other = CausalVersion.advance('2', alice, '2', [], null);
const advanced = CausalVersion.advance('1', alice, '3', [selected], null);
const following = CausalVersion.advance('3', alice, '4', [], row('following'));
const wire = versions => versions.map(value => CausalVersion.encode(value));
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
  const owner = new CausalReplica(store, 'replica', alice, { requireDurableFlush: true });
  const check = async phase => {
    const versions = [phase ? advanced : selected, other];
    if (phase === 2) versions.push(following);
    const snapshot = await owner.snapshot();
    assert.equal(snapshot.lastCounter, phase === 2 ? '5' : phase ? '4' : '3');
    assert.deepEqual(wire(snapshot.versions), wire(versions));
    assert.deepEqual(wire([await owner.write(token(1), '1', [], row('initial\ufeff雪😀\0'))]), wire([initial]));
    assert.equal(await owner.receive([initial, deletion]), false);
    snapshot.versions[0].clock[0].counter = '99';
    assert.deepEqual(wire(await owner.read('1')), wire([phase ? advanced : selected]));
  };
  try {
    if (action === 'write') {
      await owner.write(token(1), '1', [], initial.payload); await owner.receive([deletion]);
      await owner.write(token(2), '1', [deletion], selected.payload); await owner.write(token(3), '2', [], null);
      physical.flush(); physical.compact();
    }
    await check(['read-advanced', 'retry-after', 'following'].includes(action) ? 1 : action === 'read-following' ? 2 : 0);
    if (['mutate', 'cut-before', 'cut-after', 'retry-before', 'retry-after'].includes(action)) {
      assert.deepEqual(wire([await owner.write(token(4), '1', [selected], null)]), wire([advanced]));
      await check(1);
    }
    if (action === 'following') {
      await owner.write(token(5), '3', [], following.payload);
      assert.deepEqual(wire([await owner.write(token(4), '1', [selected], null)]), wire([advanced]));
      await check(2);
    }
    if (['mutate', 'retry-before', 'following'].includes(action)) { physical.flush(); physical.compact(); }
  } finally { physical.close(); }
}
console.log(`PASS JS causal native LSM ${action} ${mirrors}`);
