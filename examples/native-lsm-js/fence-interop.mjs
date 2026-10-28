// SQL-free physical author ownership across handoffs and process exits.
import assert from 'node:assert/strict';
import { readdirSync } from 'node:fs';
import { FileLsmJournalStore } from '../../js/client/src/engine-file-lsm-store.js';
import { AuthorFence } from '../../js/client/src/engine-author-fence.js';
const [action, directory, mirrors] = process.argv.slice(2);
assert.ok(['write','read','mutate','read-advanced','blocked','cut-before','cut-after',
  'retry-before','retry-after','read-claimed','following','read-following'].includes(action));
assert.ok(['plain','mirrored'].includes(mirrors));
const A = '00000000000000000000000000000001', mirrored = mirrors === 'mirrored';
const claim = epoch => ({author:A, token:(epoch*10).toString(16).padStart(32,'0'), epoch:epoch.toString()});
const keys = ['a','b'], allKeys = ['fence', ...keys];
const rows = phase => phase === 1 ? [['seed-a\0雪'],['seed-b\uFEFF😀']] :
  phase === 2 ? [['takeover-a'],['takeover-b']] : [['following-a'],['following-b']];
const options = {maxKeys:32,maxBytes:65536,bufferKeys:16,bufferBytes:32768,maxRuns:8,
  maxPages:128,mirroredRoot:mirrored,mirroredBanks:mirrored,mirroredTables:mirrored};
if (action === 'blocked') { assert.throws(() => new FileLsmJournalStore(directory, options)); process.exit(0); }
if (action === 'write') assert.deepEqual(readdirSync(directory), []);
const physical = new FileLsmJournalStore(directory, options);
const fence = new AuthorFence(physical, 'fence', A, {requireDurableFlush:true});
async function check(phase, dataPhase) {
  const state = await fence.snapshot();
  assert.equal(state.epoch, phase.toString()); assert.deepEqual(state.claims, Array.from({length:phase}, (_,i) => claim(i+1)));
  assert.deepEqual(await fence.claim((phase-1).toString(), claim(phase).token), claim(phase));
  const before = physical.read(allKeys);
  for (let oldEpoch = 1; oldEpoch < phase; oldEpoch++) {
    await assert.rejects(fence.claim((oldEpoch-1).toString(), claim(oldEpoch).token), /fenced/i); const stale = fence.bind(claim(oldEpoch));
    await assert.rejects(stale.read(keys), /fenced/i);
    await assert.rejects(stale.compareExchange(keys, before.slice(1), rows(1)), /fenced/i);
  }
  const guarded = fence.bind(claim(phase)), values = await guarded.read(keys);
  assert.deepEqual(values.map(v => v.entries), rows(dataPhase));
  assert.equal(await guarded.compareExchange(keys, values, [null,null]), true);
  assert.deepEqual(physical.read(allKeys), before);
}
try {
  if (action === 'write') {
    assert.deepEqual(await fence.claim('0', claim(1).token), claim(1)); const guarded = fence.bind(claim(1));
    assert.equal(await guarded.compareExchange(keys, keys.map(() => ({generation:'0',entries:[]})), rows(1)), true);
    physical.flush(); physical.compact();
  }
  const [phase, dataPhase] = action === 'read-advanced' ? [2,2] :
    ['retry-after','read-claimed','following'].includes(action) ? [2,1] : action === 'read-following' ? [3,3] : [1,1];
  await check(phase, dataPhase);
  if (['cut-before','cut-after'].includes(action)) {
    const cut = {
      get supportsDurableFlush() { return physical.supportsDurableFlush; }, read: keys => physical.read(keys),
      compareExchange: (...args) => {
        assert.deepEqual(args[0], ['fence']);
        if (action === 'cut-before') process.exit(73);
        assert.equal(physical.compareExchange(...args), true); process.exit(73);
      },
    };
    await new AuthorFence(cut, 'fence', A, {requireDurableFlush:true}).claim('1', claim(2).token);
    assert.fail('Selected takeover CAS did not exit');
  }
  if (['retry-before','retry-after'].includes(action)) { assert.deepEqual(await fence.claim('1', claim(2).token), claim(2)); await check(2,1); }
  if (['mutate','following'].includes(action)) {
    const next = action === 'following' ? 3 : 2;
    assert.deepEqual(await fence.claim((next-1).toString(), claim(next).token), claim(next)); const generation = (await fence.snapshot()).generation;
    const current = fence.bind(claim(next)); assert.equal(await current.compareExchange(keys, await current.read(keys), rows(next)), true);
    assert.equal((await fence.snapshot()).generation, generation); await check(next,next);
  }
  if (['mutate','retry-before','following'].includes(action)) { physical.flush(); physical.compact(); }
} finally { physical.close(); }
console.log(`PASS JS author fencing native LSM ${action} ${mirrors}`);
