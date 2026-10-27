// The same manifest is read and advanced by independent language implementations.
import assert from 'node:assert/strict';
import { mkdirSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { FileLsmJournalStore } from '../../js/client/src/engine-file-lsm-store.js';
import { PartitionedJournalStore } from '../../js/client/src/partitioned.js';
const [action, directory, mirrors] = process.argv.slice(2);
assert.ok(['write', 'read', 'mutate', 'read-advanced'].includes(action));
assert.ok(['plain', 'mirrored'].includes(mirrors));
if (action === 'write') assert.deepEqual(readdirSync(directory), []);
const mirrored = mirrors === 'mirrored';
const options = { maxKeys: 256, maxBytes: 1048576, bufferKeys: 64, bufferBytes: 262144, maxPages: 128, mirroredRoot: mirrored, mirroredBanks: mirrored, mirroredTables: mirrored };
const physical = [];
try {
  for (const name of ['catalog', 'east']) {
    if (action === 'write') mkdirSync(join(directory, name));
    physical.push(new FileLsmJournalStore(join(directory, name), options));
  }
  const store = new PartitionedJournalStore(physical[0], { east: physical[1] }, () => 'east', { cacheBytes: 256, chunks: { maxRecords: action === 'mutate' ? 3 : 2, maxBytes: 64, maxChunks: 32 } });
  const keys = ['a', 'b']; const seed = ['aa', 'bb', '\ufeff雪', '😀\0', 'tail'];
  if (action === 'write') assert.equal(await store.compareExchange(keys, await store.read(keys), [seed, []]), true);
  const check = async (advanced) => assert.deepEqual(await store.read(keys), [{ generation: advanced ? '2' : '1', entries: advanced ? [...seed, 'next'] : seed }, { generation: '1', entries: [] }]);
  await check(action === 'read-advanced');
  if (action === 'mutate') {
    const before = await store.read(keys);
    assert.equal(await store.compareExchange(keys, before, [[...seed, 'next'], null]), true);
    assert.deepEqual(before[0].entries, seed);
    assert.equal(await store.compareExchange(keys, before, [['stale'], null]), false);
    await check(true);
  }
  if (action === 'write' || action === 'mutate') for (const files of physical) { files.flush(); files.compact(); }
  console.log(`PASS JavaScript native chunks ${action} ${mirrors}`);
} finally { for (const files of physical.reverse()) files.close(); }
