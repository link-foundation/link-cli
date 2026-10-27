// Native complete-store files, owned versions, tombstones and following writes.
import assert from 'node:assert/strict';
import { readdirSync } from 'node:fs';
import { FileLsmJournalStore } from '../../js/client/src/engine-file-lsm-store.js';
const [action, directory, mirrors] = process.argv.slice(2);
assert.ok(['write', 'read', 'mutate', 'read-advanced'].includes(action));
assert.ok(['plain', 'mirrored'].includes(mirrors));
if (action === 'write') assert.deepEqual(readdirSync(directory), []);
const mirrored = mirrors === 'mirrored', keys = ['a', 'b', 'c', 'd'];
const store = new FileLsmJournalStore(directory, {
  maxKeys: 32, maxBytes: 65536, bufferKeys: 16, bufferBytes: 32768, maxRuns: 8,
  maxPages: 128, mirroredRoot: mirrored, mirroredBanks: mirrored, mirroredTables: mirrored,
});
function check(advanced) {
  const generations = advanced ? ['5', '1', '6', '3'] : ['2', '1', '4', '3'];
  const entries = [advanced ? ['handoff\0雪😀'] : ['selected\ufeff雪😀'], [], advanced ? ['revived'] : [], ['buffer']];
  assert.deepEqual(store.read(keys), keys.map((_, index) => ({ generation: generations[index], entries: entries[index] })));
  assert.deepEqual(store.scanNamespace().map(row => row.key), advanced ? keys : ['a', 'b', 'd']);
  assert.equal(store.compareExchange(['a'], [{ generation: '0' }], [['lost']]), false);
}
try {
  if (action === 'write') {
    assert.equal(store.compareExchange(['a', 'b', 'c'], store.read(['a', 'b', 'c']), [['old'], [], ['removed\0雪😀']]), true); store.flush();
    assert.equal(store.compareExchange(['a', 'b'], store.read(['a', 'b']), [['selected\ufeff雪😀'], null]), true); store.flush();
    assert.equal(store.compareExchange(['d'], store.read(['d']), [['buffer']]), true); assert.equal(store.compareDelete('c', '1'), true);
  }
  check(action === 'read-advanced');
  if (action === 'mutate') {
    assert.equal(store.compareExchange(['a', 'd'], store.read(['a', 'd']), [['handoff\0雪😀'], null]), true); store.flush(); store.compact();
    assert.equal(store.compareExchange(['c'], store.read(['c']), [['revived']]), true); check(true);
  }
  console.log(`PASS JavaScript native LSM ${action} ${mirrors}`);
} finally { store.close(); }
