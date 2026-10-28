// Permanent grants survive handoff, compaction, process exits and lost replies.
import assert from 'node:assert/strict';
import { readdirSync } from 'node:fs';
import { FileLsmJournalStore } from '../../js/client/src/engine-file-lsm-store.js';
import { AddressAllocator } from '../../js/client/src/engine-address-allocator.js';

const [action, directory, mirrors] = process.argv.slice(2);
assert.ok(['write','read','mutate','read-advanced','blocked','cut-before','cut-after',
  'retry-before','retry-after','following','read-following'].includes(action));
assert.ok(['plain', 'mirrored'].includes(mirrors));
const A = '00000000000000000000000000000001', B = '00000000000000000000000000000002';
const token = n => n.toString(16).padStart(32, '0'), mirrored = mirrors === 'mirrored';
const options = { maxKeys: 32, maxBytes: 65536, bufferKeys: 16, bufferBytes: 32768, maxRuns: 8,
  maxPages: 128, mirroredRoot: mirrored, mirroredBanks: mirrored, mirroredTables: mirrored };
if (action === 'blocked') assert.throws(() => new FileLsmJournalStore(directory, options));
else {
  if (action === 'write') assert.deepEqual(readdirSync(directory), []);
  const physical = new FileLsmJournalStore(directory, options);
  const store = {
    get supportsDurableFlush() { return physical.supportsDurableFlush; },
    read: keys => physical.read(keys),
    compareExchange: (...args) => {
      if (action === 'cut-before') process.exit(73);
      const result = physical.compareExchange(...args);
      if (action === 'cut-after' && result) process.exit(73);
      return result;
    },
  };
  const owner = new AddressAllocator(store, 'addresses', '1', '20', { requireDurableFlush: true });
  const grants = [
    { author: A, token: token(100), count: '2', first: '1', last: '2' },
    { author: B, token: token(200), count: '3', first: '3', last: '5' },
    { author: A, token: token(300), count: '2', first: '6', last: '7' },
  ];
  const reserve = async index => {
    const grant = grants[index];
    assert.deepEqual(await owner.reserve(grant.author, grant.token, grant.count), grant);
  };
  const check = async phase => {
    assert.deepEqual((await owner.snapshot()).reservations, grants.slice(0, phase + 1));
    assert.equal((await owner.snapshot()).next, ['3','6','8'][phase]);
    for (let i = 0; i <= phase; i++) await reserve(i);
    await assert.rejects(owner.reserve(B, token(100), '2'));
    await assert.rejects(owner.reserve(A, token(100), '1'));
    await assert.rejects(new AddressAllocator(store, 'addresses', '2', '20').snapshot());
  };
  try {
    if (action === 'write') { await reserve(0); physical.flush(); physical.compact(); }
    await check(['read-advanced','retry-after','following'].includes(action) ? 1 : action === 'read-following' ? 2 : 0);
    if (['mutate','cut-before','cut-after','retry-before','retry-after'].includes(action)) { await reserve(1); await check(1); }
    if (action === 'following') { await reserve(2); await check(2); }
    if (['mutate','retry-before','following'].includes(action)) { physical.flush(); physical.compact(); }
  } finally { physical.close(); }
}
console.log(`PASS JS address allocation native LSM ${action} ${mirrors}`);
